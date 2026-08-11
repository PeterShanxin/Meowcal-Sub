// =============================================================================
// TRANSPORT_HTTP.RS - Namespace-aware HTTP transport for translation requests
// =============================================================================
// The engine speaks the OpenAI chat-completions dialect, but its API namespace
// (`/openai/v1/` vs `/v1/`) is not guaranteed. This module owns the transport
// half of that contract: the HTTP client, the namespace discovery state
// machine, endpoint URL assembly, and request dispatch with 404 fallback.
//
// It deliberately owns nothing else. Retry counts, context tiers, output
// validation, probe-cache bookkeeping, service lifecycle, and the LlmError
// mapping all stay with the callers; the transport reports what happened
// (`TransportError` / `ModelsProbeOutcome`) and the backend decides what it
// means.
// =============================================================================

use serde::Serialize;
use std::sync::atomic::{AtomicU8, Ordering};
use tracing::debug;

const API_NAMESPACE_UNKNOWN: u8 = 0;
const API_NAMESPACE_OPENAI: u8 = 1;
const API_NAMESPACE_V1: u8 = 2;

/// What went wrong with a request, before it is mapped to an app-level error.
///
/// The `reqwest::Error` is kept inside the timeout/failure variants so the
/// probe path can still tell a timeout from a hard failure, and so
/// `describe_request_failure` can preserve the exact message strings the
/// retry classifier reads. The `StatusCode` variant keeps the canonical
/// reason phrase the historical `API error {status}` messages carried.
pub(super) enum TransportError {
    /// The request hit its timeout. Distinct from `Failed` because a probe
    /// timeout means "model still warming up" (`Ok(false)`), not an error.
    Timeout(reqwest::Error),
    /// Any other request-level failure (refused, reset, closed connection, ...).
    Failed(reqwest::Error),
    /// The server answered with a non-success HTTP status.
    ApiStatus(reqwest::StatusCode),
}

/// Outcome of a `/models` readiness probe.
pub(super) enum ModelsProbeOutcome {
    /// A model endpoint answered 200, on the preferred or the fallback namespace.
    Ready,
    /// The probe request hit its timeout without an answer.
    TimedOut,
    /// The request itself failed (refused, reset, ...).
    RequestFailed(reqwest::Error),
    /// The server answered with a non-success HTTP status.
    BadStatus(reqwest::StatusCode),
}

/// Namespace-aware HTTP transport for a single backend instance.
///
/// `api_namespace` is shared state: a successful probe teaches it which
/// namespace the service speaks, and subsequent translation requests use it
/// without a 404 round-trip. `probe_models` accepts its own client because a
/// probe carries a different timeout than the default transport client.
pub(super) struct HttpTransport {
    client: reqwest::Client,
    api_namespace: AtomicU8,
}

impl HttpTransport {
    /// Build a transport whose client enforces `timeout_ms` on every request.
    pub(super) fn new(timeout_ms: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .unwrap_or_default();
        Self {
            client,
            api_namespace: AtomicU8::new(API_NAMESPACE_UNKNOWN),
        }
    }

    /// Forget which namespace the service speaks (service restarted, URL gone).
    pub(super) fn reset_namespace(&self) {
        self.api_namespace
            .store(API_NAMESPACE_UNKNOWN, Ordering::SeqCst);
    }

    /// The endpoint URL a request to `endpoint` would use right now, for logs.
    pub(super) fn endpoint_url_for(&self, base_url: &str, endpoint: &str) -> String {
        let preferred = self.preferred_api_namespace();
        self.api_url_for(base_url, preferred, endpoint)
    }

    /// The namespace to try first: a learned one, or the Openai default.
    fn preferred_api_namespace(&self) -> u8 {
        match self.api_namespace.load(Ordering::SeqCst) {
            API_NAMESPACE_V1 => API_NAMESPACE_V1,
            API_NAMESPACE_OPENAI => API_NAMESPACE_OPENAI,
            _ => API_NAMESPACE_OPENAI,
        }
    }

    fn api_url_for(&self, base_url: &str, api_namespace: u8, endpoint: &str) -> String {
        match api_namespace {
            API_NAMESPACE_V1 => format!("{}/v1/{}", base_url, endpoint),
            _ => format!("{}/openai/v1/{}", base_url, endpoint),
        }
    }

    /// The alternate namespace to try on 404 errors.
    fn fallback_namespace(preferred: u8) -> u8 {
        if preferred == API_NAMESPACE_OPENAI {
            API_NAMESPACE_V1
        } else {
            API_NAMESPACE_OPENAI
        }
    }

    /// Execute a GET request with automatic namespace fallback on 404.
    ///
    /// On success, stores the working namespace for future requests.
    pub(super) async fn get_with_namespace_fallback(
        &self,
        base_url: &str,
        endpoint: &str,
    ) -> Result<reqwest::Response, TransportError> {
        let (response, _) = self
            .get_with_client(&self.client, base_url, endpoint)
            .await?;
        Ok(response)
    }

    /// Execute a POST request with automatic namespace fallback on 404.
    ///
    /// On success, stores the working namespace for future requests.
    pub(super) async fn post_with_namespace_fallback<T: Serialize>(
        &self,
        base_url: &str,
        endpoint: &str,
        body: &T,
    ) -> Result<reqwest::Response, TransportError> {
        self.post_with_client(&self.client, base_url, endpoint, body)
            .await
    }

    /// Probe the `/models` endpoint with namespace fallback, using `client`
    /// (a probe carries its own timeout). Shares the namespace state machine
    /// with regular requests.
    pub(super) async fn probe_models(
        &self,
        client: &reqwest::Client,
        base_url: &str,
    ) -> ModelsProbeOutcome {
        match self.get_with_client(client, base_url, "models").await {
            Ok((_, used_fallback)) => {
                if used_fallback {
                    debug!("Foundry Local probe succeeded (fallback)");
                } else {
                    debug!("Foundry Local probe succeeded");
                }
                ModelsProbeOutcome::Ready
            }
            Err(TransportError::Timeout(_)) => {
                debug!("Foundry Local probe timed out");
                ModelsProbeOutcome::TimedOut
            }
            Err(TransportError::Failed(error)) => ModelsProbeOutcome::RequestFailed(error),
            Err(TransportError::ApiStatus(status)) => ModelsProbeOutcome::BadStatus(status),
        }
    }

    /// Health check that mirrors the historical behavior exactly: a 404 on the
    /// preferred namespace flips the stored namespace to the fallback *without*
    /// verifying it, and returns false.
    pub(super) async fn check_health(&self, base_url: &str) -> bool {
        let preferred_namespace = self.preferred_api_namespace();
        let models_url = self.api_url_for(base_url, preferred_namespace, "models");

        if let Ok(resp) = self.client.get(&models_url).send().await {
            if resp.status().is_success() {
                self.api_namespace
                    .store(preferred_namespace, Ordering::SeqCst);
                return true;
            }

            if resp.status().as_u16() == 404 {
                let fallback_namespace = Self::fallback_namespace(preferred_namespace);
                self.api_namespace
                    .store(fallback_namespace, Ordering::SeqCst);
            }
        }
        false
    }

    /// Shared GET dispatch: on success returns the response and whether the
    /// fallback namespace answered (probe logging uses the latter).
    async fn get_with_client(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        endpoint: &str,
    ) -> Result<(reqwest::Response, bool), TransportError> {
        let preferred = self.preferred_api_namespace();
        let url = self.api_url_for(base_url, preferred, endpoint);

        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                self.api_namespace.store(preferred, Ordering::SeqCst);
                Ok((resp, false))
            }
            Ok(resp) if resp.status().as_u16() == 404 => {
                let fallback = Self::fallback_namespace(preferred);
                let fallback_url = self.api_url_for(base_url, fallback, endpoint);
                debug!(
                    "Endpoint {} returned 404, trying fallback {}",
                    url, fallback_url
                );

                let resp = client
                    .get(&fallback_url)
                    .send()
                    .await
                    .map_err(TransportError::from_reqwest)?;

                if resp.status().is_success() {
                    self.api_namespace.store(fallback, Ordering::SeqCst);
                    Ok((resp, true))
                } else {
                    Err(TransportError::ApiStatus(resp.status()))
                }
            }
            Ok(resp) => Err(TransportError::ApiStatus(resp.status())),
            Err(error) => Err(TransportError::from_reqwest(error)),
        }
    }

    async fn post_with_client<T: Serialize>(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        endpoint: &str,
        body: &T,
    ) -> Result<reqwest::Response, TransportError> {
        let preferred = self.preferred_api_namespace();
        let url = self.api_url_for(base_url, preferred, endpoint);

        match client.post(&url).json(body).send().await {
            Ok(resp) if resp.status().is_success() => {
                self.api_namespace.store(preferred, Ordering::SeqCst);
                Ok(resp)
            }
            Ok(resp) if resp.status().as_u16() == 404 => {
                let fallback = Self::fallback_namespace(preferred);
                let fallback_url = self.api_url_for(base_url, fallback, endpoint);
                debug!(
                    "Endpoint {} returned 404, trying fallback {}",
                    url, fallback_url
                );

                let resp = client
                    .post(&fallback_url)
                    .json(body)
                    .send()
                    .await
                    .map_err(TransportError::from_reqwest)?;

                if resp.status().is_success() {
                    self.api_namespace.store(fallback, Ordering::SeqCst);
                    Ok(resp)
                } else {
                    Err(TransportError::ApiStatus(resp.status()))
                }
            }
            Ok(resp) => Err(TransportError::ApiStatus(resp.status())),
            Err(error) => Err(TransportError::from_reqwest(error)),
        }
    }
}

impl TransportError {
    /// Classify a request-level failure so the probe can tell a timeout apart.
    fn from_reqwest(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            TransportError::Timeout(error)
        } else {
            TransportError::Failed(error)
        }
    }
}

#[cfg(test)]
#[path = "transport_http_mock.rs"]
pub(super) mod transport_http_mock;

#[cfg(test)]
#[path = "transport_http_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "transport_http_backend_tests.rs"]
mod backend_tests;
