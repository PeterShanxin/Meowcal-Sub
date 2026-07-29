use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardModelInfo {
    pub id: String,
    pub recommended: bool,
    pub hardware_tag: Option<String>,
    pub description: Option<String>,
    pub download_size: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardTranslationTest {
    pub translated_text: String,
    pub latency_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardHardwareInfo {
    pub arch: String,
    pub is_arm64: bool,
    pub has_npu: bool,
    pub has_gpu: bool,
    pub gpu_name: String,
    pub recommendation: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardDiskSpace {
    pub available_bytes: u64,
    pub available_display: String,
}
