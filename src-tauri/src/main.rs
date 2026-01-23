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
use tracing::info;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

// Import our custom modules
use meowcal_sub::commands::{self, AppState};
use meowcal_sub::config::load_config;
use meowcal_sub::http_server;

const LOG_RETENTION_DAYS: u64 = 7;
const DEFAULT_LOG_FILTER: &str = "meowcal_sub=debug,translation_io=info,tauri=info,axum=info,tower_http=info,hyper=warn,hyper_util=warn,reqwest=warn";

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
