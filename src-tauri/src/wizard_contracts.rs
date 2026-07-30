use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardTranslationTest {
    pub translated_text: String,
    pub latency_ms: u64,
}
