use super::*;
use crate::llm::foundry_local::FoundryLocalBackend;
use crate::llm::TranslatorBackend;

use super::transport_http_mock::{
    backend_for, lock_probe_cache, models_json, MockResponse, MockServer,
};

// ---------------------------------------------------------------------------
// Timeout selection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn translation_client_timeout_comes_from_config() {
    let server = MockServer::start(|_| MockResponse::delayed(200, &chat_json_late(), 2_000))
        .await
        .expect("mock server must bind");
    let _guard = lock_probe_cache();
    let backend = backend_for(&server.url(), "m1:v1", 150);

    let started = std::time::Instant::now();
    let error = backend
        .translate("你好", "zh-CN", "en-US")
        .await
        .expect_err("a 2s server against a 150ms timeout must fail");

    assert!(started.elapsed() < std::time::Duration::from_millis(1_500));
    assert!(error.to_string().contains("Request failed"));
}

#[tokio::test]
async fn probe_timeout_is_a_warmup_result_not_an_error() {
    let server = MockServer::start(|_| MockResponse::delayed(200, &models_json(&["m1:v1"]), 2_000))
        .await
        .expect("mock server must bind");
    let _guard = lock_probe_cache();
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    let result = backend
        .probe_chat_completions(100)
        .await
        .expect("a probe timeout is Ok(false), not an error");

    assert!(!result);
    let snapshot = backend.probe_snapshot().expect("snapshot must exist");
    assert!(matches!(
        snapshot.result,
        crate::llm::FoundryProbeResult::Timeout
    ));
}

#[tokio::test]
async fn probe_success_records_the_probe_snapshot() {
    let server = MockServer::start(|_| MockResponse::ok(&models_json(&["m1:v1"])))
        .await
        .expect("mock server must bind");
    let _guard = lock_probe_cache();
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    let result = backend
        .probe_chat_completions(2_000)
        .await
        .expect("probe should succeed");

    assert!(result);
    assert!(backend.is_probe_cache_valid());
    let snapshot = backend.probe_snapshot().expect("snapshot must exist");
    assert!(matches!(
        snapshot.result,
        crate::llm::FoundryProbeResult::Success
    ));
    assert!(snapshot.last_success_ms.is_some());
}

// ---------------------------------------------------------------------------
// Transport error preservation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_connection_closed_without_response_is_a_request_failure() {
    let server = MockServer::start_closing()
        .await
        .expect("mock server must bind");
    let _guard = lock_probe_cache();
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    let error = backend
        .translate("你好", "zh-CN", "en-US")
        .await
        .expect_err("a closed connection must fail");

    assert!(
        error
            .to_string()
            .starts_with("API error: Request failed: error sending request"),
        "unexpected error: {error}"
    );
}

// ---------------------------------------------------------------------------
// Health checks
// ---------------------------------------------------------------------------

#[tokio::test]
async fn check_health_is_false_without_a_service_url() {
    let _guard = lock_probe_cache();
    let backend = FoundryLocalBackend::new(crate::config::FoundryLocalConfig {
        model: Some("m1:v1".to_string()),
        endpoint_url: None,
        ..crate::config::FoundryLocalConfig::default()
    });

    assert!(!backend.check_health().await);
}

#[tokio::test]
async fn check_health_is_true_on_success() {
    let server = MockServer::start(|_| MockResponse::ok(&models_json(&["m1:v1"])))
        .await
        .expect("mock server must bind");
    let _guard = lock_probe_cache();
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    assert!(backend.check_health().await);
}

#[tokio::test]
async fn check_health_404_flips_the_namespace_without_verifying_it() {
    let server = MockServer::start(|path| {
        if path == "/openai/v1/models" {
            MockResponse::status_only(404)
        } else {
            MockResponse::status_only(500)
        }
    })
    .await
    .expect("mock server must bind");
    let _guard = lock_probe_cache();
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    assert!(!backend.check_health().await);

    // The 404 flipped the stored namespace to /v1/ even though it was never
    // verified, so the next request starts there instead of at /openai/.
    let error = backend.list_models().await.expect_err("500 must fail");
    assert_eq!(
        error.to_string(),
        "API error: API error 500 Internal Server Error"
    );
    assert_eq!(
        server.request_paths(),
        vec!["/openai/v1/models".to_string(), "/v1/models".to_string(),]
    );
}

// ---------------------------------------------------------------------------
// Namespace reset
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reset_namespace_restarts_at_the_default_namespace() {
    let server = MockServer::start(|path| {
        if path == "/openai/v1/models" {
            MockResponse::status_only(404)
        } else {
            MockResponse::ok(&models_json(&["m1:v1"]))
        }
    })
    .await
    .expect("mock server must bind");
    let transport = HttpTransport::new(1_000);
    let client = reqwest::Client::new();

    let learned = transport.probe_models(&client, &server.url()).await;
    assert!(matches!(learned, ModelsProbeOutcome::Ready));
    let again = transport.probe_models(&client, &server.url()).await;
    assert!(matches!(again, ModelsProbeOutcome::Ready));

    transport.reset_namespace();

    let after = transport.probe_models(&client, &server.url()).await;
    assert!(matches!(after, ModelsProbeOutcome::Ready));

    // Learned: /openai (404) -> /v1; direct /v1; reset; /openai (404) -> /v1.
    assert_eq!(
        server.request_paths(),
        vec![
            "/openai/v1/models".to_string(),
            "/v1/models".to_string(),
            "/v1/models".to_string(),
            "/openai/v1/models".to_string(),
            "/v1/models".to_string(),
        ]
    );
}

fn chat_json_late() -> String {
    format!(r#"{{"choices":[{{"message":{{"role":"assistant","content":"late"}}}}]}}"#)
}
