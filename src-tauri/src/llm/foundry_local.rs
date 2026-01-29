// =============================================================================
// FOUNDRY_LOCAL.RS - Foundry Local Backend (OpenAI-compatible API)
// =============================================================================

use crate::config::FoundryLocalConfig;
use crate::llm::{
    build_subtitle_translation_prompt, BackendId, FoundryLocalPhase, LlmError, PromptRouterOptions,
    ReadyState, TranslatorBackend,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

const START_ATTEMPT_COOLDOWN_MS: u64 = 30_000;
static LAST_START_ATTEMPT_MS: AtomicU64 = AtomicU64::new(0);

/// Cache probe success for this duration (milliseconds)
const PROBE_CACHE_DURATION_MS: u64 = 300_000;
/// Fast probe timeout for "Refresh Status" button (milliseconds)
pub const FAST_PROBE_TIMEOUT_MS: u64 = 2_000;
/// Slow warmup probe timeout for "Prepare Foundry" button (milliseconds)
pub const SLOW_PROBE_TIMEOUT_MS: u64 = 25_000;

const API_NAMESPACE_UNKNOWN: u8 = 0;
const API_NAMESPACE_OPENAI: u8 = 1;
const API_NAMESPACE_V1: u8 = 2;

// Cache probe success across backend instances. Diagnostics frequently create fresh
// FoundryLocalBackend values, so per-instance caches won't persist long enough.
struct ProbeCache {
    last_success_ms: AtomicU64,
    last_attempt_ms: AtomicU64,
    last_result: AtomicU8,
    last_error: RwLock<Option<String>>,
    last_model: RwLock<Option<String>>,
    last_service_url: RwLock<Option<String>>,
}

static PROBE_CACHE: OnceLock<ProbeCache> = OnceLock::new();

const PROBE_RESULT_NONE: u8 = 0;
const PROBE_RESULT_SUCCESS: u8 = 1;
const PROBE_RESULT_TIMEOUT: u8 = 2;
const PROBE_RESULT_ERROR: u8 = 3;

fn probe_cache() -> &'static ProbeCache {
    PROBE_CACHE.get_or_init(|| ProbeCache {
        last_success_ms: AtomicU64::new(0),
        last_attempt_ms: AtomicU64::new(0),
        last_result: AtomicU8::new(PROBE_RESULT_NONE),
        last_error: RwLock::new(None),
        last_model: RwLock::new(None),
        last_service_url: RwLock::new(None),
    })
}

fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

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
    api_namespace: AtomicU8,
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
            api_namespace: AtomicU8::new(API_NAMESPACE_UNKNOWN),
        }
        // Note: Service detection happens lazily on first is_available() call
    }

    fn mark_service_unavailable(&self) {
        *self.service_url.write().unwrap() = None;
        self.service_available.store(false, Ordering::SeqCst);
        self.api_namespace
            .store(API_NAMESPACE_UNKNOWN, Ordering::SeqCst);
        self.invalidate_probe_cache();
    }

    fn preferred_api_namespace(&self) -> u8 {
        match self.api_namespace.load(Ordering::SeqCst) {
            API_NAMESPACE_V1 => API_NAMESPACE_V1,
            API_NAMESPACE_OPENAI => API_NAMESPACE_OPENAI,
            _ => API_NAMESPACE_OPENAI, // Default to the Foundry Local /openai namespace when unknown.
        }
    }

    fn api_url_for(&self, base_url: &str, api_namespace: u8, endpoint: &str) -> String {
        match api_namespace {
            API_NAMESPACE_V1 => format!("{}/v1/{}", base_url, endpoint),
            _ => format!("{}/openai/v1/{}", base_url, endpoint),
        }
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
        let start = line.find("http://").or_else(|| line.find("https://"))?;
        let after = &line[start..];
        let end = after
            .find(|c: char| c.is_whitespace() || c == ',' || c == ')' || c == '!')
            .unwrap_or(after.len());
        let raw = after[..end]
            .trim_end_matches(['.', ',', ';', '!'])
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
        let output = Command::new("foundry").args(["cache", "list"]).output();

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

            let candidate = candidate
                .trim_matches(|c: char| c == ',' || c == ';' || c == '|' || c == '[' || c == ']');

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
        let context_keys = [
            "context_length",
            "context length",
            "context_window",
            "context window",
            "context:",
            "context =",
            "max_position_embeddings",
            "max position embeddings",
            "max_position",
            "max position",
            "n_ctx",
        ];
        for line in stdout.lines() {
            let line_lower = line.to_lowercase();
            if !context_keys.iter().any(|key| line_lower.contains(key)) {
                continue;
            }

            if let Some(num) = Self::extract_number_from_line(line) {
                if (512..=131_072).contains(&num) {
                    debug!("Detected context window for {}: {}", model, num);
                    return Some(num);
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
            "models", "model", "alias", "cache", "cached", "total", "name", "id", "status", "size",
            "gb", "mb", "kb", "tb",
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

    /// Refresh service status and URL (read-only; does NOT start the service).
    pub fn refresh_service_status(&self) {
        let previous_url = self.service_url.read().unwrap().clone();

        if let Some(url) = Self::get_service_url_from_cli() {
            debug!("Foundry Local service detected at {}", url);
            if previous_url.as_deref() != Some(&url) {
                // Service restarted (port changed) - cached probe is no longer valid.
                self.invalidate_probe_cache();
            }
            *self.service_url.write().unwrap() = Some(url);
            self.service_available.store(true, Ordering::SeqCst);

            // Also try to populate models from CLI if cache is empty
            let models = self.cached_models.read().unwrap();
            if models.is_empty() {
                drop(models); // Release read lock before acquiring write lock
                let cli_models = Self::get_cached_models_from_cli();
                if !cli_models.is_empty() {
                    debug!(
                        "Populated {} models from CLI during refresh",
                        cli_models.len()
                    );
                    *self.cached_models.write().unwrap() = cli_models;
                }
            }
        } else {
            if previous_url.is_some() {
                self.invalidate_probe_cache();
            }

            debug!("Foundry Local service not running");
            *self.service_url.write().unwrap() = None;
            self.service_available.store(false, Ordering::SeqCst);
        }
    }

    /// Ensure the Foundry Local service is running (may start the service).
    ///
    /// Use this for the explicit "Prepare Foundry" flow - regular status checks should
    /// call `refresh_service_status()` which is read-only.
    pub fn ensure_service_running(&self) -> bool {
        if Self::get_service_url_from_cli().is_some() {
            self.refresh_service_status();
            return true;
        }

        debug!("Foundry Local service not running, attempting to start");
        if !Self::try_start_service() {
            self.refresh_service_status();
            return false;
        }

        // The start command returns quickly; poll for the service URL for a short time.
        let start = std::time::Instant::now();
        let deadline = Duration::from_secs(5);
        while start.elapsed() < deadline {
            if Self::get_service_url_from_cli().is_some() {
                self.refresh_service_status();
                return true;
            }
            std::thread::sleep(Duration::from_millis(250));
        }

        self.refresh_service_status();
        false
    }

    /// Get cached service URL
    fn get_service_url(&self) -> Option<String> {
        self.service_url.read().unwrap().clone()
    }

    async fn send_chat_completion(
        &self,
        base_url: &str,
        request: &ChatCompletionRequest,
    ) -> Result<reqwest::Response, LlmError> {
        let preferred_namespace = self.preferred_api_namespace();
        let url = self.api_url_for(base_url, preferred_namespace, "chat/completions");

        let response = self.http_client.post(&url).json(request).send().await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                self.api_namespace
                    .store(preferred_namespace, Ordering::SeqCst);
                // Any successful chat completion implies the model is "ready to talk".
                self.record_probe_success();
                Ok(resp)
            }
            Ok(resp) if resp.status().as_u16() == 404 => {
                let fallback_namespace = if preferred_namespace == API_NAMESPACE_OPENAI {
                    API_NAMESPACE_V1
                } else {
                    API_NAMESPACE_OPENAI
                };
                let fallback_url =
                    self.api_url_for(base_url, fallback_namespace, "chat/completions");
                debug!(
                    "Foundry Local chat completions returned 404 for {}, trying {}",
                    url, fallback_url
                );
                let resp = self
                    .http_client
                    .post(&fallback_url)
                    .json(request)
                    .send()
                    .await
                    .map_err(|e| {
                        warn!("Foundry Local request failed: {}", e);
                        LlmError::ApiError(format!("Request failed: {}", e))
                    })?;

                if resp.status().is_success() {
                    self.api_namespace
                        .store(fallback_namespace, Ordering::SeqCst);
                    self.record_probe_success();
                    Ok(resp)
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    warn!("Foundry Local returned error {}: {}", status, body);
                    Err(LlmError::ApiError(format!(
                        "API error {}: {}",
                        status,
                        body.chars().take(200).collect::<String>()
                    )))
                }
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                warn!("Foundry Local returned error {}: {}", status, body);
                Err(LlmError::ApiError(format!(
                    "API error {}: {}",
                    status,
                    body.chars().take(200).collect::<String>()
                )))
            }
            Err(e) => {
                warn!("Foundry Local request failed: {}", e);
                Err(LlmError::ApiError(format!("Request failed: {}", e)))
            }
        }
    }

    /// List available models from the service
    pub async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let base_url = self
            .get_service_url()
            .ok_or_else(|| LlmError::ApiError("Foundry Local service not running".to_string()))?;

        let preferred_namespace = self.preferred_api_namespace();
        let url = self.api_url_for(&base_url, preferred_namespace, "models");

        let response = self.http_client.get(&url).send().await;

        // If /openai/ path fails, try fallback to /v1/models (for compatibility)
        let response = match response {
            Ok(resp) if resp.status().is_success() => {
                self.api_namespace
                    .store(preferred_namespace, Ordering::SeqCst);
                resp
            }
            Ok(resp) if resp.status().as_u16() == 404 => {
                let fallback_namespace = if preferred_namespace == API_NAMESPACE_OPENAI {
                    API_NAMESPACE_V1
                } else {
                    API_NAMESPACE_OPENAI
                };
                let fallback_url = self.api_url_for(&base_url, fallback_namespace, "models");
                debug!(
                    "Foundry Local models returned 404 for {}, trying {}",
                    url, fallback_url
                );
                let resp = self
                    .http_client
                    .get(&fallback_url)
                    .send()
                    .await
                    .map_err(|e| LlmError::ApiError(format!("Failed to fetch models: {}", e)))?;

                if resp.status().is_success() {
                    self.api_namespace
                        .store(fallback_namespace, Ordering::SeqCst);
                }

                resp
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
            || (!model.contains(':')
                && models.iter().any(|m| m.starts_with(&format!("{}-", model))))
    }

    /// Check if service is healthy by probing the models endpoint
    pub async fn check_health(&self) -> bool {
        if let Some(url) = self.get_service_url() {
            let preferred_namespace = self.preferred_api_namespace();
            let models_url = self.api_url_for(&url, preferred_namespace, "models");

            if let Ok(resp) = self.http_client.get(&models_url).send().await {
                if resp.status().is_success() {
                    self.api_namespace
                        .store(preferred_namespace, Ordering::SeqCst);
                    return true;
                }

                if resp.status().as_u16() == 404 {
                    let fallback_namespace = if preferred_namespace == API_NAMESPACE_OPENAI {
                        API_NAMESPACE_V1
                    } else {
                        API_NAMESPACE_OPENAI
                    };
                    self.api_namespace
                        .store(fallback_namespace, Ordering::SeqCst);
                }
            }
        }
        false
    }

    // =========================================================================
    // CHAT PROBE METHODS - "Ready to Talk" verification
    // =========================================================================

    /// Check if the Foundry CLI is available on this system
    pub fn is_cli_available() -> bool {
        Command::new("foundry")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Check if the probe cache is still valid (recent successful probe)
    pub fn is_probe_cache_valid(&self) -> bool {
        let cache = probe_cache();
        let last_success = cache.last_success_ms.load(Ordering::SeqCst);
        if last_success == 0 {
            return false;
        }

        let now_ms = epoch_ms();
        if now_ms.saturating_sub(last_success) >= PROBE_CACHE_DURATION_MS {
            return false;
        }

        let current_url = match self.get_service_url() {
            Some(url) => url,
            None => return false,
        };
        let current_model = match self.get_model() {
            Some(model) => model,
            None => return false,
        };

        let cached_url = cache.last_service_url.read().unwrap().clone();
        let cached_model = cache.last_model.read().unwrap().clone();

        cached_url.as_deref() == Some(current_url.as_str())
            && cached_model.as_deref() == Some(current_model.as_str())
    }

    /// Record a successful probe (updates cache timestamp)
    pub fn record_probe_success(&self) {
        let (Some(url), Some(model)) = (self.get_service_url(), self.get_model()) else {
            self.invalidate_probe_cache();
            return;
        };

        let cache = probe_cache();
        cache.last_success_ms.store(epoch_ms(), Ordering::SeqCst);
        cache.last_attempt_ms.store(epoch_ms(), Ordering::SeqCst);
        cache
            .last_result
            .store(PROBE_RESULT_SUCCESS, Ordering::SeqCst);
        *cache.last_error.write().unwrap() = None;
        *cache.last_service_url.write().unwrap() = Some(url);
        *cache.last_model.write().unwrap() = Some(model);
    }

    fn record_probe_timeout(&self) {
        let (Some(url), Some(model)) = (self.get_service_url(), self.get_model()) else {
            self.invalidate_probe_cache();
            return;
        };

        let cache = probe_cache();
        cache.last_attempt_ms.store(epoch_ms(), Ordering::SeqCst);
        cache
            .last_result
            .store(PROBE_RESULT_TIMEOUT, Ordering::SeqCst);
        *cache.last_error.write().unwrap() = None;
        *cache.last_service_url.write().unwrap() = Some(url);
        *cache.last_model.write().unwrap() = Some(model);
    }

    fn record_probe_error(&self, message: String) {
        let (Some(url), Some(model)) = (self.get_service_url(), self.get_model()) else {
            self.invalidate_probe_cache();
            return;
        };

        let cache = probe_cache();
        cache.last_attempt_ms.store(epoch_ms(), Ordering::SeqCst);
        cache
            .last_result
            .store(PROBE_RESULT_ERROR, Ordering::SeqCst);
        *cache.last_error.write().unwrap() = Some(message);
        *cache.last_service_url.write().unwrap() = Some(url);
        *cache.last_model.write().unwrap() = Some(model);
    }

    /// Invalidate the probe cache (call when model selection changes)
    pub fn invalidate_probe_cache(&self) {
        let cache = probe_cache();
        cache.last_success_ms.store(0, Ordering::SeqCst);
        cache.last_attempt_ms.store(0, Ordering::SeqCst);
        cache.last_result.store(PROBE_RESULT_NONE, Ordering::SeqCst);
        *cache.last_error.write().unwrap() = None;
        *cache.last_service_url.write().unwrap() = None;
        *cache.last_model.write().unwrap() = None;
    }

    fn is_last_probe_for_current_target(&self) -> bool {
        let cache = probe_cache();

        let current_url = match self.get_service_url() {
            Some(url) => url,
            None => return false,
        };
        let current_model = match self.get_model() {
            Some(model) => model,
            None => return false,
        };

        let cached_url = cache.last_service_url.read().unwrap().clone();
        let cached_model = cache.last_model.read().unwrap().clone();

        cached_url.as_deref() == Some(current_url.as_str())
            && cached_model.as_deref() == Some(current_model.as_str())
    }

    fn recent_probe_result_for_current_target(&self) -> Option<(u8, u64, Option<String>)> {
        if !self.is_last_probe_for_current_target() {
            return None;
        }

        let cache = probe_cache();
        let attempt_ms = cache.last_attempt_ms.load(Ordering::SeqCst);
        if attempt_ms == 0 {
            return None;
        }

        // Only surface "Preparing/Error" for a short window; after that we go back to Unchecked.
        let age_ms = epoch_ms().saturating_sub(attempt_ms);
        if age_ms > 30_000 {
            return None;
        }

        let kind = cache.last_result.load(Ordering::SeqCst);
        let error = cache.last_error.read().unwrap().clone();
        Some((kind, age_ms, error))
    }

    /// Probe chat completions endpoint with a minimal request.
    ///
    /// This sends a tiny chat request (max_tokens=1) to verify that the model
    /// is actually ready to respond. This catches NPU warmup delays that would
    /// otherwise cause translation timeouts.
    ///
    /// Returns Ok(true) on success, Ok(false) on timeout, Err on other errors.
    pub async fn probe_chat_completions(&self, timeout_ms: u64) -> Result<bool, LlmError> {
        async fn attempt_probe(
            backend: &FoundryLocalBackend,
            probe_client: &Client,
            base_url: &str,
            request: &ChatCompletionRequest,
            preferred_namespace: u8,
            timeout_ms: u64,
        ) -> Result<bool, LlmError> {
            let url = backend.api_url_for(base_url, preferred_namespace, "chat/completions");
            debug!(
                "Probing Foundry Local chat completions (timeout={}ms): {}",
                timeout_ms, url
            );

            let response = probe_client.post(&url).json(request).send().await;
            match response {
                Ok(resp) if resp.status().is_success() => {
                    backend
                        .api_namespace
                        .store(preferred_namespace, Ordering::SeqCst);
                    backend.record_probe_success();
                    debug!("Foundry Local probe succeeded");
                    Ok(true)
                }
                Ok(resp) if resp.status().as_u16() == 404 => {
                    // Try fallback namespace
                    let fallback_namespace = if preferred_namespace == API_NAMESPACE_OPENAI {
                        API_NAMESPACE_V1
                    } else {
                        API_NAMESPACE_OPENAI
                    };
                    let fallback_url =
                        backend.api_url_for(base_url, fallback_namespace, "chat/completions");
                    debug!(
                        "Foundry Local probe returned 404, trying fallback: {}",
                        fallback_url
                    );

                    let fallback_response =
                        probe_client.post(&fallback_url).json(request).send().await;

                    match fallback_response {
                        Ok(resp) if resp.status().is_success() => {
                            backend
                                .api_namespace
                                .store(fallback_namespace, Ordering::SeqCst);
                            backend.record_probe_success();
                            debug!("Foundry Local probe succeeded (fallback namespace)");
                            Ok(true)
                        }
                        Ok(resp) => {
                            let status = resp.status();
                            debug!("Foundry Local probe failed: HTTP {}", status);
                            let msg = format!("Probe failed: HTTP {}", status);
                            backend.record_probe_error(msg.clone());
                            Err(LlmError::ApiError(msg))
                        }
                        Err(e) if e.is_timeout() => {
                            debug!("Foundry Local probe timed out (fallback)");
                            backend.record_probe_timeout();
                            Ok(false) // Timeout = preparing
                        }
                        Err(e) => {
                            debug!("Foundry Local probe error (fallback): {:?}", e);
                            let msg = format!("Probe failed: {}", e);
                            backend.record_probe_error(msg.clone());
                            Err(LlmError::ApiError(msg))
                        }
                    }
                }
                Ok(resp) => {
                    let status = resp.status();
                    debug!("Foundry Local probe failed: HTTP {}", status);
                    let msg = format!("Probe failed: HTTP {}", status);
                    backend.record_probe_error(msg.clone());
                    Err(LlmError::ApiError(msg))
                }
                Err(e) if e.is_timeout() => {
                    debug!("Foundry Local probe timed out");
                    backend.record_probe_timeout();
                    Ok(false) // Timeout = preparing (model warming up)
                }
                Err(e) => {
                    debug!("Foundry Local probe error: {:?}", e);
                    let msg = format!("Probe failed: {}", e);
                    backend.record_probe_error(msg.clone());
                    Err(LlmError::ApiError(msg))
                }
            }
        }

        let base_url = self
            .get_service_url()
            .ok_or_else(|| LlmError::ApiError("Foundry Local service not running".to_string()))?;

        let model = self
            .get_model()
            .ok_or_else(|| LlmError::ModelNotAvailable("No model available".to_string()))?;

        let request = ChatCompletionRequest {
            model,
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            }],
            temperature: 0.0,
            max_tokens: 1,
        };

        let probe_client = Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .unwrap_or_default();

        let preferred_namespace = self.preferred_api_namespace();
        match attempt_probe(
            self,
            &probe_client,
            &base_url,
            &request,
            preferred_namespace,
            timeout_ms,
        )
        .await
        {
            Ok(v) => Ok(v),
            Err(err) => {
                // Foundry Local can restart and change its port; retry once with a refreshed URL.
                self.mark_service_unavailable();
                self.refresh_service_status();
                if let Some(refreshed_url) = self.get_service_url() {
                    if refreshed_url != base_url {
                        debug!(
                            "Retrying Foundry Local probe after refreshing service URL: {} -> {}",
                            base_url, refreshed_url
                        );
                        let retry_ns = self.preferred_api_namespace();
                        return attempt_probe(
                            self,
                            &probe_client,
                            &refreshed_url,
                            &request,
                            retry_ns,
                            timeout_ms,
                        )
                        .await;
                    }
                }
                Err(err)
            }
        }
    }

    /// Determine the current phase based on system state and optional probe result.
    ///
    /// If `probe_result` is None, no probe was performed (fast status check).
    /// If `probe_result` is Some, it contains the result of a probe attempt.
    pub fn determine_phase(
        &self,
        probe_result: Option<Result<bool, LlmError>>,
    ) -> FoundryLocalPhase {
        // Check if CLI is available
        if !Self::is_cli_available() {
            return FoundryLocalPhase::NotInstalled;
        }

        // Check if service is running
        if !self.service_available.load(Ordering::SeqCst) {
            return FoundryLocalPhase::NotRunning;
        }

        // Check if models are cached
        let models = self.cached_models.read().unwrap();
        if models.is_empty() {
            return FoundryLocalPhase::NoModels;
        }
        drop(models);

        // If we have a probe result, use it
        if let Some(result) = probe_result {
            match result {
                Ok(true) => {
                    self.record_probe_success();
                    return FoundryLocalPhase::Ready;
                }
                Ok(false) => {
                    // Timeout = model warming up
                    return FoundryLocalPhase::Preparing;
                }
                Err(_) => {
                    return FoundryLocalPhase::Error;
                }
            }
        }

        // No probe performed - check cache
        if self.is_probe_cache_valid() {
            return FoundryLocalPhase::Ready;
        }

        // If we recently tried to probe and it timed out or errored, surface that.
        if let Some((kind, _age_ms, _error)) = self.recent_probe_result_for_current_target() {
            match kind {
                PROBE_RESULT_TIMEOUT => return FoundryLocalPhase::Preparing,
                PROBE_RESULT_ERROR => return FoundryLocalPhase::Error,
                _ => {}
            }
        }

        // Service running with models but no recent probe - not checked yet.
        // (caller should perform a probe if they want accurate status)
        FoundryLocalPhase::Unchecked
    }

    /// Get the current phase (performs CLI checks but no network probe).
    ///
    /// For accurate status including warmup detection, call `probe_chat_completions()`
    /// first and pass the result to `determine_phase()`.
    pub fn phase(&self) -> FoundryLocalPhase {
        self.determine_phase(None)
    }

    /// Summarize translation history into a compact memory block
    pub async fn summarize_context(
        &self,
        history: &[String], // source-only subtitle lines
    ) -> Result<String, LlmError> {
        if history.is_empty() {
            return Ok(String::new());
        }

        let base_url = self
            .get_service_url()
            .ok_or_else(|| LlmError::ApiError("Foundry Local service not running".to_string()))?;

        let model = self
            .get_model()
            .ok_or_else(|| LlmError::ModelNotAvailable("No model available".to_string()))?;

        let preferred_namespace = self.preferred_api_namespace();
        let url = self.api_url_for(&base_url, preferred_namespace, "chat/completions");

        // Build history text
        let history_text: String = history
            .iter()
            .map(|line| format!("\"{}\"", line))
            .collect::<Vec<_>>()
            .join("\n");

        let system_prompt =
            "You are a helpful assistant that extracts key information from subtitle lines. \
            Given a list of subtitle lines (source text only), extract and summarize:\n\
            1. Character names / proper nouns (as they appear)\n\
            2. Genre/tone of the content\n\
            3. Any recurring terms\n\n\
            Be extremely concise. Output in this format:\n\
            Genre: [detected genre]. Names: [names]. Terms: [key terms]\n\
            If you can't determine something, omit it. Maximum 80 words.";

        let request = ChatCompletionRequest {
            model,
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: format!("Summarize these subtitle lines:\n{}", history_text),
                },
            ],
            temperature: 0.3,
            max_tokens: 150,
        };

        debug!("Sending summarization request to Foundry Local: {}", url);

        let response = self.http_client.post(&url).json(&request).send().await;

        let response = match response {
            Ok(resp) if resp.status().is_success() => {
                self.api_namespace
                    .store(preferred_namespace, Ordering::SeqCst);
                resp
            }
            Ok(resp) if resp.status().as_u16() == 404 => {
                let fallback_namespace = if preferred_namespace == API_NAMESPACE_OPENAI {
                    API_NAMESPACE_V1
                } else {
                    API_NAMESPACE_OPENAI
                };
                let fallback_url =
                    self.api_url_for(&base_url, fallback_namespace, "chat/completions");
                debug!(
                    "Foundry Local chat completions returned 404 for {}, trying {}",
                    url, fallback_url
                );
                let resp = self
                    .http_client
                    .post(&fallback_url)
                    .json(&request)
                    .send()
                    .await
                    .map_err(|e| {
                        warn!("Foundry Local summarization request failed: {}", e);
                        LlmError::ApiError(format!("Summarization request failed: {}", e))
                    })?;

                if resp.status().is_success() {
                    self.api_namespace
                        .store(fallback_namespace, Ordering::SeqCst);
                }

                resp
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                warn!(
                    "Foundry Local summarization returned error {}: {}",
                    status, body
                );
                return Err(LlmError::ApiError(format!(
                    "Summarization API error {}: {}",
                    status,
                    body.chars().take(200).collect::<String>()
                )));
            }
            Err(e) => {
                warn!("Foundry Local summarization request failed: {}", e);
                return Err(LlmError::ApiError(format!(
                    "Summarization request failed: {}",
                    e
                )));
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!(
                "Foundry Local summarization returned error {}: {}",
                status, body
            );
            return Err(LlmError::ApiError(format!(
                "Summarization API error {}: {}",
                status,
                body.chars().take(200).collect::<String>()
            )));
        }

        let completion: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| LlmError::ApiError(format!("Failed to parse response: {}", e)))?;

        let summary = completion
            .choices
            .first()
            .map(|c| c.message.content.trim().to_string())
            .unwrap_or_default();

        debug!("Context summarization produced {} chars", summary.len());
        Ok(summary)
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

        let model_ready = {
            let models = self.cached_models.read().unwrap();
            if let Some(ref model) = self.config.model {
                Self::model_in_cache(model, &models)
            } else {
                !models.is_empty()
            }
        };

        if !model_ready {
            return ReadyState::NotReady;
        }

        if self.is_probe_cache_valid() {
            ReadyState::Ready
        } else {
            ReadyState::NotReady
        }
    }

    fn notes(&self) -> String {
        if let Some(url) = self.get_service_url() {
            let models = self.cached_models.read().unwrap();

            if let Some(ref configured) = self.config.model {
                if Self::model_in_cache(configured, &models) {
                    let resolved = self.resolve_model_id(configured, &models);
                    format!("Service at {}. Selected model: {}.", url, resolved)
                } else if models.is_empty() {
                    format!(
                        "Service at {}. Selected model: {} (not cached). No models cached - run: foundry model run {}",
                        url,
                        configured,
                        configured
                    )
                } else {
                    format!(
                        "Service at {}. Selected model: {} (not cached). Run: foundry model run {}",
                        url, configured, configured
                    )
                }
            } else if let Some(model) = models.first() {
                let resolved = self.resolve_model_id(model, &models);
                format!(
                    "Service at {}. Auto-selected model: {}. To pick a different model, choose one and Save Settings.",
                    url, resolved
                )
            } else {
                format!(
                    "Service at {}. No models cached - run: foundry model run <model>",
                    url
                )
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
        self.translate_with_context(text, source_language, target_language, None)
            .await
    }

    async fn translate_with_context(
        &self,
        text: &str,
        source_language: &str,
        target_language: &str,
        context: Option<&str>,
    ) -> Result<String, LlmError> {
        self.translate_with_context_options(text, source_language, target_language, context, None)
            .await
    }

    async fn translate_with_context_options(
        &self,
        text: &str,
        source_language: &str,
        target_language: &str,
        context: Option<&str>,
        options: Option<PromptRouterOptions>,
    ) -> Result<String, LlmError> {
        if text.trim().is_empty() {
            return Ok(String::new());
        }

        let base_url = match self.get_service_url() {
            Some(url) => url,
            None => {
                self.refresh_service_status();
                self.get_service_url().ok_or_else(|| {
                    LlmError::ApiError("Foundry Local service not running".to_string())
                })?
            }
        };

        let model = self.get_model().ok_or_else(|| {
            LlmError::ModelNotAvailable(
                "No model available. Run: foundry model run <model>".to_string(),
            )
        })?;

        let prompt_options = options.unwrap_or(PromptRouterOptions {
            enable_context: context.is_some(),
            max_context_chars: 600,
            max_source_chars: 300,
        });

        let built = build_subtitle_translation_prompt(
            text,
            Some(source_language).filter(|value| !value.trim().is_empty()),
            target_language,
            context,
            prompt_options,
        );

        let prompt = match built {
            Some(built) => built.prompt,
            None => return Ok(String::new()),
        };

        let request = ChatCompletionRequest {
            model,
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt,
            }],
            temperature: 0.2, // Lower temperature for subtitle consistency
            max_tokens: 512,
        };

        let current_namespace = self.preferred_api_namespace();
        let url = self.api_url_for(&base_url, current_namespace, "chat/completions");
        debug!("Sending translation request to Foundry Local: {}", url);
        info!(
            target: "translation_io",
            source_text = %text,
            source_lang = %source_language,
            target_lang = %target_language,
            model = %request.model,
            "Translation request"
        );

        let response = match self.send_chat_completion(&base_url, &request).await {
            Ok(resp) => resp,
            Err(err) => {
                // Foundry Local's port can change when the service restarts. If we see a request
                // failure, clear the cached URL and refresh from CLI once, then retry.
                self.mark_service_unavailable();
                self.refresh_service_status();

                if let Some(refreshed_url) = self.get_service_url() {
                    if refreshed_url != base_url {
                        let retry_namespace = self.preferred_api_namespace();
                        let retry_url =
                            self.api_url_for(&refreshed_url, retry_namespace, "chat/completions");
                        debug!(
                            "Retrying Foundry Local request after refreshing service URL: {}",
                            retry_url
                        );
                    }

                    self.send_chat_completion(&refreshed_url, &request)
                        .await
                        .map_err(|retry_err| {
                            warn!(
                                "Foundry Local request failed after service refresh: {}",
                                retry_err
                            );
                            retry_err
                        })?
                } else {
                    return Err(err);
                }
            }
        };

        let completion: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| LlmError::ApiError(format!("Failed to parse response: {}", e)))?;

        let translated = completion
            .choices
            .first()
            .map(|c| c.message.content.trim().to_string())
            .unwrap_or_default();
        let translated = sanitize_subtitle_translation_output(&translated);

        debug!(
            "Foundry Local translated {} chars to {} chars",
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

fn sanitize_subtitle_translation_output(output: &str) -> String {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Strip common wrapping quotes.
    let trimmed = trimmed
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('“')
        .trim_matches('”')
        .trim_matches('`')
        .trim();

    // If the model returned a labelled response, strip the label and keep the content.
    let mut collected: Vec<String> = Vec::new();
    let mut started = false;
    for raw_line in trimmed.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            if started {
                break;
            }
            continue;
        }

        let lower = line.to_ascii_lowercase();
        let is_header = lower.starts_with("translation")
            || lower.starts_with("translated")
            || lower.starts_with("output")
            || line.starts_with("翻译")
            || line.starts_with("译文");
        let is_expl = lower.starts_with("explanation")
            || lower.starts_with("note")
            || line.starts_with("解释")
            || line.starts_with("说明");

        if !started {
            if is_header {
                // Try to keep anything after a colon on the same line.
                if let Some((_, rest)) = line.split_once(':') {
                    let rest = rest.trim();
                    if !rest.is_empty() {
                        collected.push(rest.to_string());
                        started = true;
                    }
                }
                continue;
            }
            if is_expl {
                continue;
            }
            started = true;
        }

        if started && is_expl {
            break;
        }

        collected.push(line.to_string());
    }

    if collected.is_empty() {
        trimmed.to_string()
    } else {
        collected.join("\n")
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
