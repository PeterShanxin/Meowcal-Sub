use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize)]
pub(super) struct HelloResult {
    pub version: String,
    pub api: u32,
    pub capabilities: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct OcrLanguagesResult {
    pub languages: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OcrInitializeResult {
    pub resolved_language: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreInstallPaths {
    pub root: PathBuf,
    pub runtime_dir: PathBuf,
    pub runtime_archive: PathBuf,
    pub executable: PathBuf,
    pub model_dir: PathBuf,
    pub model: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreStatus {
    pub installed: bool,
    pub ready: bool,
    #[serde(default)]
    pub cpu_locked: bool,
    /// `gpu` or `cpu` while Core owns a running translation engine.
    #[serde(default)]
    pub acceleration: Option<String>,
    pub model: String,
    pub version: String,
    pub storage_root: PathBuf,
    pub managed_config: Option<crate::config::ManagedLocalRuntimeConfig>,
    pub install_paths: Option<CoreInstallPaths>,
}

#[derive(Debug)]
pub enum StatusPoll {
    Status(Box<CoreStatus>),
    Busy,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrRecognizeParams {
    pub engine: meowcal_core::ocr::Engine,
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_threads: Option<u8>,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrRecognizeResult {
    pub text: String,
    pub lines: Vec<String>,
    pub boxes: Vec<crate::ocr::LineBox>,
    pub frame_width: f32,
}

pub(super) struct Request {
    pub params: serde_json::Value,
    pub payload: Vec<u8>,
}

impl From<serde_json::Value> for Request {
    fn from(params: serde_json::Value) -> Self {
        Self {
            params,
            payload: Vec::new(),
        }
    }
}

impl Request {
    pub(super) fn ocr(params: OcrRecognizeParams, payload: Vec<u8>) -> Result<Self, String> {
        let expected = (params.stride as usize).checked_mul(params.height as usize);
        if params.width == 0
            || params.height == 0
            || params.width > 4096
            || params.height > 4096
            || params.stride != params.width * 4
            || !(1..=30_000).contains(&params.timeout_ms)
            || payload.len() > meowcal_core::ocr::MAX_FRAME_BYTES
            || expected != Some(payload.len())
        {
            return Err("CORE_OCR_FRAME_INVALID".to_string());
        }
        Ok(Self {
            params: serde_json::json!(params),
            payload,
        })
    }
}
