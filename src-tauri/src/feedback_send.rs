//! The main window sends only frozen preview data to a build-owned destination.
use age::x25519::Recipient;
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Write};
use std::time::Duration;
use uuid::Uuid;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

const MAX_BYTES: usize = 16 * 1024 * 1024;
const INVALID: &str = "FEEDBACK_REJECTED";
const CONFIG: &str = "FEEDBACK_NOT_CONFIGURED";
const LARGE: &str = "FEEDBACK_TOO_LARGE";
const OFFLINE: &str = "FEEDBACK_OFFLINE";
const PENDING: &str = "FEEDBACK_PENDING";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FeedbackRequest {
    request_id: Uuid,
    snapshot_id: String,
    #[serde(rename = "type")]
    category: String,
    description: String,
    material_ids: Vec<String>,
}

impl FeedbackRequest {
    fn validate(&self) -> Result<(), String> {
        let known = matches!(
            self.category.as_str(),
            "translation" | "ocr" | "subtitles" | "error" | "suggestion" | "other"
        );
        if !known
            || self.request_id.get_version() != Some(uuid::Version::Random)
            || self.description.chars().count() > 4000
            || self.material_ids.len() > 32
            || self.snapshot_id.len() > 100
            || self.material_ids.iter().any(|s| s.len() > 128)
        {
            return Err(INVALID.into());
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    issue_number: u64,
}

struct Destination {
    endpoint: Url,
    recipient: Recipient,
}

impl Destination {
    fn new(origin: &str, recipient: &str) -> Result<Self, String> {
        let mut endpoint = Url::parse(origin).map_err(|_| CONFIG)?;
        if endpoint.scheme() != "https"
            || endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || endpoint.path() != "/"
        {
            return Err(CONFIG.into());
        }
        endpoint.set_path("/v1/reports");
        Ok(Self {
            endpoint,
            recipient: recipient.parse().map_err(|_| CONFIG)?,
        })
    }
}

fn seal(
    recipient: &Recipient,
    description: &str,
    files: Vec<(String, Vec<u8>)>,
) -> Result<Vec<u8>, String> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut total = 0usize;
    for (name, bytes) in std::iter::once((
        "description.txt".to_owned(),
        description.as_bytes().to_vec(),
    ))
    .chain(files)
    {
        // Archive member names come from the snapshot, never from the original filesystem.
        if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\', ':']) {
            return Err(INVALID.into());
        }
        total = total.saturating_add(bytes.len());
        if total > MAX_BYTES {
            return Err(LARGE.into());
        }
        zip.start_file(name, options).map_err(|_| INVALID)?;
        zip.write_all(&bytes).map_err(|_| INVALID)?;
    }
    let plaintext = zip.finish().map_err(|_| INVALID)?.into_inner();
    let payload = age::encrypt(recipient, &plaintext).map_err(|_| "FEEDBACK_ENCRYPTION")?;
    if payload.len() > MAX_BYTES {
        return Err(LARGE.into());
    }
    Ok(payload)
}

async fn upload(
    endpoint: Url,
    req_id: Uuid,
    category: &str,
    payload: Vec<u8>,
) -> Result<Receipt, String> {
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(40))
        .build()
        .map_err(|_| OFFLINE)?;
    let mut res = client
        .post(endpoint)
        .header("content-type", "application/octet-stream")
        .header("x-report-id", req_id.to_string())
        .header("x-app-version", env!("CARGO_PKG_VERSION"))
        .header("x-report-type", category)
        .body(payload)
        .send()
        .await
        .map_err(|_| OFFLINE)?;
    match res.status() {
        StatusCode::OK | StatusCode::CREATED => {}
        StatusCode::TOO_MANY_REQUESTS => return Err("FEEDBACK_RATE_LIMITED".into()),
        StatusCode::PAYLOAD_TOO_LARGE => return Err(LARGE.into()),
        status if status.is_server_error() || status == StatusCode::ACCEPTED => {
            return Err(PENDING.into())
        }
        _ => return Err(INVALID.into()),
    }
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|_| OFFLINE)? {
        if body.len() + chunk.len() > 1024 {
            return Err(PENDING.into());
        }
        body.extend_from_slice(&chunk);
    }
    let receipt: Receipt = serde_json::from_slice(&body).map_err(|_| PENDING)?;
    if !(1..=9_007_199_254_740_991).contains(&receipt.issue_number) {
        return Err(PENDING.into());
    }
    Ok(receipt)
}

#[tauri::command]
pub async fn feedback_send(
    window: tauri::WebviewWindow,
    request: FeedbackRequest,
) -> Result<Receipt, String> {
    crate::feedback::require_main(&window)?;
    request.validate()?;
    let destination = Destination::new(
        option_env!("MEOWCAL_FEEDBACK_URL").unwrap_or_default(),
        option_env!("MEOWCAL_FEEDBACK_RECIPIENT").unwrap_or_default(),
    )?;
    let payload = tokio::task::spawn_blocking(move || {
        let files = crate::feedback::selected_files(&request.snapshot_id, &request.material_ids)?;
        seal(&destination.recipient, &request.description, files)
    })
    .await
    .map_err(|_| "FEEDBACK_ENCRYPTION")??;
    upload(
        destination.endpoint,
        request.request_id,
        &request.category,
        payload,
    )
    .await
}

#[cfg(test)]
#[path = "feedback_send_tests.rs"]
mod tests;
