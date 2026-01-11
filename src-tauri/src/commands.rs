// =============================================================================
// COMMANDS.RS - Tauri IPC Commands
// =============================================================================
// These are functions that JavaScript can call from the web UI!
// 
// The #[tauri::command] attribute is magic - it automatically:
// 1. Converts JavaScript arguments to Rust types
// 2. Converts Rust return values to JavaScript types
// 3. Handles errors gracefully
//
// In JavaScript, you call these like:
//   const result = await invoke('get_settings');
//   await invoke('save_settings', { settings: { ... } });
// =============================================================================

use crate::config::{AppConfig, CaptureRegion};
use serde::Serialize;
use tauri::State;
use std::sync::Mutex;
use tracing::info;

// =============================================================================
// APP STATE
// =============================================================================
// This holds the current state of our application.
// It's shared across all commands and persists while the app is running.

/// The application state, managed by Tauri
pub struct AppState {
    /// Current app configuration (settings)
    pub config: Mutex<AppConfig>,
    /// Whether translation is currently active
    pub is_running: Mutex<bool>,
    /// The current capture region (if set)
    pub capture_region: Mutex<Option<CaptureRegion>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            config: Mutex::new(AppConfig::default()),
            is_running: Mutex::new(false),
            capture_region: Mutex::new(None),
        }
    }
}

// =============================================================================
// SYSTEM INFO
// =============================================================================

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

/// Get information about the system
/// 
/// Called from JavaScript: `const info = await invoke('get_system_info');`
#[tauri::command]
pub fn get_system_info() -> SystemInfo {
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
        info.os, info.arch, info.is_copilot_plus, 
        info.phi_silica_available, info.windows_ocr_available
    );
    
    info
}

// =============================================================================
// SETTINGS COMMANDS
// =============================================================================

/// Get the current app settings
/// 
/// Called from JavaScript: `const settings = await invoke('get_settings');`
#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppConfig {
    info!("Getting settings...");
    let config = state.config.lock().unwrap();
    config.clone()
}

/// Save new app settings
/// 
/// Called from JavaScript: `await invoke('save_settings', { settings: { ... } });`
#[tauri::command]
pub fn save_settings(settings: AppConfig, state: State<'_, AppState>) -> Result<(), String> {
    info!("Saving settings: {:?}", settings);
    
    // Update the in-memory config
    let mut config = state.config.lock().unwrap();
    *config = settings.clone();
    
    // TODO: Persist to disk
    // For now, settings are only stored in memory
    
    Ok(())
}

// =============================================================================
// CAPTURE REGION COMMANDS
// =============================================================================

/// Set the screen region to capture
/// 
/// Called from JavaScript: 
/// `await invoke('set_capture_region', { x: 100, y: 100, width: 800, height: 100 });`
#[tauri::command]
pub fn set_capture_region(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    info!("Setting capture region: ({}, {}) {}x{}", x, y, width, height);
    
    // Validate the region
    if width <= 0 || height <= 0 {
        return Err("Width and height must be positive".to_string());
    }
    
    let region = CaptureRegion { x, y, width, height };
    
    let mut capture_region = state.capture_region.lock().unwrap();
    *capture_region = Some(region);
    
    Ok(())
}

// =============================================================================
// TRANSLATION COMMANDS
// =============================================================================

/// Start the translation process
/// 
/// This will:
/// 1. Capture the screen region periodically
/// 2. Run OCR on each capture
/// 3. Translate the recognized text
/// 4. Send results back to the overlay UI
/// 
/// Called from JavaScript: `await invoke('start_translation');`
#[tauri::command]
pub async fn start_translation(state: State<'_, AppState>) -> Result<(), String> {
    info!("Starting translation...");
    
    // Check if we have a capture region set
    {
        let region = state.capture_region.lock().unwrap();
        if region.is_none() {
            return Err("No capture region set. Please select an area first.".to_string());
        }
    }
    
    // Mark as running
    {
        let mut is_running = state.is_running.lock().unwrap();
        *is_running = true;
    }
    
    // TODO: Start the capture -> OCR -> translate loop
    // This will be implemented in the capture and ocr modules
    
    info!("✅ Translation started!");
    Ok(())
}

/// Stop the translation process
/// 
/// Called from JavaScript: `await invoke('stop_translation');`
#[tauri::command]
pub fn stop_translation(state: State<'_, AppState>) -> Result<(), String> {
    info!("Stopping translation...");
    
    let mut is_running = state.is_running.lock().unwrap();
    *is_running = false;
    
    info!("✅ Translation stopped!");
    Ok(())
}
