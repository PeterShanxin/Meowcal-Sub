use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedLocalRuntimeConfig {
    pub kind: String,
    pub executable_path: String,
    pub model_path: String,
    pub port: u16,
}
