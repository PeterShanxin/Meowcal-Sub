use crate::config::FoundryLocalConfig;
use crate::llm::chat_wire::{
    generation_rate, ChatCompletionRequest, ChatCompletionResponse, ChatMessage,
};
use crate::llm::{
    build_subtitle_translation_prompt, subtitle_output::sanitize_subtitle_translation_output,
    transport_errors::describe_request_failure, BackendId, FoundryLocalPhase, LlmError,
    PromptRouterOptions, ReadyState, TranslatorBackend,
};
use crate::sync_utils::{read_or_recover, write_or_recover};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

use super::transport_http::{HttpTransport, ModelsProbeOutcome, TransportError};

/// Every `foundry` spawn goes through here: these run on a timer while the
/// viewer is watching, and four of the five once drew a console window (#67).
fn foundry_command() -> std::process::Command {
    crate::windowless_command::std_command("foundry")
}

const START_ATTEMPT_COOLDOWN_MS: u64 = 6_000;
static LAST_START_ATTEMPT_MS: AtomicU64 = AtomicU64::new(0);

/// Time when the service was last detected as newly started (or restarted on a new port).
/// Used to prevent probing too soon after service start, which can crash Foundry.
static LAST_SERVICE_START_MS: AtomicU64 = AtomicU64::new(0);
/// Minimum delay after service start before we attempt chat probes (milliseconds).
/// NPU models can take 15+ seconds to load; this delay prevents crashing Foundry during loading.
/// We use 8 seconds as a balance between UX and stability - the probe may still timeout
/// (showing "preparing" status) but shouldn't crash the service.
const SERVICE_STABILIZATION_MS: u64 = 8_000;

/// Cache probe success for this duration (milliseconds)
const PROBE_CACHE_DURATION_MS: u64 = 300_000;
/// Fast probe timeout for "Refresh Status" button (milliseconds)
pub const FAST_PROBE_TIMEOUT_MS: u64 = 2_000;
/// Slow warmup probe timeout for "Prepare Foundry" button (milliseconds)
pub const SLOW_PROBE_TIMEOUT_MS: u64 = 25_000;

const AUTO_MODEL_GROUP_WEIGHT: u32 = 120;

struct ProbeCache {
    last_success_ms: AtomicU64,
    last_attempt_ms: AtomicU64,
    last_result: AtomicU8,
    last_error: RwLock<Option<String>>,
    last_model: RwLock<Option<String>>,
    last_service_url: RwLock<Option<String>>,
}

static PROBE_CACHE: OnceLock<ProbeCache> = OnceLock::new();

/// TTL for CLI cache entries (milliseconds). CLI calls spawn processes which is expensive,
/// so we cache results for a short time to avoid repeated process spawning in hot paths.
const CLI_CACHE_TTL_MS: u64 = 5_000;

/// Global cache for CLI results to avoid repeated process spawning.
/// This is separate from per-instance state because TranslationManager creates fresh
/// FoundryLocalBackend instances frequently (e.g., in list_backends).
struct CliCache {
    /// Cached service URL from `foundry service status`
    service_url: RwLock<Option<String>>,
    /// Timestamp when service URL was cached
    service_url_cached_at: AtomicU64,
    /// Cached models from `foundry cache list`
    cached_models: RwLock<Vec<String>>,
    /// Timestamp when models were cached
    models_cached_at: AtomicU64,
}

static CLI_CACHE: OnceLock<CliCache> = OnceLock::new();

fn cli_cache() -> &'static CliCache {
    CLI_CACHE.get_or_init(|| CliCache {
        service_url: RwLock::new(None),
        service_url_cached_at: AtomicU64::new(0),
        cached_models: RwLock::new(Vec::new()),
        models_cached_at: AtomicU64::new(0),
    })
}

/// Invalidate the CLI cache, forcing fresh CLI calls on next access.
pub fn invalidate_cli_cache() {
    let cache = cli_cache();
    cache.service_url_cached_at.store(0, Ordering::SeqCst);
    cache.models_cached_at.store(0, Ordering::SeqCst);
}

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

/// Cached "ready to talk" probe result for the currently selected Foundry Local target
/// (service URL + resolved model id).
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FoundryProbeResult {
    None,
    Success,
    Timeout,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundryProbeSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_attempt_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_success_ms: Option<u64>,
    pub result: FoundryProbeResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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

/// Foundry Local backend using OpenAI-compatible API
pub struct FoundryLocalBackend {
    config: FoundryLocalConfig,
    transport: HttpTransport,
    service_url: RwLock<Option<String>>,
    service_available: AtomicBool,
    cached_models: RwLock<Vec<String>>,
}

impl FoundryLocalBackend {
    pub fn new(config: FoundryLocalConfig) -> Self {
        let transport = HttpTransport::new(config.timeout_ms as u64);
        let configured_url = config.effective_endpoint_url();
        let has_configured_url = configured_url.is_some();
        let configured_model = config.model.clone();

        Self {
            config,
            transport,
            service_url: RwLock::new(configured_url),
            service_available: AtomicBool::new(has_configured_url),
            cached_models: RwLock::new(configured_model.into_iter().collect()),
        }
        // Note: Service detection happens lazily on first is_available() call
    }

    fn mark_service_unavailable(&self) {
        *write_or_recover(&self.service_url) = None;
        self.service_available.store(false, Ordering::SeqCst);
        self.transport.reset_namespace();
        self.invalidate_probe_cache();
    }

    /// Execute a GET request with automatic namespace fallback on 404.
    ///
    /// On success, stores the working namespace for future requests.
    async fn get_with_namespace_fallback(
        &self,
        base_url: &str,
        endpoint: &str,
    ) -> Result<reqwest::Response, LlmError> {
        self.transport
            .get_with_namespace_fallback(base_url, endpoint)
            .await
            .map_err(map_get_error)
    }

    /// Execute a POST request with automatic namespace fallback on 404.
    ///
    /// On success, stores the working namespace for future requests.
    async fn post_with_namespace_fallback<T: Serialize>(
        &self,
        base_url: &str,
        endpoint: &str,
        body: &T,
    ) -> Result<reqwest::Response, LlmError> {
        self.transport
            .post_with_namespace_fallback(base_url, endpoint, body)
            .await
            .map_err(map_post_error)
    }

    /// Get the service URL by parsing `foundry service status` output.
    /// Uses a TTL cache to avoid spawning processes repeatedly.
    pub fn get_service_url_from_cli() -> Option<String> {
        Self::get_service_url_from_cli_cached(false)
    }

    /// Get service URL, optionally bypassing the cache for fresh data.
    pub fn get_service_url_from_cli_cached(force_refresh: bool) -> Option<String> {
        let cache = cli_cache();
        let now_ms = epoch_ms();

        // Check cache validity
        if !force_refresh {
            let cached_at = cache.service_url_cached_at.load(Ordering::SeqCst);
            if cached_at > 0 && now_ms.saturating_sub(cached_at) < CLI_CACHE_TTL_MS {
                let cached = crate::sync_utils::read_or_recover(&cache.service_url).clone();
                return cached;
            }
        }

        // Cache miss or expired - fetch fresh data
        let result = Self::fetch_service_url_from_cli();

        // Update cache
        *crate::sync_utils::write_or_recover(&cache.service_url) = result.clone();
        cache.service_url_cached_at.store(now_ms, Ordering::SeqCst);

        result
    }

    /// Internal: actually spawn the CLI process to get service URL
    fn fetch_service_url_from_cli() -> Option<String> {
        let output = foundry_command()
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

    /// Get cached models by parsing `foundry cache list` output.
    /// This is a fallback when the API doesn't return models (e.g., models cached but not running).
    /// Uses a TTL cache to avoid spawning processes repeatedly.
    pub fn get_cached_models_from_cli() -> Vec<String> {
        Self::get_cached_models_from_cli_cached(false)
    }

    /// Get cached models, optionally bypassing the cache for fresh data.
    pub fn get_cached_models_from_cli_cached(force_refresh: bool) -> Vec<String> {
        let cache = cli_cache();
        let now_ms = epoch_ms();

        // Check cache validity - return cached value even if empty (empty is a valid cached outcome)
        if !force_refresh {
            let cached_at = cache.models_cached_at.load(Ordering::SeqCst);
            if cached_at > 0 && now_ms.saturating_sub(cached_at) < CLI_CACHE_TTL_MS {
                return crate::sync_utils::read_or_recover(&cache.cached_models).clone();
            }
        }

        // Cache miss or expired - fetch fresh data
        let result = Self::fetch_cached_models_from_cli();

        // Update cache (even if empty, to avoid repeated failed calls)
        *crate::sync_utils::write_or_recover(&cache.cached_models) = result.clone();
        cache.models_cached_at.store(now_ms, Ordering::SeqCst);

        result
    }

    /// Internal: actually spawn the CLI process to get cached models
    fn fetch_cached_models_from_cli() -> Vec<String> {
        let output = foundry_command().args(["cache", "list"]).output();

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
        let output = foundry_command()
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

    fn parse_decimal_tenths(value: &str) -> Option<u32> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }

        let mut iter = trimmed.splitn(2, '.');
        let int_part = iter.next()?.parse::<u32>().ok()?;
        let frac_part = iter.next();
        let tenths = match frac_part {
            None => 0,
            Some(frac) => frac.chars().next()?.to_digit(10)?,
        };
        Some(int_part.saturating_mul(10).saturating_add(tenths))
    }

    fn estimate_param_size_tenths(model: &str) -> Option<u32> {
        for segment in model.split(['-', '_', ' ']) {
            let lower = segment.to_ascii_lowercase();
            if lower.ends_with('b') {
                let num = lower.trim_end_matches('b');
                if let Some(parsed) = Self::parse_decimal_tenths(num) {
                    return Some(parsed);
                }
            }
        }

        let lower = model.to_ascii_lowercase();
        if let Some(idx) = lower.find("phi-") {
            let rest = &lower[(idx + 4)..];
            let mut digits = String::new();
            for ch in rest.chars() {
                if ch.is_ascii_digit() || ch == '.' {
                    digits.push(ch);
                } else {
                    break;
                }
            }
            if !digits.is_empty() {
                return Self::parse_decimal_tenths(&digits);
            }
        }

        None
    }

    fn is_thinking_model(model: &str) -> bool {
        let lower = model.to_ascii_lowercase();
        if lower.contains("reasoner") || lower.contains("reasoning") || lower.contains("thinking") {
            return true;
        }

        // Common naming pattern: deepseek-r1-*
        lower.split(['-', '_', ' ']).any(|segment| segment == "r1")
    }

    fn auto_model_score(model: &str) -> u32 {
        let lower = model.to_ascii_lowercase();

        let group_rank = if lower.contains("qnn-npu") {
            0u32
        } else if lower.contains("generic-cpu") {
            1u32
        } else if lower.contains("generic-gpu") {
            2u32
        } else {
            3u32
        };

        let size_tenths = Self::estimate_param_size_tenths(model).unwrap_or(9_999);
        let mut score = group_rank.saturating_mul(AUTO_MODEL_GROUP_WEIGHT);
        score = score.saturating_add(size_tenths.saturating_mul(10));

        // Heuristics: prefer smaller, instruction-tuned models. Avoid very large or "R1/distill"
        // models by default because they can take a long time to warm up (or crash the service).
        if lower.contains("deepseek") || lower.contains("r1") {
            score = score.saturating_add(50_000);
        }
        if lower.contains("distill") {
            score = score.saturating_add(20_000);
        }
        if lower.contains("coder") {
            score = score.saturating_add(10_000);
        }

        // Avoid ultra-tiny models as the default (quality can be too low for subtitles).
        if size_tenths < 10 {
            score = score.saturating_add(3_000);
        } else if size_tenths < 15 {
            score = score.saturating_add(800);
        }

        score
    }

    pub fn choose_auto_model(models: &[String]) -> Option<String> {
        if models.is_empty() {
            return None;
        }

        let mut candidates: Vec<&String> = models.iter().collect();
        let has_instruct = candidates
            .iter()
            .any(|m| m.to_ascii_lowercase().contains("instruct"));
        if has_instruct {
            candidates.retain(|m| m.to_ascii_lowercase().contains("instruct"));
        }

        // Avoid reasoning/"thinking" models (e.g. DeepSeek R1) for real-time subtitle translation.
        // If the user only has thinking models installed, fall back to them (better than nothing).
        let non_thinking: Vec<&String> = candidates
            .iter()
            .copied()
            .filter(|m| !Self::is_thinking_model(m))
            .collect();
        if !non_thinking.is_empty() {
            candidates = non_thinking;
        }

        candidates
            .into_iter()
            .min_by_key(|m| Self::auto_model_score(m))
            .cloned()
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
        let mut command = foundry_command();
        command
            .args(["service", "start"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

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
        if let Some(url) = self.configured_endpoint_url() {
            *write_or_recover(&self.service_url) = Some(url);
            self.service_available.store(true, Ordering::SeqCst);
            if let Some(model) = self.config.model.clone() {
                *write_or_recover(&self.cached_models) = vec![model];
            }
            return;
        }

        let previous_url = read_or_recover(&self.service_url).clone();

        if let Some(url) = Self::get_service_url_from_cli() {
            debug!("Foundry Local service detected at {}", url);
            let is_new_service = previous_url.is_none() || previous_url.as_deref() != Some(&url);
            if is_new_service {
                // Service just started or restarted (port changed).
                // Record timestamp so probes wait for stabilization.
                LAST_SERVICE_START_MS.store(epoch_ms(), Ordering::SeqCst);
                // Cached probe is no longer valid.
                self.invalidate_probe_cache();
                // Invalidate CLI cache so next calls get fresh data (new port = potentially new models).
                invalidate_cli_cache();
            }
            *write_or_recover(&self.service_url) = Some(url);
            self.service_available.store(true, Ordering::SeqCst);

            // Also try to populate models from CLI if cache is empty
            let models = read_or_recover(&self.cached_models);
            if models.is_empty() {
                drop(models); // Release read lock before acquiring write lock
                let cli_models = Self::get_cached_models_from_cli();
                if !cli_models.is_empty() {
                    debug!(
                        "Populated {} models from CLI during refresh",
                        cli_models.len()
                    );
                    *write_or_recover(&self.cached_models) = cli_models;
                }
            }
        } else {
            if previous_url.is_some() {
                self.invalidate_probe_cache();
                // Service went away; invalidate CLI cache so next refresh gets accurate state.
                invalidate_cli_cache();
            }

            debug!("Foundry Local service not running");
            *write_or_recover(&self.service_url) = None;
            self.service_available.store(false, Ordering::SeqCst);
        }
    }

    /// Ensure the Foundry Local service is running (may start the service).
    ///
    /// Use this for the explicit "Prepare Foundry" flow - regular status checks should
    /// call `refresh_service_status()` which is read-only.
    pub fn ensure_service_running(&self) -> bool {
        if self.configured_endpoint_url().is_some() {
            self.refresh_service_status();
            return true;
        }

        // Force refresh since we're actively trying to start/find the service
        if Self::get_service_url_from_cli_cached(true).is_some() {
            self.refresh_service_status();
            return true;
        }

        debug!("Foundry Local service not running, attempting to start");
        if !Self::try_start_service() {
            self.refresh_service_status();
            return false;
        }

        // The start command returns quickly; poll for the service URL for a short time.
        // Use force_refresh=true since we're polling for a newly started service.
        let start = std::time::Instant::now();
        let deadline = Duration::from_secs(5);
        while start.elapsed() < deadline {
            if Self::get_service_url_from_cli_cached(true).is_some() {
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
        read_or_recover(&self.service_url).clone()
    }

    fn configured_endpoint_url(&self) -> Option<String> {
        self.config.effective_endpoint_url()
    }

    async fn send_chat_completion(
        &self,
        base_url: &str,
        request: &ChatCompletionRequest,
    ) -> Result<reqwest::Response, LlmError> {
        let resp = self
            .post_with_namespace_fallback(base_url, "chat/completions", request)
            .await?;
        // Any successful chat completion implies the model is "ready to talk".
        self.record_probe_success();
        Ok(resp)
    }

    /// List available models from the service
    pub async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        // If service isn't running, fall back to CLI-based discovery.
        // This allows the model dropdown to populate even when the service is stopped.
        let base_url = match self.get_service_url() {
            Some(url) => url,
            None => {
                debug!("Service not running, falling back to CLI for model list");
                return Ok(Self::get_cached_models_from_cli());
            }
        };

        let response = self
            .get_with_namespace_fallback(&base_url, "models")
            .await?;

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
        *write_or_recover(&self.cached_models) = model_ids.clone();

        Ok(model_ids)
    }

    /// Get the model to use (configured or preferred auto-selection)
    fn get_model(&self) -> Option<String> {
        let models = read_or_recover(&self.cached_models);

        // Use configured model if set and available
        if let Some(ref model) = self.config.model {
            if self.configured_endpoint_url().is_some() || Self::model_in_cache(model, &models) {
                return Some(self.resolve_model_id(model, &models));
            }
        }

        // Otherwise choose a reasonable auto model.
        if let Some(chosen) = Self::choose_auto_model(&models) {
            return Some(self.resolve_model_id(&chosen, &models));
        }

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
            self.transport.check_health(&url).await
        } else {
            false
        }
    }

    // =========================================================================
    // CHAT PROBE METHODS - "Ready to Talk" verification
    // =========================================================================

    /// Check if the Foundry CLI is available on this system
    pub fn is_cli_available() -> bool {
        foundry_command()
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

        let cached_url = read_or_recover(&cache.last_service_url).clone();
        let cached_model = read_or_recover(&cache.last_model).clone();

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
        *write_or_recover(&cache.last_error) = None;
        *write_or_recover(&cache.last_service_url) = Some(url);
        *write_or_recover(&cache.last_model) = Some(model);
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
        *write_or_recover(&cache.last_error) = None;
        *write_or_recover(&cache.last_service_url) = Some(url);
        *write_or_recover(&cache.last_model) = Some(model);
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
        *write_or_recover(&cache.last_error) = Some(message);
        *write_or_recover(&cache.last_service_url) = Some(url);
        *write_or_recover(&cache.last_model) = Some(model);
    }

    /// Invalidate the probe cache (call when model selection changes)
    pub fn invalidate_probe_cache(&self) {
        let cache = probe_cache();
        cache.last_success_ms.store(0, Ordering::SeqCst);
        cache.last_attempt_ms.store(0, Ordering::SeqCst);
        cache.last_result.store(PROBE_RESULT_NONE, Ordering::SeqCst);
        *write_or_recover(&cache.last_error) = None;
        *write_or_recover(&cache.last_service_url) = None;
        *write_or_recover(&cache.last_model) = None;
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

        let cached_url = read_or_recover(&cache.last_service_url).clone();
        let cached_model = read_or_recover(&cache.last_model).clone();

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
        let error = read_or_recover(&cache.last_error).clone();
        Some((kind, age_ms, error))
    }

    /// Get the currently selected model id (resolved) if available.
    pub fn selected_model(&self) -> Option<String> {
        self.get_model()
    }

    /// Get the last probe snapshot for the current (service URL + model) target.
    pub fn probe_snapshot(&self) -> Option<FoundryProbeSnapshot> {
        if !self.is_last_probe_for_current_target() {
            return None;
        }

        let cache = probe_cache();
        let attempt_ms = cache.last_attempt_ms.load(Ordering::SeqCst);
        if attempt_ms == 0 {
            return None;
        }

        let success_ms = cache.last_success_ms.load(Ordering::SeqCst);
        let kind = cache.last_result.load(Ordering::SeqCst);
        let error = cache
            .last_error
            .read()
            .unwrap()
            .clone()
            .map(|value| value.chars().take(160).collect::<String>());

        let result = match kind {
            PROBE_RESULT_SUCCESS => FoundryProbeResult::Success,
            PROBE_RESULT_TIMEOUT => FoundryProbeResult::Timeout,
            PROBE_RESULT_ERROR => FoundryProbeResult::Error,
            _ => FoundryProbeResult::None,
        };

        Some(FoundryProbeSnapshot {
            last_attempt_ms: Some(attempt_ms),
            last_success_ms: (success_ms > 0).then_some(success_ms),
            result,
            error,
        })
    }

    /// Probe the service to check if it's ready.
    ///
    /// Previously this sent a chat completion request, but that crashes Foundry Local
    /// during model loading. Now we just check if the /models endpoint responds,
    /// which is safer. The actual model warmup will happen on the first translation
    /// request (with appropriate timeout handling).
    ///
    /// Returns Ok(true) on success, Ok(false) on timeout, Err on other errors.
    pub async fn probe_chat_completions(&self, timeout_ms: u64) -> Result<bool, LlmError> {
        let base_url = match self.get_service_url() {
            Some(url) => url,
            None => {
                return Err(LlmError::ApiError(
                    "Foundry Local service not running".to_string(),
                ))
            }
        };

        // Wait for service to stabilize if it just started.
        let last_start = LAST_SERVICE_START_MS.load(Ordering::SeqCst);
        if last_start > 0 {
            let elapsed = epoch_ms().saturating_sub(last_start);
            if elapsed < SERVICE_STABILIZATION_MS {
                let wait_ms = SERVICE_STABILIZATION_MS - elapsed;
                debug!(
                    "Waiting {}ms for Foundry service to stabilize before probing",
                    wait_ms
                );
                tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            }
        }

        // Check if /models endpoint responds - this doesn't trigger model loading
        // and won't crash Foundry like chat completions does.
        let probe_client = Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .unwrap_or_default();

        // Try probe with namespace fallback
        let outcome = self.transport.probe_models(&probe_client, &base_url).await;
        self.probe_outcome_to_result(outcome)
    }

    /// Convert a transport probe outcome into the backend's probe-cache write
    /// and the historical `Ok(true)` / `Ok(false)` / `Err` result shape.
    fn probe_outcome_to_result(&self, outcome: ModelsProbeOutcome) -> Result<bool, LlmError> {
        match outcome {
            ModelsProbeOutcome::Ready => {
                self.record_probe_success();
                Ok(true)
            }
            ModelsProbeOutcome::TimedOut => {
                self.record_probe_timeout();
                Ok(false)
            }
            ModelsProbeOutcome::RequestFailed(error) => {
                let msg = format!("Probe failed: {}", error);
                self.record_probe_error(msg.clone());
                Err(LlmError::ApiError(msg))
            }
            ModelsProbeOutcome::BadStatus(status) => {
                let msg = format!("Models endpoint returned status {}", status);
                self.record_probe_error(msg.clone());
                Err(LlmError::ApiError(msg))
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
        let models = read_or_recover(&self.cached_models);
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
            top_k: 20,
            top_p: 0.6,
            repeat_penalty: 1.05,
            max_tokens: 150,
        };

        debug!("Sending summarization request to Foundry Local");

        let response = self
            .post_with_namespace_fallback(&base_url, "chat/completions", &request)
            .await?;

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
        "Local Translation Engine"
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
            let models = read_or_recover(&self.cached_models);
            if let Some(ref model) = self.config.model {
                Self::model_in_cache(model, &models)
            } else {
                !models.is_empty()
            }
        };

        if !model_ready {
            return ReadyState::NotReady;
        }

        // If service is running and model is available, consider it ready for translation.
        // The probe cache is for UI status display (the "Model ready (probe)" ladder step),
        // not for blocking translation attempts. Translation will timeout/fallback if
        // the model is still warming up.
        ReadyState::Ready
    }

    fn notes(&self) -> String {
        if let Some(url) = self.get_service_url() {
            let models = read_or_recover(&self.cached_models);

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
                let auto = Self::choose_auto_model(&models).unwrap_or_else(|| model.clone());
                let resolved = self.resolve_model_id(&auto, &models);
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
            temperature: 0.7,
            top_k: 20,
            top_p: 0.6,
            repeat_penalty: 1.05,
            max_tokens: 120,
        };

        let url = self
            .transport
            .endpoint_url_for(&base_url, "chat/completions");
        let request_started = std::time::Instant::now();
        debug!("Sending translation request to Foundry Local: {}", url);
        info!(
            target: "translation_io",
            source_chars = text.chars().count(),
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
                        let retry_url = self
                            .transport
                            .endpoint_url_for(&refreshed_url, "chat/completions");
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

        let elapsed_ms = request_started.elapsed().as_millis() as u64;
        let completion_tokens = completion.completion_tokens();

        debug!(
            "Foundry Local translated {} chars to {} chars",
            text.chars().count(),
            translated.chars().count()
        );
        info!(
            target: "translation_io",
            source_chars = text.chars().count(),
            translated_chars = translated.chars().count(),
            completion_tokens,
            elapsed_ms,
            tokens_per_second = generation_rate(completion_tokens, elapsed_ms),
            "Translation response"
        );

        Ok(translated)
    }
}

/// Map a GET transport failure to the app-level error with the historical
/// message and (absent) logging behavior: no warn, `API error {status}` for a
/// non-success status, full failure description otherwise.
fn map_get_error(error: TransportError) -> LlmError {
    match error {
        TransportError::Timeout(error) | TransportError::Failed(error) => {
            LlmError::ApiError(describe_request_failure(&error))
        }
        TransportError::ApiStatus(status) => LlmError::ApiError(format!("API error {status}")),
    }
}

/// Map a POST transport failure to the app-level error with the historical
/// message and logging behavior: warns before returning `API error {status}` /
/// the full failure description.
fn map_post_error(error: TransportError) -> LlmError {
    match error {
        TransportError::Timeout(error) | TransportError::Failed(error) => {
            warn!("Foundry Local request failed: {}", error);
            LlmError::ApiError(describe_request_failure(&error))
        }
        TransportError::ApiStatus(status) => {
            warn!("Local translation endpoint returned HTTP {}", status);
            LlmError::ApiError(format!("API error {status}"))
        }
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
