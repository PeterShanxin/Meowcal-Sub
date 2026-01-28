// =============================================================================
// MAIN.RS - Application Entry Point
// =============================================================================
// This is where the app starts! Think of it like index.js or main.py.
//
// What happens when the app launches:
// 1. Set up logging (so we can see debug messages)
// 2. Create the Tauri app with our custom commands
// 3. Set up the system tray icon
// 4. Start the main window
//
// BROWSER DEV MODE:
// Run with --http-only flag to start only the HTTP server (no Tauri window).
// This allows testing the frontend in a browser.
// =============================================================================

// Tell Rust not to show console window on Windows when running the release build
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, PhysicalPosition, PhysicalSize,
};
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

// Import our custom modules
use meowcal_sub::commands::{self, AppState};
use meowcal_sub::config::load_config;
use meowcal_sub::http_server;
use meowcal_sub::ipc::{IpcServer, IpcMessage};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::path::PathBuf;

const LOG_RETENTION_DAYS: u64 = 7;
const DEFAULT_LOG_FILTER: &str = "meowcal_sub=debug,translation_io=info,tauri=info,axum=info,tower_http=info,hyper=warn,hyper_util=warn,reqwest=warn";

// =============================================================================
// OVERLAY HOST PROCESS MANAGEMENT
// =============================================================================

/// Managed state for tracking the OverlayHost child process
struct OverlayHostProcess(Arc<Mutex<Option<Child>>>);

/// Get the appropriate runtime identifier for the current architecture.
/// Used to locate OverlayHost.exe in the correct architecture-specific folder.
#[allow(dead_code)]
fn get_runtime_id() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "win-arm64"
    } else {
        "win-x64"
    }
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn resolve_log_filter() -> EnvFilter {
    let custom = std::env::var("MEOWCAL_LOG_FILTER").ok();
    let rust_log = std::env::var("RUST_LOG").ok();

    for candidate in [custom, rust_log].into_iter().flatten() {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }

        match EnvFilter::try_new(trimmed) {
            Ok(filter) => return filter,
            Err(err) => {
                eprintln!("Invalid log filter '{}': {}", trimmed, err);
            }
        }
    }

    EnvFilter::new(DEFAULT_LOG_FILTER)
}

fn resolve_log_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("MEOWCAL_LOG_DIR") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return std::path::PathBuf::from(trimmed);
        }
    }

    if let Ok(appdata) = std::env::var("APPDATA") {
        return std::path::PathBuf::from(appdata)
            .join("com.meowcal.sub")
            .join("logs");
    }

    std::path::PathBuf::from("logs")
}

// =============================================================================
// IPC MESSAGE HANDLER
// =============================================================================

/// Handle IPC messages from OverlayHost
fn handle_ipc_message(app: &tauri::AppHandle, message: IpcMessage) {
    info!("📨 IPC message received: {}", message.message_type);

    match message.message_type.as_str() {
        "Selector.Result" => {
            // Parse selector result and update capture region
            if let Some(payload) = message.payload {
                if let Ok(result) = serde_json::from_value::<meowcal_sub::ipc::SelectorResultPayload>(payload) {
                    info!("✅ Selector result: ({},{}) {}x{} @ {}% DPI",
                        result.region_physical.x,
                        result.region_physical.y,
                        result.region_physical.width,
                        result.region_physical.height,
                        (result.dpi / 96.0) * 100.0
                    );

                    // Update backend state with new region
                    let state: tauri::State<AppState> = app.state();
                    let new_region = meowcal_sub::config::CaptureRegion {
                        x: result.region_physical.x,
                        y: result.region_physical.y,
                        width: result.region_physical.width,
                        height: result.region_physical.height,
                    };

                    *state.capture_region.lock().unwrap() = Some(new_region.clone());

                    // Save to config
                    {
                        let mut config = state.config.lock().unwrap();
                        config.last_capture_region = Some(new_region);
                        let _ = meowcal_sub::config::save_config(app, &config);
                    }
                }
            }
        }

        "Selector.Cancelled" => {
            info!("❌ Area selection cancelled");
        }

        "Region.Updated" => {
            info!("📍 Region updated by user (drag/resize)");
            // TODO: Update backend state if we want live updates
        }

        "Overlay.SettingsClicked" | "Overlay.SettingsRequested" => {
            info!("⚙️ Settings button clicked - bringing main window to front");
            // TODO: Focus main settings window
        }

        _ => {
            warn!("⚠️ Unknown IPC message type: {}", message.message_type);
        }
    }
}

// =============================================================================
// MAIN FUNCTION
// =============================================================================

fn main() {
    // Check for --http-only flag (browser dev mode)
    let args: Vec<String> = std::env::args().collect();
    let http_only_mode = args.iter().any(|arg| arg == "--http-only");
    // --- Step 1: Set up logging ---
    if http_only_mode {
        // Log to console in HTTP-only mode for easier debugging
        let filter = resolve_log_filter();
        let subscriber = FmtSubscriber::builder()
            .with_env_filter(filter)
            .with_ansi(true)
            .pretty()
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");
    } else {
        // Log to file in normal mode - per-session with unique timestamp
        // Create logs directory if it doesn't exist
        let logs_dir = resolve_log_dir();
        std::fs::create_dir_all(&logs_dir).ok();

        // Clean up old log files (older than LOG_RETENTION_DAYS days)
        cleanup_old_logs(&logs_dir, LOG_RETENTION_DAYS);

        // Generate session-unique log filename with full timestamp
        let now = chrono::Local::now();
        let log_filename = format!("meowcal-sub_{}.log", now.format("%Y-%m-%d_%H-%M-%S"));
        let log_path = logs_dir.join(&log_filename);

        // Create a file appender for this specific session
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .expect("Failed to open log file");

        let (non_blocking, guard) = tracing_appender::non_blocking(file);

        let filter = resolve_log_filter();
        let subscriber = FmtSubscriber::builder()
            .with_env_filter(filter)
            .with_writer(non_blocking)
            .with_ansi(false) // File logs shouldn't have color codes
            .pretty()
            .finish();

        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");

        // INFO: We must keep the guard alive!
        // We'll leak it since main() runs for the whole app duration
        Box::leak(Box::new(guard));
    }

    info!("🐱 Meowcal Sub starting up...");

    // If --http-only flag is set, run only the HTTP server
    if http_only_mode {
        info!("🌐 Running in HTTP-only mode (browser dev mode)");
        run_http_only_mode();
        return;
    }

    // --- Step 2: Build and run the Tauri app ---
    tauri::Builder::default()
        // Register our custom commands (functions that JavaScript can call)
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::set_capture_region,
            commands::get_capture_region,
            commands::open_area_selector,
            commands::close_area_selector,
            commands::get_selector_snapshot,
            commands::start_translation,
            commands::stop_translation,
            commands::is_translation_running,
            commands::get_system_info,
            commands::list_translation_backends,
            commands::get_translation_diagnostics,
            commands::get_windows_ai_diagnostics,
            commands::detect_offline_mt_binary,
            commands::open_translate_locally_download,
            commands::get_translate_locally_download_info,
            commands::download_translate_locally,
            commands::translate_once,
            // Foundry Local commands
            commands::get_foundry_local_status,
            commands::list_foundry_local_models,
            commands::refresh_foundry_local_status,
            commands::prepare_foundry_local,
            // Overlay commands
            commands::set_overlay_click_through,
            commands::set_overlay_window_clip,
        ])
        .plugin(tauri_plugin_opener::init())
        // Register our app state (shared across all commands)
        .manage(AppState::default())
        // Set up the system tray icon
        .setup(move |app| {
            info!("Setting up system tray...");

            // Load persisted config
            let loaded_config = load_config(app.handle());
            {
                let state = app.state::<AppState>();
                *state.config.lock().unwrap() = loaded_config.clone();
                if let Some(region) = loaded_config.last_capture_region {
                    *state.capture_region.lock().unwrap() = Some(region);
                }
            }

            // Apply window preferences if available
            if let Some(window) = app.get_webview_window("main") {
                let prefs = &loaded_config.window_preferences;
                if let (Some(width), Some(height)) = (prefs.width, prefs.height) {
                    let _ = window.set_size(PhysicalSize::new(width, height));
                }
                if let (Some(x), Some(y)) = (prefs.x, prefs.y) {
                    let _ = window.set_position(PhysicalPosition::new(x, y));
                }
                if prefs.is_maximized {
                    let _ = window.maximize();
                }
            }

            // --- Spawn OverlayHost.exe + start IPC server (WinUI features) ---
            //
            // Premium legacy selector/overlay is the default now. The WinUI OverlayHost can still be
            // enabled for experimentation via env vars.
            let use_winui_selector = env_truthy("MEOWCAL_USE_WINUI_SELECTOR");
            let use_winui_overlay = env_truthy("MEOWCAL_USE_WINUI_OVERLAY");
            let should_spawn_overlay_host = use_winui_selector || use_winui_overlay;

            if should_spawn_overlay_host {
                let mut overlay_candidates: Vec<PathBuf> = Vec::new();

                if let Ok(resource_dir) = app.path().resource_dir() {
                    overlay_candidates.push(resource_dir.join("OverlayHost.exe"));
                }

                if let Ok(current_dir) = std::env::current_dir() {
                    if current_dir
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.eq_ignore_ascii_case("src-tauri"))
                    {
                        overlay_candidates.push(current_dir.join("resources").join("OverlayHost.exe"));
                    }
                    overlay_candidates.push(
                        current_dir
                            .join("src-tauri")
                            .join("resources")
                            .join("OverlayHost.exe"),
                    );
                }

                if let Ok(exe) = std::env::current_exe() {
                    if let Some(parent) = exe.parent() {
                        overlay_candidates.push(parent.join("OverlayHost.exe"));
                    }
                }

                let overlay_host_path = overlay_candidates
                    .iter()
                    .find(|candidate| candidate.exists())
                    .cloned();

                match overlay_host_path {
                    Some(overlay_host_path) => {
                        info!("🚀 Spawning OverlayHost from: {:?}", overlay_host_path);
                        match Command::new(&overlay_host_path).spawn() {
                            Ok(child) => {
                                info!("✅ OverlayHost spawned (PID: {})", child.id());
                                app.manage(OverlayHostProcess(Arc::new(Mutex::new(Some(child)))));
                            }
                            Err(e) => {
                                warn!("⚠️ Failed to spawn OverlayHost: {}", e);
                                app.manage(OverlayHostProcess(Arc::new(Mutex::new(None))));
                            }
                        }
                    }
                    None => {
                        warn!(
                            "⚠️ OverlayHost.exe not found. Tried: {:?}",
                            overlay_candidates
                        );
                        app.manage(OverlayHostProcess(Arc::new(Mutex::new(None))));
                    }
                }

                // --- Start IPC server ---
                let app_handle = app.handle().clone();
                let ipc_handler = Arc::new(move |message: IpcMessage| {
                    handle_ipc_message(&app_handle, message);
                });

                let ipc_server = Arc::new(IpcServer::new(ipc_handler));
                let ipc_server_clone = ipc_server.clone();

                tauri::async_runtime::spawn(async move {
                    ipc_server_clone.start().await;
                });

                // Store IPC server in app state
                app.manage(ipc_server);
            } else {
                info!(
                    "Skipping OverlayHost + IPC server (premium legacy). Set MEOWCAL_USE_WINUI_SELECTOR=1 or MEOWCAL_USE_WINUI_OVERLAY=1 to enable."
                );
                app.manage(OverlayHostProcess(Arc::new(Mutex::new(None))));
            }

            // Create menu items for the tray
            let show_item = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
            let select_area_item =
                MenuItem::with_id(app, "select_area", "Select Area", true, None::<&str>)?;
            let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            // Build the tray menu
            let menu = Menu::with_items(
                app,
                &[&show_item, &select_area_item, &settings_item, &quit_item],
            )?;

            // Create the tray icon
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("Meowcal Sub - Click to show")
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "show" => {
                            info!("Tray: Show window clicked");
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "select_area" => {
                            info!("Tray: Select area clicked");
                            // TODO: Implement area selection
                        }
                        "settings" => {
                            info!("Tray: Settings clicked");
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            info!("Tray: Quit clicked");
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button,
                        button_state,
                        ..
                    } = event
                    {
                        if button == MouseButton::Left && button_state == MouseButtonState::Up {
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            info!("✅ System tray set up successfully!");
            Ok(())
        })
        // Add cleanup on window close
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                if let Some(overlay_process) = window.try_state::<OverlayHostProcess>() {
                    if let Some(mut child) = overlay_process.0.lock().unwrap().take() {
                        let _ = child.kill();
                        info!("🛑 Killed OverlayHost process");
                    }
                }
            }
        })
        // Run the app!
        .run(tauri::generate_context!())
        .expect("Failed to run Meowcal Sub");
}

// =============================================================================
// HTTP-ONLY MODE (Browser Dev Mode)
// =============================================================================

/// Run only the HTTP server without Tauri windows.
/// Used for browser-based testing by AI agents.
fn run_http_only_mode() {
    // Create a tokio runtime for the HTTP server
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    runtime.block_on(async {
        println!();
        println!("╔════════════════════════════════════════════════════════════════╗");
        println!("║         MEOWCAL SUB - BROWSER DEV MODE                         ║");
        println!("╠════════════════════════════════════════════════════════════════╣");
        println!("║  HTTP API server starting on http://localhost:3001             ║");
        println!("║  Frontend served separately on http://localhost:3000           ║");
        println!("║                                                                ║");
        println!("║  To test in browser:                                           ║");
        println!("║    1. Run: npx serve src -l 3000 -C                            ║");
        println!("║    2. Open: http://localhost:3000                              ║");
        println!("║                                                                ║");
        println!("║  Press Ctrl+C to stop                                          ║");
        println!("╚════════════════════════════════════════════════════════════════╝");
        println!();

        if let Err(e) = http_server::start_server(3001).await {
            eprintln!("❌ HTTP server error: {}", e);
        }
    });
}

// =============================================================================
// LOG CLEANUP
// =============================================================================

/// Clean up old log files to prevent folder bloat.
/// Deletes log files older than `max_age_days` days.
fn cleanup_old_logs(logs_dir: &std::path::Path, max_age_days: u64) {
    use std::fs;
    use std::time::{Duration, SystemTime};

    let max_age = Duration::from_secs(max_age_days * 24 * 60 * 60);
    let now = SystemTime::now();

    let entries = match fs::read_dir(logs_dir) {
        Ok(entries) => entries,
        Err(_) => return, // Directory doesn't exist or can't be read
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Only process .log files
        if path.extension().is_none_or(|ext| ext != "log") {
            continue;
        }

        // Check file modification time
        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let modified = match metadata.modified() {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Delete if older than max_age
        if let Ok(age) = now.duration_since(modified) {
            if age > max_age {
                let _ = fs::remove_file(&path);
            }
        }
    }
}
