// =============================================================================
// EDGE_TRANSLATOR.RS - Experimental Edge Translator Backend (WebView2)
// =============================================================================

use crate::llm::{BackendId, LlmError, ReadyState, TranslatorBackend};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Listener};
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration};
use tracing::warn;

const EDGE_TRANSLATOR_TIMEOUT_MS: u64 = 2500;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);
static PENDING_RESPONSES: OnceLock<PendingMap> = OnceLock::new();
static LISTENER_READY: OnceLock<()> = OnceLock::new();

type PendingMap = Arc<Mutex<HashMap<String, oneshot::Sender<EdgeTranslateResponse>>>>;

/// Experimental Edge Translator backend (WebView2)
pub struct EdgeTranslatorBackend {
    app: AppHandle,
    pending: PendingMap,
    cache: Arc<Mutex<EdgeProbeCache>>,
}

impl EdgeTranslatorBackend {
    pub fn new(app: AppHandle) -> Self {
        let pending = init_listener(&app);
        let cache = Arc::new(Mutex::new(EdgeProbeCache::default()));

        Self { app, pending, cache }
    }

    fn next_request_id() -> String {
        let id = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("edge-{}", id)
    }

    async fn send_request(
        &self,
        kind: EdgeRequestKind,
        text: Option<String>,
        source_language: &str,
        target_language: &str,
    ) -> Result<EdgeTranslateResponse, LlmError> {
        let request_id = Self::next_request_id();
        let request = EdgeTranslateRequest {
            request_id: request_id.clone(),
            kind,
            text,
            source_language: source_language.to_string(),
            target_language: target_language.to_string(),
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            pending.insert(request_id.clone(), tx);
        }

        if let Err(err) = self
            .app
            .emit_to("main", "edge-translate-request", request)
        {
            let mut pending = self.pending.lock().unwrap();
            pending.remove(&request_id);
            return Err(LlmError::ApiError(format!(
                "Failed to emit edge-translate-request: {}",
                err
            )));
        }

        let response = match timeout(Duration::from_millis(EDGE_TRANSLATOR_TIMEOUT_MS), rx).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => {
                let mut pending = self.pending.lock().unwrap();
                pending.remove(&request_id);
                return Err(LlmError::ApiError(
                    "Edge Translator response channel dropped".to_string(),
                ));
            }
            Err(_) => {
                let mut pending = self.pending.lock().unwrap();
                pending.remove(&request_id);
                return Err(LlmError::ApiError(format!(
                    "Edge Translator timed out after {}ms",
                    EDGE_TRANSLATOR_TIMEOUT_MS
                )));
            }
        };

        Ok(response)
    }

    async fn probe(
        &self,
        source_language: &str,
        target_language: &str,
    ) -> Result<EdgeProbeCache, LlmError> {
        let response = self
            .send_request(
                EdgeRequestKind::Probe,
                None,
                source_language,
                target_language,
            )
            .await?;

        if response.kind != EdgeRequestKind::Probe {
            warn!("Edge Translator probe response kind mismatch: {:?}", response.kind);
        }

        let mut state = map_ready_state(response.ready_state.as_deref());
        let mut notes = response
            .notes
            .unwrap_or_else(|| "No probe details returned.".to_string());

        if let Some(error) = response.error {
            state = ReadyState::Error;
            notes = error;
        }

        let updated = EdgeProbeCache { state, notes };
        let mut cache = self.cache.lock().unwrap();
        *cache = updated.clone();
        Ok(updated)
    }
}

#[async_trait]
impl TranslatorBackend for EdgeTranslatorBackend {
    fn id(&self) -> BackendId {
        BackendId::EdgeTranslator
    }

    fn name(&self) -> &'static str {
        "Edge Translator (Experimental)"
    }

    fn is_available(&self) -> bool {
        matches!(
            self.cache.lock().unwrap().state,
            ReadyState::Ready | ReadyState::NotReady
        )
    }

    fn ready_state(&self) -> ReadyState {
        self.cache.lock().unwrap().state
    }

    fn notes(&self) -> String {
        self.cache.lock().unwrap().notes.clone()
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

        let probe = self.probe(source_language, target_language).await?;
        if probe.state != ReadyState::Ready {
            return Err(LlmError::ModelNotAvailable(format!(
                "Edge Translator not ready: {}",
                probe.notes
            )));
        }

        let response = self
            .send_request(
                EdgeRequestKind::Translate,
                Some(text.to_string()),
                source_language,
                target_language,
            )
            .await?;

        if response.kind != EdgeRequestKind::Translate {
            warn!("Edge Translator translate response kind mismatch: {:?}", response.kind);
        }

        if let Some(error) = response.error {
            return Err(LlmError::TranslationError(error));
        }

        response
            .translated_text
            .ok_or_else(|| LlmError::TranslationError("Edge Translator returned no text".to_string()))
    }
}

#[derive(Debug, Clone)]
struct EdgeProbeCache {
    state: ReadyState,
    notes: String,
}

impl Default for EdgeProbeCache {
    fn default() -> Self {
        Self {
            state: ReadyState::NotSupported,
            notes: "Edge Translator not probed yet.".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum EdgeRequestKind {
    Probe,
    Translate,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EdgeTranslateRequest {
    request_id: String,
    kind: EdgeRequestKind,
    text: Option<String>,
    source_language: String,
    target_language: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EdgeTranslateResponse {
    request_id: String,
    kind: EdgeRequestKind,
    translated_text: Option<String>,
    error: Option<String>,
    ready_state: Option<String>,
    notes: Option<String>,
}

fn map_ready_state(value: Option<&str>) -> ReadyState {
    match value {
        Some("ready") => ReadyState::Ready,
        Some("notReady") => ReadyState::NotReady,
        Some("notSupported") => ReadyState::NotSupported,
        Some("error") => ReadyState::Error,
        _ => ReadyState::Error,
    }
}

fn init_listener(app: &AppHandle) -> PendingMap {
    let pending = PENDING_RESPONSES
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone();

    let pending_clone = pending.clone();
    LISTENER_READY.get_or_init(|| {
        let pending_map = pending_clone.clone();
        app.listen("edge-translate-response", move |event| {
            let response = match serde_json::from_str::<EdgeTranslateResponse>(event.payload()) {
                Ok(payload) => payload,
                Err(err) => {
                    warn!("Edge Translator response parse error: {}", err);
                    return;
                }
            };

            let sender = {
                let mut pending = pending_map.lock().unwrap();
                pending.remove(&response.request_id)
            };

            if let Some(sender) = sender {
                let _ = sender.send(response);
            }
        });
    });

    pending
}
