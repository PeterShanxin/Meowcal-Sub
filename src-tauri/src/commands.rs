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
use crate::capture;
use crate::ocr::WindowsOcr;
use crate::llm::{PhiSilica, TranslationProvider};
use crate::overlay;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::watch;
use tracing::{info, warn, debug};

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
    /// Stop signal sender for the translation loop
    /// When we send `true` through this, the loop stops
    pub stop_signal: Mutex<Option<watch::Sender<bool>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            config: Mutex::new(AppConfig::default()),
            is_running: Mutex::new(false),
            capture_region: Mutex::new(None),
            stop_signal: Mutex::new(None),
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

/// Get the current capture region (if set)
/// 
/// Called from JavaScript: `const region = await invoke('get_capture_region');`
#[tauri::command]
pub fn get_capture_region(state: State<'_, AppState>) -> Option<CaptureRegion> {
    let region = state.capture_region.lock().unwrap();
    region.clone()
}

/// Open the area selector overlay window
/// 
/// Called from JavaScript: `await invoke('open_area_selector');`
#[tauri::command]
pub async fn open_area_selector(app: AppHandle) -> Result<(), String> {
    info!("Opening area selector...");
    
    if let Some(window) = app.get_webview_window("selector") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        info!("✅ Area selector opened!");
    } else {
        return Err("Selector window not found".to_string());
    }
    
    Ok(())
}

/// Close the area selector overlay window
/// 
/// Called from JavaScript: `await invoke('close_area_selector');`
#[tauri::command]
pub async fn close_area_selector(app: AppHandle) -> Result<(), String> {
    info!("Closing area selector...");
    
    if let Some(window) = app.get_webview_window("selector") {
        window.hide().map_err(|e| e.to_string())?;
        info!("✅ Area selector closed!");
    } else {
        return Err("Selector window not found".to_string());
    }
    
    Ok(())
}

// =============================================================================
// TRANSLATION COMMANDS
// =============================================================================

/// Payload sent to the frontend with translation results
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationPayload {
    /// The original text from OCR
    pub original: String,
    /// The translated text
    pub translated: String,
    /// Unix timestamp in milliseconds
    pub timestamp: u64,
}

/// Payload sent to the frontend to report capture status
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStatusPayload {
    /// Whether the capture is using the fallback method (GDI)
    pub using_fallback: bool,
    /// Human-readable message about capture status
    pub message: String,
    /// Is this an error state?
    pub is_error: bool,
}

/// Start the translation process
/// 
/// This will:
/// 1. Capture the screen region periodically
/// 2. Run OCR on each capture
/// 3. Translate the recognized text
/// 4. Send results back to the overlay UI via events
/// 
/// Called from JavaScript: `await invoke('start_translation');`
#[tauri::command]
pub async fn start_translation(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    info!(">>> START_TRANSLATION COMMAND CALLED <<<");
    info!("Starting translation...");
    
    // Check if already running
    {
        let is_running = state.is_running.lock().unwrap();
        if *is_running {
            return Err("Translation is already running".to_string());
        }
    }
    
    // Get the capture region
    let region = {
        let region_guard = state.capture_region.lock().unwrap();
        match region_guard.clone() {
            Some(r) => r,
            None => return Err("No capture region set. Please select an area first.".to_string()),
        }
    };
    
    // Get the capture interval from config
    let interval_ms = {
        let config = state.config.lock().unwrap();
        config.capture_interval_ms
    };
    
    // Get target language from config
    let target_language = {
        let config = state.config.lock().unwrap();
        config.target_language.clone()
    };
    
    // Create a stop signal channel
    // The sender stays here, the receiver goes to the spawned task
    let (stop_tx, mut stop_rx) = watch::channel(false);
    
    // Store the sender so stop_translation can use it
    {
        let mut stop_signal = state.stop_signal.lock().unwrap();
        *stop_signal = Some(stop_tx);
    }
    
    // Mark as running
    {
        let mut is_running = state.is_running.lock().unwrap();
        *is_running = true;
    }
    
    info!("✅ Translation started! Interval: {}ms, Target: {}", interval_ms, target_language);
    
    // Show the overlay and send the capture region to it
    if let Err(e) = overlay::show_overlay(&app) {
        warn!("⚠️ Failed to show overlay: {}", e);
    }
    if let Err(e) = overlay::update_overlay_region(&app, &region) {
        warn!("⚠️ Failed to update overlay region: {}", e);
    }
    
    // Spawn the background translation loop
    tokio::spawn(async move {
        // Initialize OCR engine
        let ocr = match WindowsOcr::new() {
            Ok(o) => o,
            Err(e) => {
                warn!("❌ Failed to initialize OCR: {}", e);
                return;
            }
        };
        
        // Initialize translator
        let translator = PhiSilica::new();
        let translator_available = translator.is_available();
        info!("Using translator: {}", translator.name());
        if !translator_available {
            info!("Translation provider not available; using OCR text for overlay output");
        }
        
        // Keep track of last OCR text to avoid duplicate processing
        let mut last_text = String::new();
        
        // Track if we've already notified about fallback
        let mut fallback_notified = false;
        
        // Initialize persistent capture session (no border flashing)
        let session_initialized = match capture::init_capture_session() {
            Ok(_) => {
                info!("✅ Persistent capture session initialized");
                true
            }
            Err(e) => {
                warn!("⚠️ Failed to init persistent session, will use per-frame capture: {}", e);
                false
            }
        };
        
        loop {
            // Check if we should stop
            if *stop_rx.borrow() {
                info!("🛑 Stop signal received, exiting loop");
                break;
            }
            
            // Also check for channel changes (non-blocking)
            if stop_rx.has_changed().unwrap_or(false) {
                let _ = stop_rx.borrow_and_update();
                if *stop_rx.borrow() {
                    info!("🛑 Stop signal received, exiting loop");
                    break;
                }
            }
            
            debug!("📸 Capturing region: {:?}", region);
            
            // Step 1: Capture screen region
            // If persistent session is available, use it (no border flashing)
            // Otherwise fall back to smart_capture which creates new session each time
            let capture_result = if session_initialized {
                match capture::capture_with_session(&region) {
                    Ok(result) => result,
                    Err(e) => {
                        warn!("⚠️ Session capture failed: {}", e);
                        let status = CaptureStatusPayload {
                            using_fallback: false,
                            message: format!("Capture failed: {}", e),
                            is_error: true,
                        };
                        let _ = app.emit("capture-status", status);
                        tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                        continue;
                    }
                }
            } else {
                // Fallback to smart_capture (creates new session each time)
                match capture::smart_capture(&region) {
                    Ok((result, fallback)) => {
                        if fallback && !fallback_notified {
                            fallback_notified = true;
                            let status = CaptureStatusPayload {
                                using_fallback: true,
                                message: "Using GDI fallback - video content may not capture correctly".to_string(),
                                is_error: false,
                            };
                            let _ = app.emit("capture-status", status);
                        }
                        result
                    }
                    Err(e) => {
                        warn!("⚠️ Capture failed: {}", e);
                        let status = CaptureStatusPayload {
                            using_fallback: false,
                            message: format!("Capture failed: {}", e),
                            is_error: true,
                        };
                        let _ = app.emit("capture-status", status);
                        tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                        continue;
                    }
                }
            };
            
            // Step 2: Run OCR
            let ocr_result = match ocr.recognize(
                &capture_result.data,
                capture_result.width,
                capture_result.height,
            ).await {
                Ok(result) => result,
                Err(e) => {
                    warn!("⚠️ OCR failed: {}", e);
                    tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                    continue;
                }
            };
            
            // Skip if empty or same as last frame
            if ocr_result.is_empty() {
                debug!("No text detected, skipping");
                tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                continue;
            }
            
            let current_text = ocr_result.text.trim().to_string();
            if current_text == last_text {
                debug!("Same text as last frame, skipping");
                tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
                continue;
            }
            
            last_text = current_text.clone();
            info!("📝 OCR detected: {}", current_text);
            
            // Step 3: Translate (or fall back to OCR text if translator isn't available)
            let translated = if translator_available {
                match translator.translate(&current_text, &target_language).await {
                    Ok(t) => t,
                    Err(e) => {
                        warn!("⚠️ Translation failed: {}", e);
                        current_text.clone()
                    }
                }
            } else {
                current_text.clone()
            };
            
            info!("🌐 Translated: {}", translated);
            
            // Step 4: Emit event to frontend
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            
            let payload = TranslationPayload {
                original: current_text,
                translated,
                timestamp,
            };
            
            if let Err(e) = app.emit("translation-update", payload) {
                warn!("⚠️ Failed to emit event: {}", e);
            }
            
            // Wait for next iteration
            tokio::time::sleep(Duration::from_millis(interval_ms as u64)).await;
        }
        
        // Close the capture session when loop ends
        capture::close_capture_session();
        info!("Translation loop ended");
    });
    
    Ok(())
}

/// Stop the translation process
/// 
/// Called from JavaScript: `await invoke('stop_translation');`
#[tauri::command]
pub fn stop_translation(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    info!("Stopping translation...");
    
    // Send the stop signal
    {
        let stop_signal = state.stop_signal.lock().unwrap();
        if let Some(ref sender) = *stop_signal {
            let _ = sender.send(true);
            info!("Stop signal sent");
        }
    }
    
    // Clear the stop signal sender
    {
        let mut stop_signal = state.stop_signal.lock().unwrap();
        *stop_signal = None;
    }
    
    // Mark as not running
    {
        let mut is_running = state.is_running.lock().unwrap();
        *is_running = false;
    }
    
    // Close the capture session
    capture::close_capture_session();
    
    // Hide the overlay
    if let Err(e) = overlay::hide_overlay(&app) {
        warn!("⚠️ Failed to hide overlay: {}", e);
    }
    
    info!("✅ Translation stopped!");
    Ok(())
}

