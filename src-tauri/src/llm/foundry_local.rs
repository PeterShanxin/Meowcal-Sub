// =============================================================================
// FOUNDRY_LOCAL.RS - Foundry Local Backend (OpenAI-compatible API)
// =============================================================================

use crate::config::FoundryLocalConfig;
use crate::llm::{BackendId, LlmError, ReadyState, TranslatorBackend};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

const START_ATTEMPT_COOLDOWN_MS: u64 = 30_000;
static LAST_START_ATTEMPT_MS: AtomicU64 = AtomicU64::new(0);

/// Response from /v1/models endpoint
#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelData>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelData {
    pub id: String,
    #[serde(default)]
    pub object: String,
}

/// Request to /v1/chat/completions endpoint
#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

/// Response from /v1/chat/completions endpoint
#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

/// Foundry Local backend using OpenAI-compatible API
pub struct FoundryLocalBackend {
    config: FoundryLocalConfig,
    http_client: Client,
    service_url: RwLock<Option<String>>,
    service_available: AtomicBool,
    cached_models: RwLock<Vec<String>>,
}

impl FoundryLocalBackend {
    pub fn new(config: FoundryLocalConfig) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms as u64))
            .build()
            .unwrap_or_default();

        Self {
            config,
            http_client,
            service_url: RwLock::new(None),
            service_available: AtomicBool::new(false),
            cached_models: RwLock::new(Vec::new()),
        }
        // Note: Service detection happens lazily on first is_available() call
    }

    /// Get the service URL by parsing `foundry service status` output
    pub fn get_service_url_from_cli() -> Option<String> {
        let output = Command::new("foundry")
            .args(["service", "status"])
            .output()
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse output like: "🟢 Service is Started on http://127.0.0.1:59971/, PID 29716!"
        // or: "🟢 Model management service is running on http://127.0.0.1:59971/openai/status"
        for line in stdout.lines() {
            if let Some(url) = Self::extract_base_url_from_line(line) {
                return Some(Self::normalize_service_url(&url));
            }
        }
        None
    }

    fn extract_base_url_from_line(line: &str) -> Option<String> {
        let start = line
            .find("http://")
            .or_else(|| line.find("https://"))?;
        let after = &line[start..];
        let end = after
            .find(|c: char| c.is_whitespace() || c == ',' || c == ')' || c == '!')
            .unwrap_or(after.len());
        let raw = after[..end]
            .trim_end_matches(|c: char| c == '.' || c == ',' || c == ';' || c == '!')
            .trim_end_matches('/');

        if let Some(scheme_idx) = raw.find("://") {
            let host_start = scheme_idx + 3;
            if let Some(path_idx) = raw[host_start..].find('/') {
                let end_idx = host_start + path_idx;
                return Some(raw[..end_idx].trim_end_matches('/').to_string());
            }
        }

        if raw.len() > 10 {
            return Some(raw.to_string());
        }

        None
    }

    fn normalize_service_url(raw: &str) -> String {
        let trimmed = raw.trim_end_matches('/');
        let without_path = trimmed
            .find("/openai/")
            .or_else(|| trimmed.find("/v1/"))
            .map(|idx| &trimmed[..idx])
            .unwrap_or(trimmed);

        if let Some(scheme_idx) = without_path.find("://") {
            let after_scheme = &without_path[scheme_idx + 3..];
            if let Some(path_idx) = after_scheme.find('/') {
                let host = &after_scheme[..path_idx];
                return format!("{}://{}", &without_path[..scheme_idx], host);
            }
        }

        without_path.to_string()
    }

    /// Get cached models by parsing `foundry cache list` output
    /// This is a fallback when the API doesn't return models (e.g., models cached but not running)
    pub fn get_cached_models_from_cli() -> Vec<String> {
        let output = Command::new("foundry")
            .args(["cache", "list"])
            .output();

        let Ok(output) = output else {
            debug!("Failed to run 'foundry cache list'");
            return Vec::new();
        };

        if !output.status.success() {
            debug!("'foundry cache list' returned non-zero status");
            return Vec::new();
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut models = Vec::new();

        // Parse output to extract model IDs
        // Expected format varies, but typically includes model identifiers
        for line in stdout.lines() {
            let line = line.trim();
            let lower = line.to_ascii_lowercase();
            if line.is_empty()
                || lower.starts_with("cache")
                || lower.starts_with("total")
                || lower.starts_with("models")
                || lower.starts_with("alias")
                || line.starts_with('-')
                || lower.contains("cached on device")
            {
                continue;
            }

            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.is_empty() {
                continue;
            }

            let mut candidate = tokens.last().copied().unwrap_or_default();
            if !candidate.contains(':') {
                if let Some(with_colon) = tokens.iter().rev().find(|token| token.contains(':')) {
                    candidate = with_colon;
                }
            }

            let candidate = candidate.trim_matches(|c: char| {
                c == ',' || c == ';' || c == '|' || c == '[' || c == ']'
            });

            if Self::is_probable_model_id(candidate) {
                let model_id = candidate.to_string();
                if !models.contains(&model_id) {
                    models.push(model_id);
                }
            }
        }

        debug!("Found {} cached models via CLI: {:?}", models.len(), models);
        models
    }

    /// Get model context window size from `foundry model info <model>` output
    /// Returns None if detection fails (caller should use default budget)
    pub fn get_model_context_window(model: &str) -> Option<usize> {
        let output = Command::new("foundry")
            .args(["model", "info", model])
            .output()
            .ok()?;

        if !output.status.success() {
            debug!("'foundry model info {}' returned non-zero status", model);
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse output looking for context window/length info
        // Expected formats may vary, look for common patterns:
        // "context_length: 4096" or "max_position_embeddings: 4096"
        for line in stdout.lines() {
            let line_lower = line.to_lowercase();
            if line_lower.contains("context") || line_lower.contains("position") || line_lower.contains("length") {
                // Try to extract a number
                if let Some(num) = Self::extract_number_from_line(line) {
                    if num >= 512 && num <= 131072 {
                        debug!("Detected context window for {}: {}", model, num);
                        return Some(num);
                    }
                }
            }
        }

        debug!("Could not detect context window for model {}", model);
        None
    }

    /// Extract the first reasonable number from a line
    fn extract_number_from_line(line: &str) -> Option<usize> {
        for part in line.split_whitespace() {
            let cleaned = part.trim_matches(|c: char| !c.is_ascii_digit());
            if let Ok(num) = cleaned.parse::<usize>() {
                return Some(num);
            }
        }
        None
    }

    fn is_probable_model_id(candidate: &str) -> bool {
        if candidate.is_empty() {
            return false;
        }

        if candidate.len() < 3 {
            return false;
        }

        let lower = candidate.to_ascii_lowercase();
        let blocked = [
            "models", "model", "alias", "cache", "cached", "total", "name", "id", "status",
            "size", "gb", "mb", "kb", "tb",
        ];
        if blocked.contains(&lower.as_str()) {
            return false;
        }

        let has_alpha = candidate.chars().any(|c| c.is_ascii_alphabetic());
        if !has_alpha {
            return false;
        }

        candidate.chars().any(|c| c.is_ascii_alphanumeric())
    }

    /// Try to start the Foundry service if it's not running
    fn try_start_service() -> bool {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let last_attempt = LAST_START_ATTEMPT_MS.load(Ordering::SeqCst);
        if now_ms.saturating_sub(last_attempt) < START_ATTEMPT_COOLDOWN_MS {
            debug!("Skipping Foundry Local service start (cooldown active)");
            return false;
        }
        LAST_START_ATTEMPT_MS.store(now_ms, Ordering::SeqCst);

        debug!("Attempting to start Foundry Local service");
        let mut command = Command::new("foundry");
        command
            .args(["service", "start"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        match command.spawn() {
            Ok(_child) => {
                debug!("Foundry Local service start command launched");
                // Service will be available when ready (no need to block here)
                true
            }
            Err(e) => {
                debug!("Foundry command not found or failed to execute: {}", e);
                false
            }
        }
    }

    /// Refresh service status and URL
    pub fn refresh_service_status(&self) {
        if let Some(url) = Self::get_service_url_from_cli() {
            debug!("Foundry Local service detected at {}", url);
            *self.service_url.write().unwrap() = Some(url);
            self.service_available.store(true, Ordering::SeqCst);

            // Also try to populate models from CLI if cache is empty
            let models = self.cached_models.read().unwrap();
            if models.is_empty() {
                drop(models); // Release read lock before acquiring write lock
                let cli_models = Self::get_cached_models_from_cli();
                if !cli_models.is_empty() {
                    debug!("Populated {} models from CLI during refresh", cli_models.len());
                    *self.cached_models.write().unwrap() = cli_models;
                }
            }
        } else {
            debug!("Foundry Local service not running, attempting to start");

            // Try to start the service
            if Self::try_start_service() {
                // Check again after starting
                if let Some(url) = Self::get_service_url_from_cli() {
                    debug!("Foundry Local service started at {}", url);
                    *self.service_url.write().unwrap() = Some(url);
                    self.service_available.store(true, Ordering::SeqCst);

                    // Populate models from CLI
                    let cli_models = Self::get_cached_models_from_cli();
                    if !cli_models.is_empty() {
                        debug!("Populated {} models after service start", cli_models.len());
                        *self.cached_models.write().unwrap() = cli_models;
                    }
                    return;
                }
            }

            debug!("Foundry Local service not available");
            *self.service_url.write().unwrap() = None;
            self.service_available.store(false, Ordering::SeqCst);
        }
    }

    /// Get cached service URL
    fn get_service_url(&self) -> Option<String> {
        self.service_url.read().unwrap().clone()
    }

    /// List available models from the service
    pub async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let base_url = self.get_service_url().ok_or_else(|| {
            LlmError::ApiError("Foundry Local service not running".to_string())
        })?;

        // Try /openai/v1/models first (standard Foundry Local path)
        let url = format!("{}/openai/v1/models", base_url);

        let response = self.http_client.get(&url).send().await;

        // If /openai/ path fails, try fallback to /v1/models (for compatibility)
        let response = match response {
            Ok(resp) if resp.status().is_success() => resp,
            Ok(resp) if resp.status().as_u16() == 404 => {
                debug!("Foundry Local /openai/v1/models returned 404, trying /v1/models");
                let fallback_url = format!("{}/v1/models", base_url);
                self.http_client
                    .get(&fallback_url)
                    .send()
                    .await
                    .map_err(|e| LlmError::ApiError(format!("Failed to fetch models: {}", e)))?
            }
            Ok(resp) => {
                return Err(LlmError::ApiError(format!(
                    "Models endpoint returned status {}",
                    resp.status()
                )));
            }
            Err(e) => {
                return Err(LlmError::ApiError(format!("Failed to fetch models: {}", e)));
            }
        };

        let models_response: ModelsResponse = response
            .json()
            .await
            .map_err(|e| LlmError::ApiError(format!("Failed to parse models response: {}", e)))?;

        let mut model_ids: Vec<String> = models_response.data.into_iter().map(|m| m.id).collect();

        // If API returned empty list, fall back to CLI-based discovery
        if model_ids.is_empty() {
            debug!("API returned no models, trying CLI fallback");
            model_ids = Self::get_cached_models_from_cli();
        } else if model_ids.iter().any(|id| !id.contains(':')) {
            let cli_models = Self::get_cached_models_from_cli();
            if !cli_models.is_empty() {
                let resolved = model_ids
                    .into_iter()
                    .map(|model| {
                        if model.contains(':') {
                            model
                        } else {
                            let prefix = format!("{}-", model);
                            cli_models
                                .iter()
                                .find(|entry| entry.starts_with(&prefix) && entry.contains(':'))
                                .cloned()
                                .unwrap_or(model)
                        }
                    })
                    .collect();
                model_ids = resolved;
            }
        }

        model_ids.retain(|id| !id.trim().is_empty());
        let mut seen = Vec::new();
        model_ids.retain(|id| {
            if seen.contains(id) {
                false
            } else {
                seen.push(id.clone());
                true
            }
        });

        // Cache the models
        *self.cached_models.write().unwrap() = model_ids.clone();

        Ok(model_ids)
    }

    /// Get the model to use (configured or first available)
    fn get_model(&self) -> Option<String> {
        let models = self.cached_models.read().unwrap();

        // Use configured model if set and available
        if let Some(ref model) = self.config.model {
            if Self::model_in_cache(model, &models) {
                return Some(self.resolve_model_id(model, &models));
            }
        }

        // Otherwise use first cached model
        models
            .first()
            .map(|model| self.resolve_model_id(model, &models))
    }

    fn resolve_model_id(&self, model: &str, models: &[String]) -> String {
        if model.contains(':') {
            return model.to_string();
        }

        let prefix = format!("{}-", model);
        if let Some(id) = models
            .iter()
            .find(|entry| entry.starts_with(&prefix) && entry.contains(':'))
        {
            return id.clone();
        }

        model.to_string()
    }

    fn model_in_cache(model: &str, models: &[String]) -> bool {
        models.iter().any(|m| m == model)
            || (!model.contains(':') && models.iter().any(|m| m.starts_with(&format!("{}-", model))))
    }

    /// Check if service is healthy by probing the models endpoint
    pub async fn check_health(&self) -> bool {
        if let Some(url) = self.get_service_url() {
            let models_url = format!("{}/openai/v1/models", url);
            if let Ok(resp) = self.http_client.get(&models_url).send().await {
                return resp.status().is_success();
            }
        }
        false
    }
}

#[async_trait]
impl TranslatorBackend for FoundryLocalBackend {
    fn id(&self) -> BackendId {
        BackendId::FoundryLocal
    }

    fn name(&self) -> &'static str {
        "Foundry Local"
    }

    fn is_available(&self) -> bool {
        // Refresh status if currently unavailable
        if !self.service_available.load(Ordering::SeqCst) {
            self.refresh_service_status();
        }
        self.service_available.load(Ordering::SeqCst)
    }

    fn ready_state(&self) -> ReadyState {
        if !self.is_available() {
            return ReadyState::NotReady;
        }

        let models = self.cached_models.read().unwrap();
        if let Some(ref model) = self.config.model {
            if Self::model_in_cache(model, &models) {
                ReadyState::Ready
            } else {
                ReadyState::NotReady
            }
        } else if models.is_empty() {
            ReadyState::NotReady
        } else {
            ReadyState::Ready
        }
    }

    fn notes(&self) -> String {
        if let Some(url) = self.get_service_url() {
            if let Some(model) = self.get_model() {
                format!("Service at {}. Using model: {}. If translation fails, run: foundry model run {}", url, model, model)
            } else {
                format!("Service at {}. No models cached - run: foundry model run <model>", url)
            }
        } else {
            "Foundry Local not running. If installed, run: foundry service start. If not installed: winget install Microsoft.FoundryLocal".to_string()
        }
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

        let base_url = self.get_service_url().ok_or_else(|| {
            LlmError::ApiError("Foundry Local service not running".to_string())
        })?;

        let model = self.get_model().ok_or_else(|| {
            LlmError::ModelNotAvailable("No model available. Run: foundry model run <model>".to_string())
        })?;

        let url = format!("{}/openai/v1/chat/completions", base_url);

        let system_prompt = format!(
            "You are a translator. Translate the following text from {} to {}. \
             Output ONLY the translated text, nothing else. No explanations, no quotes, no formatting.",
            source_language, target_language
        );

        let request = ChatCompletionRequest {
            model,
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: text.to_string(),
                },
            ],
            temperature: 0.3, // Low temperature for consistent translations
            max_tokens: 2048,
        };

        debug!("Sending translation request to Foundry Local: {}", url);

        let response = self
            .http_client
            .post(&url)
            .json(&request)
            .send()
            .await;

        let response = match response {
            Ok(resp) if resp.status().is_success() => resp,
            Ok(resp) if resp.status().as_u16() == 404 => {
                debug!("Foundry Local /openai/v1/chat/completions returned 404, trying /v1/chat/completions");
                let fallback_url = format!("{}/v1/chat/completions", base_url);
                self.http_client
                    .post(&fallback_url)
                    .json(&request)
                    .send()
                    .await
                    .map_err(|e| {
                        warn!("Foundry Local request failed: {}", e);
                        LlmError::ApiError(format!("Request failed: {}", e))
                    })?
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                warn!("Foundry Local returned error {}: {}", status, body);
                return Err(LlmError::ApiError(format!(
                    "API error {}: {}",
                    status,
                    body.chars().take(200).collect::<String>()
                )));
            }
            Err(e) => {
                warn!("Foundry Local request failed: {}", e);
                return Err(LlmError::ApiError(format!("Request failed: {}", e)));
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!("Foundry Local returned error {}: {}", status, body);
            return Err(LlmError::ApiError(format!(
                "API error {}: {}",
                status,
                body.chars().take(200).collect::<String>()
            )));
        }

        let completion: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| LlmError::ApiError(format!("Failed to parse response: {}", e)))?;

        let translated = completion
            .choices
            .first()
            .map(|c| c.message.content.trim().to_string())
            .unwrap_or_default();

        debug!(
            "Foundry Local translated {} chars to {} chars",
            text.chars().count(),
            translated.chars().count()
        );

        Ok(translated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_service_url() {
        // This would require mocking the CLI output
        // For now, just verify the struct can be created
        let config = FoundryLocalConfig::default();
        let _backend = FoundryLocalBackend::new(config);
    }
}
