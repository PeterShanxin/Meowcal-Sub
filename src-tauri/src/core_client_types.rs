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
    pub resolved_language: String,
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
    pub model: String,
    pub version: String,
    pub storage_root: PathBuf,
    pub managed_config: Option<crate::config::ManagedLocalRuntimeConfig>,
    pub install_paths: Option<CoreInstallPaths>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrRecognizeParams {
    pub language: Option<String>,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub bgra_base64: String,
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
