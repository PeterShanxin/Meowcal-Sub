use super::*;
use crate::llm::foundry_local::FoundryLocalBackend;
use crate::llm::TranslatorBackend;

use super::transport_http_mock::{
    backend_for, lock_probe_cache, models_json, must_fail, MockResponse, MockServer,
};

// ---------------------------------------------------------------------------
// Timeout selection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn translation_client_timeout_comes_from_config() -> Result<(), Box<dyn std::error::Error>> {
    let server =
        MockServer::start(|_| MockResponse::delayed(200, &chat_json_late(), 2_000)).await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "m1:v1", 150);

    let started = std::time::Instant::now();
    let error = must_fail(
        backend.translate("你好", "zh-CN", "en-US").await,
        "translation",
    )?;

    assert!(started.elapsed() < std::time::Duration::from_millis(1_500));
    assert!(error.to_string().contains("Request failed"));
    Ok(())
}

#[tokio::test]
async fn probe_timeout_is_a_warmup_result_not_an_error() -> Result<(), Box<dyn std::error::Error>> {
    let server =
        MockServer::start(|_| MockResponse::delayed(200, &models_json(&["m1:v1"]), 2_000)).await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    let result = backend.probe_chat_completions(100).await?;

    assert!(!result);
    let snapshot = backend.probe_snapshot().ok_or("snapshot must exist")?;
    assert!(matches!(
        snapshot.result,
        crate::llm::FoundryProbeResult::Timeout
    ));
    Ok(())
}

#[tokio::test]
async fn probe_success_records_the_probe_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start(|_| MockResponse::ok(&models_json(&["m1:v1"]))).await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    let result = backend.probe_chat_completions(2_000).await?;

    assert!(result);
    assert!(backend.is_probe_cache_valid());
    let snapshot = backend.probe_snapshot().ok_or("snapshot must exist")?;
    assert!(matches!(
        snapshot.result,
        crate::llm::FoundryProbeResult::Success
    ));
    assert!(snapshot.last_success_ms.is_some());
    Ok(())
}

// ---------------------------------------------------------------------------
// Transport error preservation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_connection_closed_without_response_is_a_request_failure(
) -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start_closing().await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    let error = must_fail(
        backend.translate("你好", "zh-CN", "en-US").await,
        "translation",
    )?;

    assert!(
        error
            .to_string()
            .starts_with("API error: Request failed: error sending request"),
        "unexpected error: {error}"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Health checks
// ---------------------------------------------------------------------------

#[tokio::test]
async fn check_health_is_false_without_a_service_url() {
    let backend = FoundryLocalBackend::new(crate::config::FoundryLocalConfig {
        model: Some("m1:v1".to_string()),
        endpoint_url: None,
        ..crate::config::FoundryLocalConfig::default()
    });

    assert!(!backend.check_health().await);
}

#[tokio::test]
async fn check_health_is_true_on_success() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start(|_| MockResponse::ok(&models_json(&["m1:v1"]))).await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    assert!(backend.check_health().await);
    Ok(())
}

#[tokio::test]
async fn check_health_404_flips_the_namespace_without_verifying_it(
) -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start(|path| {
        if path == "/openai/v1/models" {
            MockResponse::status_only(404)
        } else {
            MockResponse::status_only(500)
        }
    })
    .await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    assert!(!backend.check_health().await);

    // The 404 flipped the stored namespace to /v1/ even though it was never
    // verified, so the next request starts there instead of at /openai/.
    let error = must_fail(backend.list_models().await, "models list")?;
    assert_eq!(
        error.to_string(),
        "API error: API error 500 Internal Server Error"
    );
    assert_eq!(
        server.request_paths(),
        vec!["/openai/v1/models".to_string(), "/v1/models".to_string(),]
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Namespace reset
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reset_namespace_restarts_at_the_default_namespace(
) -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start(|path| {
        if path == "/openai/v1/models" {
            MockResponse::status_only(404)
        } else {
            MockResponse::ok(&models_json(&["m1:v1"]))
        }
    })
    .await?;
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
    Ok(())
}

fn chat_json_late() -> String {
    format!(r#"{{"choices":[{{"message":{{"role":"assistant","content":"late"}}}}]}}"#)
}
