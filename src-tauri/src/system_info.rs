//! What this machine can do, reported to the setup UI.
//!
//! The serialized field names are snake_case, unlike most payloads in this
//! crate: the frontend reads `is_copilot_plus` and `windows_ocr_available` off
//! this object directly. `tests/command_contracts.rs` pins that shape.

use serde::Serialize;
use tracing::info;

/// Information about the system, returned to the UI
#[derive(Serialize)]
pub struct SystemInfo {
    /// Operating system info
    pub os: String,
    /// CPU architecture (should be aarch64 on Copilot+ PCs)
    pub arch: String,
    /// Whether we're on a Copilot+ PC (NPU available)
    pub is_copilot_plus: bool,
    /// Whether Phi Silica API is available
    pub phi_silica_available: bool,
    /// Whether Windows OCR is available
    pub windows_ocr_available: bool,
}

/// Describe the host and the platform capabilities the UI branches on.
pub fn describe() -> SystemInfo {
    info!("Getting system info...");

    // Check what features are available
    let is_arm64 = cfg!(target_arch = "aarch64");

    // TODO: Actually detect NPU presence
    // For now, assume ARM64 Windows = Copilot+ PC
    let is_copilot_plus = is_arm64 && cfg!(target_os = "windows");

    // TODO: Check if Phi Silica is available (Windows AI APIs)
    // This will be implemented when we add LLM support
    let phi_silica_available = false;

    // Windows OCR should be available on all Windows 10/11 systems
    let windows_ocr_available = cfg!(target_os = "windows");

    let info = SystemInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        is_copilot_plus,
        phi_silica_available,
        windows_ocr_available,
    };

    info!(
        "System: {} {}, Copilot+: {}, Phi Silica: {}, OCR: {}",
        info.os,
        info.arch,
        info.is_copilot_plus,
        info.phi_silica_available,
        info.windows_ocr_available
    );

    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_the_running_host() {
        let info = describe();

        assert_eq!(info.os, std::env::consts::OS);
        assert_eq!(info.arch, std::env::consts::ARCH);
    }

    #[test]
    fn copilot_plus_requires_both_arm64_and_windows() {
        let info = describe();

        assert_eq!(
            info.is_copilot_plus,
            cfg!(target_arch = "aarch64") && cfg!(target_os = "windows")
        );
    }

    #[test]
    fn phi_silica_is_not_detected_yet_and_ocr_follows_the_platform() {
        let info = describe();

        assert!(!info.phi_silica_available);
        assert_eq!(info.windows_ocr_available, cfg!(target_os = "windows"));
    }
}
