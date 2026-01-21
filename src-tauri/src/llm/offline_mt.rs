// =============================================================================
// OFFLINE_MT.RS - Offline MT Backend (translateLocally CLI)
// =============================================================================

use crate::config::OfflineMtConfig;
use crate::llm::{BackendId, LlmError, ReadyState, TranslatorBackend};
use async_trait::async_trait;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::{AppHandle, Manager};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::{debug, info};

/// Offline MT backend (translateLocally CLI)
pub struct OfflineMtBackend {
    config: OfflineMtConfig,
    binary_path: Option<PathBuf>,
    binary_source: Option<&'static str>,
    config_path_missing: bool,
}

impl OfflineMtBackend {
    pub fn new(app: AppHandle, config: OfflineMtConfig) -> Self {
        let (binary_path, binary_source, config_path_missing) =
            Self::resolve_binary_path(&app, &config);

        Self {
            config,
            binary_path,
            binary_source,
            config_path_missing,
        }
    }

    /// Create a new OfflineMtBackend without Tauri AppHandle (for HTTP server mode)
    pub fn new_standalone(config: OfflineMtConfig) -> Self {
        let (binary_path, binary_source, config_path_missing) =
            Self::resolve_binary_path_standalone(&config);

        Self {
            config,
            binary_path,
            binary_source,
            config_path_missing,
        }
    }

    /// Resolve binary path without Tauri AppHandle
    fn resolve_binary_path_standalone(
        config: &OfflineMtConfig,
    ) -> (Option<PathBuf>, Option<&'static str>, bool) {
        if let Some(path) = Self::resolve_from_config(config) {
            return (Some(path), Some("config"), false);
        }

        let config_path_missing = config.binary_path.is_some();

        if let Some(path) = Self::resolve_from_common_paths() {
            return (Some(path), Some("common path"), config_path_missing);
        }

        if let Some(path) = Self::resolve_from_path() {
            return (Some(path), Some("path"), config_path_missing);
        }

        (None, None, config_path_missing)
    }

    /// Detect binary without Tauri AppHandle (for HTTP server mode)
    pub fn detect_binary_standalone(config: &OfflineMtConfig) -> Option<(PathBuf, &'static str)> {
        let (path, source, _) = Self::resolve_binary_path_standalone(config);
        match (path, source) {
            (Some(path), Some(source)) => Some((path, source)),
            (Some(path), None) => Some((path, "unknown")),
            _ => None,
        }
    }

    fn resolve_binary_path(
        app: &AppHandle,
        config: &OfflineMtConfig,
    ) -> (Option<PathBuf>, Option<&'static str>, bool) {
        if let Some(path) = Self::resolve_from_config(config) {
            return (Some(path), Some("config"), false);
        }

        let config_path_missing = config.binary_path.is_some();

        if let Some(path) = Self::resolve_from_resources(app) {
            return (Some(path), Some("resources"), config_path_missing);
        }

        if let Some(path) = Self::resolve_from_common_paths() {
            return (Some(path), Some("common path"), config_path_missing);
        }

        if let Some(path) = Self::resolve_from_path() {
            return (Some(path), Some("path"), config_path_missing);
        }

        (None, None, config_path_missing)
    }

    fn resolve_from_config(config: &OfflineMtConfig) -> Option<PathBuf> {
        let raw_path = config.binary_path.as_ref()?;
        let path = PathBuf::from(raw_path);

        if path.is_dir() {
            return Self::find_candidate_in_dir(&path);
        }

        if path.exists() {
            return Some(path);
        }

        None
    }

    fn resolve_from_resources(app: &AppHandle) -> Option<PathBuf> {
        let resource_dir = app.path().resource_dir().ok()?;
        let candidates = vec![
            resource_dir.clone(),
            resource_dir.join("bin"),
            resource_dir.join("sidecars"),
        ];

        for dir in candidates {
            if let Some(path) = Self::find_candidate_in_dir(&dir) {
                return Some(path);
            }
        }

        None
    }

    fn resolve_from_common_paths() -> Option<PathBuf> {
        for dir in Self::common_install_dirs() {
            if let Some(path) = Self::find_candidate_in_dir(&dir) {
                return Some(path);
            }
        }
        None
    }

    fn resolve_from_path() -> Option<PathBuf> {
        let paths = env::var_os("PATH")?;
        for dir in env::split_paths(&paths) {
            if let Some(path) = Self::find_candidate_in_dir(&dir) {
                return Some(path);
            }
        }
        None
    }

    fn common_install_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        if cfg!(target_os = "windows") {
            dirs.push(PathBuf::from(r"C:\tools\translateLocally"));
            dirs.push(PathBuf::from(r"C:\translateLocally"));

            if let Ok(program_files) = env::var("ProgramFiles") {
                dirs.push(PathBuf::from(program_files).join("translateLocally"));
            }
            if let Ok(program_files_x86) = env::var("ProgramFiles(x86)") {
                dirs.push(PathBuf::from(program_files_x86).join("translateLocally"));
            }
            if let Ok(local_appdata) = env::var("LOCALAPPDATA") {
                dirs.push(PathBuf::from(local_appdata).join("translateLocally"));
            }
            if let Ok(user_profile) = env::var("USERPROFILE") {
                dirs.push(
                    PathBuf::from(user_profile)
                        .join("AppData")
                        .join("Local")
                        .join("translateLocally"),
                );
            }
        }

        dirs
    }

    fn find_candidate_in_dir(dir: &Path) -> Option<PathBuf> {
        for name in Self::binary_names() {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }

    fn binary_names() -> Vec<&'static str> {
        if cfg!(target_os = "windows") {
            vec!["translateLocally.exe", "translateLocally"]
        } else {
            vec!["translateLocally"]
        }
    }

    fn ready_notes(&self) -> String {
        if let Some(source) = self.binary_source {
            return format!("translateLocally detected via {}", source);
        }

        if self.config_path_missing {
            return "Configured binary path missing. Update translation.offlineMt.binaryPath."
                .to_string();
        }

        "translateLocally not found. Configure translation.offlineMt.binaryPath or add to PATH."
            .to_string()
    }

    async fn translate_line(
        &self,
        line: &str,
        source_language: &str,
        target_language: &str,
    ) -> Result<String, LlmError> {
        let binary = self.binary_path.as_ref().ok_or_else(|| {
            LlmError::ModelNotAvailable("translateLocally binary not available".to_string())
        })?;

        let mut command = Command::new(binary);
        command
            .arg("--source")
            .arg(source_language)
            .arg("--target")
            .arg(target_language)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = command
            .spawn()
            .map_err(|e| LlmError::ApiError(format!("Failed to start translateLocally: {}", e)))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(line.as_bytes()).await.map_err(|e| {
                LlmError::ApiError(format!("Failed to write to translateLocally: {}", e))
            })?;
            stdin.shutdown().await.map_err(|e| {
                LlmError::ApiError(format!("Failed to close translateLocally stdin: {}", e))
            })?;
        }

        let timeout_ms = self.config.timeout_ms.max(100) as u64;
        let output = timeout(Duration::from_millis(timeout_ms), child.wait_with_output())
            .await
            .map_err(|_| {
                LlmError::ApiError(format!("translateLocally timed out after {}ms", timeout_ms))
            })?
            .map_err(|e| LlmError::ApiError(format!("translateLocally failed: {}", e)))?;

        if !output.status.success() {
            let stderr = Self::sanitize_stderr(&output.stderr);
            let message = if stderr.is_empty() {
                format!("translateLocally exited with {}", output.status)
            } else {
                format!("translateLocally error: {}", stderr)
            };
            return Err(LlmError::TranslationError(message));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.trim_end_matches(['\r', '\n']).to_string())
    }

    fn sanitize_stderr(stderr: &[u8]) -> String {
        let raw = String::from_utf8_lossy(stderr);
        let trimmed = raw.trim();
        let mut sanitized = String::new();
        for ch in trimmed.chars().take(200) {
            if ch.is_control() && ch != '\n' && ch != '\r' && ch != '\t' {
                continue;
            }
            sanitized.push(ch);
        }
        sanitized
    }

    fn split_lines_preserve(text: &str) -> Vec<LineSegment> {
        let mut segments = Vec::new();
        for chunk in text.split_inclusive('\n') {
            let mut line = chunk;
            let mut ending = "";
            if let Some(stripped) = chunk.strip_suffix('\n') {
                line = stripped;
                ending = "\n";
            }
            if let Some(stripped) = line.strip_suffix('\r') {
                line = stripped;
                ending = "\r\n";
            }
            segments.push(LineSegment {
                text: line.to_string(),
                ending: ending.to_string(),
            });
        }

        if segments.is_empty() {
            segments.push(LineSegment {
                text: String::new(),
                ending: String::new(),
            });
        }

        segments
    }

    fn split_by_max_chars(input: &str, max_chars: usize) -> Vec<String> {
        if max_chars == 0 {
            return vec![input.to_string()];
        }

        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut count = 0usize;

        for ch in input.chars() {
            current.push(ch);
            count += 1;
            if count >= max_chars {
                chunks.push(current);
                current = String::new();
                count = 0;
            }
        }

        if !current.is_empty() {
            chunks.push(current);
        }

        if chunks.is_empty() {
            chunks.push(String::new());
        }

        chunks
    }

    pub fn detect_binary(
        app: &AppHandle,
        config: &OfflineMtConfig,
    ) -> Option<(PathBuf, &'static str)> {
        let (path, source, _) = Self::resolve_binary_path(app, config);
        match (path, source) {
            (Some(path), Some(source)) => Some((path, source)),
            (Some(path), None) => Some((path, "unknown")),
            _ => None,
        }
    }
}

#[async_trait]
impl TranslatorBackend for OfflineMtBackend {
    fn id(&self) -> BackendId {
        BackendId::OfflineMt
    }

    fn name(&self) -> &'static str {
        "Offline MT (translateLocally)"
    }

    fn is_available(&self) -> bool {
        self.binary_path.is_some()
    }

    fn ready_state(&self) -> ReadyState {
        if self.binary_path.is_some() {
            ReadyState::Ready
        } else {
            ReadyState::NotSupported
        }
    }

    fn notes(&self) -> String {
        self.ready_notes()
    }

    async fn translate(
        &self,
        text: &str,
        source_language: &str,
        target_language: &str,
    ) -> Result<String, LlmError> {
        if text.trim().is_empty() {
            return Ok(String::new());
        }

        info!(
            target: "translation_io",
            source_text = %text,
            source_lang = %source_language,
            target_lang = %target_language,
            backend = "offline_mt",
            "Translation request"
        );

        let max_chars = self.config.max_chunk_chars.max(1);
        let segments = Self::split_lines_preserve(text);
        let mut translated = String::new();

        for segment in segments {
            if segment.text.trim().is_empty() {
                translated.push_str(&segment.text);
                translated.push_str(&segment.ending);
                continue;
            }

            let mut line_out = String::new();
            for chunk in Self::split_by_max_chars(&segment.text, max_chars) {
                let chunk_out = self
                    .translate_line(&chunk, source_language, target_language)
                    .await?;
                line_out.push_str(&chunk_out);
            }

            translated.push_str(&line_out);
            translated.push_str(&segment.ending);
        }

        debug!(
            "Offline MT translated {} chars into {} chars",
            text.chars().count(),
            translated.chars().count()
        );
        info!(
            target: "translation_io",
            translated_text = %translated,
            source_chars = text.chars().count(),
            translated_chars = translated.chars().count(),
            "Translation response"
        );

        Ok(translated)
    }
}

struct LineSegment {
    text: String,
    ending: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_lines_preserve() {
        let segments = OfflineMtBackend::split_lines_preserve("a\r\nb\nc");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "a");
        assert_eq!(segments[0].ending, "\r\n");
        assert_eq!(segments[1].text, "b");
        assert_eq!(segments[1].ending, "\n");
        assert_eq!(segments[2].text, "c");
        assert_eq!(segments[2].ending, "");
    }

    #[test]
    fn test_split_by_max_chars() {
        let chunks = OfflineMtBackend::split_by_max_chars("abcdef", 2);
        assert_eq!(chunks, vec!["ab", "cd", "ef"]);
    }
}
