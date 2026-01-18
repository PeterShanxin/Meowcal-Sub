// =============================================================================
// FOUNDRY_LOCAL.RS - Foundry Local Backend (OpenAI-compatible API)
// =============================================================================

use crate::config::FoundryLocalConfig;
use crate::llm::{BackendId, LlmError, ReadyState, TranslatorBackend};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::time::Duration;
use tracing::{debug, warn};

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
            if let Some(start) = line.find("http://") {
                let url_part = &line[start..];
                // Extract just the base URL (up to port)
                if let Some(end) = url_part.find('/').filter(|&i| i > 7) {
                    // Find the port end
                    let base_url = &url_part[..end];
                    return Some(base_url.to_string());
                }
                // Try to extract URL with port pattern
                let url_chars: String = url_part
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == ':' || *c == '.' || *c == '/')
                    .collect();
                if url_chars.len() > 10 {
                    // Remove trailing slash if present
                    let base = url_chars.trim_end_matches('/');
                    return Some(base.to_string());
                }
            }
        }
        None
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

        // Parse output to extract model names
        // Expected format varies, but typically includes model identifiers
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("Cache") || line.starts_with("Total") {
                continue;
            }

            // Try to extract model name (handles various output formats)
            // Common pattern: "qwen2.5-0.5b    1.2 GB    2024-01-18"
            if let Some(model_name) = line.split_whitespace().next() {
                if !model_name.is_empty() && !model_name.starts_with('-') {
                    models.push(model_name.to_string());
                }
            }
        }

        debug!("Found {} cached models via CLI: {:?}", models.len(), models);
        models
    }

    /// Try to start the Foundry service if it's not running
    fn try_start_service() -> bool {
        debug!("Attempting to start Foundry Local service");
        let output = Command::new("foundry")
            .args(["service", "start"])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                debug!("Foundry Local service start command sent successfully");
                // Service will be available when ready (no need to block here)
                true
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                debug!("Failed to start Foundry Local service: {}", stderr);
                false
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
        }

        // Cache the models
        *self.cached_models.write().unwrap() = model_ids.clone();

        Ok(model_ids)
    }

    /// Get the model to use (configured or first available)
    fn get_model(&self) -> Option<String> {
        // Use configured model if set
        if let Some(ref model) = self.config.model {
            return Some(model.clone());
        }

        // Otherwise use first cached model
        let models = self.cached_models.read().unwrap();
        models.first().cloned()
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

        // Check if we have a model configured or cached
        if self.get_model().is_some() {
            ReadyState::Ready
        } else {
            ReadyState::NotReady
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
            "Foundry Local not available. Install from: winget install Microsoft.FoundryLocal".to_string()
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
            .await
            .map_err(|e| {
                warn!("Foundry Local request failed: {}", e);
                LlmError::ApiError(format!("Request failed: {}", e))
            })?;

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
