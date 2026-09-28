//! Runs the application's dependency-free policy tests from their production
//! sources, without linking the Windows/Tauri composition root.

#[path = "../src-tauri/src/env_flags.rs"]
pub mod env_flags;
#[path = "../src-tauri/src/http_port.rs"]
pub mod http_port;
#[path = "../src-tauri/src/ocr_corruption.rs"]
pub mod ocr_corruption;
#[path = "../src-tauri/src/ocr_recent_lines.rs"]
pub mod ocr_recent_lines;
#[path = "../src-tauri/src/ocr_stability.rs"]
pub mod ocr_stability;
#[path = "../src-tauri/src/pipeline_pacing.rs"]
pub mod pipeline_pacing;
#[path = "../src-tauri/src/pipeline_repeat_policy.rs"]
pub mod pipeline_repeat_policy;
#[path = "../src-tauri/src/llm/text_utils.rs"]
pub mod text_utils;
#[path = "../src-tauri/src/translation_eligibility.rs"]
pub mod translation_eligibility;

pub mod llm {
    pub use crate::text_utils;
}
