use super::*;
use crate::llm::TranslatorBackend;
use serde_json::Value;

use super::transport_http_mock::{
    backend_for, chat_json, lock_probe_cache, models_json, must_fail, MockResponse, MockServer,
};

// ---------------------------------------------------------------------------
// Endpoint construction / namespace selection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn default_namespace_is_openai_for_models() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start(|_| MockResponse::ok(&models_json(&["m1:v1"]))).await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    let models = backend.list_models().await?;

    assert_eq!(models, vec!["m1:v1".to_string()]);
    assert_eq!(
        server.request_paths(),
        vec!["/openai/v1/models".to_string()]
    );
    Ok(())
}

#[tokio::test]
async fn namespace_fallback_on_404_is_remembered() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start(|path| {
        if path == "/openai/v1/models" {
            MockResponse::status_only(404)
        } else {
            MockResponse::ok(&models_json(&["m1:v1"]))
        }
    })
    .await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    let first = backend.list_models().await?;
    let second = backend.list_models().await?;

    assert_eq!(first, vec!["m1:v1".to_string()]);
    assert_eq!(second, vec!["m1:v1".to_string()]);
    assert_eq!(
        server.request_paths(),
        vec![
            "/openai/v1/models".to_string(),
            "/v1/models".to_string(),
            "/v1/models".to_string(),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn probe_teaches_translation_the_working_namespace() -> Result<(), Box<dyn std::error::Error>>
{
    let server = MockServer::start(|path| {
        if path == "/openai/v1/models" {
            MockResponse::status_only(404)
        } else {
            MockResponse::ok(&chat_json("Probe"))
        }
    })
    .await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    let probe = backend.probe_chat_completions(2_000).await?;
    assert!(probe);

    let translated = backend.translate("你好", "zh-CN", "en-US").await?;
    assert_eq!(translated, "Probe");

    // The probe learned /v1/, so the translation request goes straight there.
    assert_eq!(
        server.request_paths(),
        vec![
            "/openai/v1/models".to_string(),
            "/v1/models".to_string(),
            "/v1/chat/completions".to_string(),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn namespace_fallback_is_remembered_for_post() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start(|path| {
        if path == "/openai/v1/chat/completions" {
            MockResponse::status_only(404)
        } else {
            MockResponse::ok(&chat_json("Hi"))
        }
    })
    .await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    backend.translate("你好", "zh-CN", "en-US").await?;
    backend.translate("再见", "zh-CN", "en-US").await?;

    assert_eq!(
        server.request_paths(),
        vec![
            "/openai/v1/chat/completions".to_string(),
            "/v1/chat/completions".to_string(),
            "/v1/chat/completions".to_string(),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn endpoint_url_for_reflects_the_current_namespace() -> Result<(), Box<dyn std::error::Error>>
{
    let transport = HttpTransport::new(1_000);
    let base = "http://127.0.0.1:9";
    assert_eq!(
        transport.endpoint_url_for(base, "chat/completions"),
        "http://127.0.0.1:9/openai/v1/chat/completions"
    );

    let server = MockServer::start(|path| {
        if path == "/openai/v1/models" {
            MockResponse::status_only(404)
        } else {
            MockResponse::ok(&models_json(&["m1:v1"]))
        }
    })
    .await?;
    let client = reqwest::Client::new();
    let learned = transport.probe_models(&client, &server.url()).await;
    assert!(matches!(learned, ModelsProbeOutcome::Ready));
    assert_eq!(
        transport.endpoint_url_for(&server.url(), "chat/completions"),
        format!("{}/v1/chat/completions", server.url())
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Status handling
// ---------------------------------------------------------------------------

#[tokio::test]
async fn non_success_status_is_reported_for_get() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start(|_| MockResponse::status_only(500)).await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    let error = must_fail(backend.list_models().await, "models list")?;

    assert_eq!(
        error.to_string(),
        "API error: API error 500 Internal Server Error"
    );
    Ok(())
}

#[tokio::test]
async fn non_success_status_is_reported_for_post_and_retried_once(
) -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start(|_| MockResponse::status_only(500)).await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    let error = must_fail(
        backend.translate("你好", "zh-CN", "en-US").await,
        "translation",
    )?;

    assert_eq!(
        error.to_string(),
        "API error: API error 500 Internal Server Error"
    );
    // The port-change recovery retries once at the same configured URL.
    assert_eq!(
        server.request_paths(),
        vec![
            "/openai/v1/chat/completions".to_string(),
            "/openai/v1/chat/completions".to_string(),
        ]
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn malformed_json_is_a_parse_error() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start(|_| MockResponse::ok("this is not json")).await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    let error = must_fail(
        backend.translate("你好", "zh-CN", "en-US").await,
        "translation",
    )?;

    assert!(error.to_string().contains("Failed to parse response"));
    // A parse failure is not a transport failure, so there is no retry.
    assert_eq!(server.request_paths().len(), 1);
    Ok(())
}

#[tokio::test]
async fn empty_completion_content_returns_an_empty_string() -> Result<(), Box<dyn std::error::Error>>
{
    let server = MockServer::start(|_| MockResponse::ok(&chat_json(""))).await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    let translated = backend.translate("你好", "zh-CN", "en-US").await?;

    assert_eq!(translated, "");
    Ok(())
}

#[tokio::test]
async fn successful_translation_returns_trimmed_content() -> Result<(), Box<dyn std::error::Error>>
{
    let server = MockServer::start(|_| MockResponse::ok(&chat_json("  \\\"Hello\\\"  "))).await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "m1:v1", 30_000);

    let translated = backend.translate("你好", "zh-CN", "en-US").await?;

    assert_eq!(translated, "Hello");
    Ok(())
}

// ---------------------------------------------------------------------------
// Wire shape
// ---------------------------------------------------------------------------

#[tokio::test]
async fn wire_request_carries_model_and_built_prompt_unchanged(
) -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start(|_| MockResponse::ok(&chat_json("OK"))).await?;
    let _guard = lock_probe_cache().await;
    let backend = backend_for(&server.url(), "my-model", 30_000);

    backend.translate("你好世界", "zh-CN", "en-US").await?;

    let bodies = server.request_bodies();
    assert_eq!(bodies.len(), 1);
    let body: Value = serde_json::from_str(&bodies[0])?;
    assert_eq!(body["model"], "my-model");
    assert_eq!(body["temperature"], 0.7);
    assert_eq!(body["top_k"], 20);
    assert_eq!(body["top_p"], 0.6);
    assert_eq!(body["repeat_penalty"], 1.05);
    assert_eq!(body["max_tokens"], 120);
    let messages = body["messages"]
        .as_array()
        .ok_or("messages must be an array")?;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(
        messages[0]["content"],
        "将以下文本翻译为英语，注意只需要输出翻译后的结果，不要额外解释：\n\n你好世界"
    );
    Ok(())
}
