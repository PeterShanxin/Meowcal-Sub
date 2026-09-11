use crate::protocol::{Error, MAX_FRAME_BYTES};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompletionParams {
    pub request: Value,
    pub timeout_ms: u64,
}

pub fn validate(params: &CompletionParams, model: &str) -> Result<(), Error> {
    let invalid = || {
        Error::new(
            "INVALID_COMPLETION",
            "Invalid completion parameters or budget",
        )
    };
    if !(1..=90_000).contains(&params.timeout_ms) {
        return Err(invalid());
    }
    let request = params.request.as_object().ok_or_else(invalid)?;
    let allowed = [
        "model",
        "messages",
        "temperature",
        "top_k",
        "top_p",
        "repeat_penalty",
        "max_tokens",
        "stream",
        "stop",
        "seed",
    ];
    if request.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid());
    }
    if request.get("model").and_then(Value::as_str) != Some(model)
        || request
            .get("stream")
            .is_some_and(|value| value != &Value::Bool(false))
    {
        return Err(invalid());
    }
    let tokens = request
        .get("max_tokens")
        .and_then(Value::as_u64)
        .ok_or_else(invalid)?;
    if !(1..=4096).contains(&tokens) {
        return Err(invalid());
    }
    let messages = request
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    if messages.is_empty() || messages.len() > 64 {
        return Err(invalid());
    }
    let mut bytes = 0;
    for message in messages {
        let message = message.as_object().ok_or_else(invalid)?;
        if message.len() != 2
            || !matches!(
                message.get("role").and_then(Value::as_str),
                Some("system" | "user" | "assistant")
            )
        {
            return Err(invalid());
        }
        bytes += message
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?
            .len();
    }
    if bytes > 64 * 1024 {
        return Err(invalid());
    }
    for (name, low, high) in [
        ("temperature", 0.0, 2.0),
        ("top_p", 0.0, 1.0),
        ("repeat_penalty", 0.0, 2.0),
    ] {
        if let Some(value) = request.get(name) {
            let value = value.as_f64().ok_or_else(invalid)?;
            if !(low..=high).contains(&value) {
                return Err(invalid());
            }
        }
    }
    if let Some(value) = request.get("top_k") {
        if value.as_u64().is_none_or(|value| value > 1000) {
            return Err(invalid());
        }
    }
    if let Some(value) = request.get("seed") {
        if value.as_i64().is_none() {
            return Err(invalid());
        }
    }
    if let Some(value) = request.get("stop") {
        let valid = value.as_str().is_some_and(|text| text.len() <= 256)
            || value.as_array().is_some_and(|list| {
                list.len() <= 16
                    && list
                        .iter()
                        .all(|item| item.as_str().is_some_and(|text| text.len() <= 256))
            });
        if !valid {
            return Err(invalid());
        }
    }
    Ok(())
}

pub async fn execute(endpoint: &str, params: &CompletionParams) -> Result<Value, Error> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_millis(params.timeout_ms))
        .build()
        .map_err(|_| Error::new("TRANSPORT_ERROR", "Could not create local transport"))?;
    let mut response = client
        .post(format!("{endpoint}/v1/chat/completions"))
        .json(&params.request)
        .send()
        .await
        .map_err(|_| Error::new("TRANSPORT_ERROR", "Local completion request failed"))?;
    if !response.status().is_success() {
        return Err(Error::new(
            "RUNTIME_ERROR",
            "Local runtime rejected completion",
        ));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| Error::new("TRANSPORT_ERROR", "Incomplete local response"))?
    {
        if body.len() + chunk.len() > MAX_FRAME_BYTES - 1024 {
            return Err(Error::new(
                "RESPONSE_TOO_LARGE",
                "Runtime response exceeds frame limit",
            ));
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body)
        .map_err(|_| Error::new("INVALID_RESPONSE", "Runtime returned invalid JSON"))
}

pub async fn sample(endpoint: &str, model: &str) -> Result<(), String> {
    let params = CompletionParams {
        timeout_ms: 90_000,
        request: json!({
            "model": model, "messages": [{"role":"user", "content":"Translate the following segment into English, without additional explanation.\n\n先不提时钟塔"}],
            "temperature":0.7, "top_k":20, "top_p":0.6, "repeat_penalty":1.05, "max_tokens":120, "stream":false
        }),
    };
    let result = execute(endpoint, &params)
        .await
        .map_err(|error| format!("ENGINE_SAMPLE_TRANSLATION_FAILED: {}", error.code))?;
    let content = result
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if content.is_empty()
        || !content
            .chars()
            .any(|character| character.is_ascii_alphabetic())
        || content
            .chars()
            .any(|character| ('\u{3400}'..='\u{9fff}').contains(&character))
    {
        return Err("ENGINE_SAMPLE_TRANSLATION_FAILED".into());
    }
    Ok(())
}
