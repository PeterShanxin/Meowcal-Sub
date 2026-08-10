// =============================================================================
// MAIN.RS - Application Entry Point
// =============================================================================
// What happens when the app launches:
// 1. Set up logging (so we can see debug messages)
// 2. Create the Tauri app with our custom commands
// 3. Set up the system tray icon
// 4. Start the main window
// BROWSER DEV MODE:
// Run with --http-only flag to start only the HTTP server (no Tauri window).
// This allows testing the frontend in a browser.
// =============================================================================

// Tell Rust not to show console window on Windows when running the release build
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::sync::Arc;

use tauri::Manager;
use tracing::info;

// Import our custom modules
use meowcal_sub::app_state::AppState;
use meowcal_sub::commands;
use meowcal_sub::env_flags::env_truthy;
use meowcal_sub::ipc::{IpcMessage, IpcServer};
use meowcal_sub::sync_utils::lock_or_recover;
use meowcal_sub::{http_server, legacy_translate_locally};

// =============================================================================
// OVERLAY HOST PROCESS MANAGEMENT
// =============================================================================
// Ownership of the child itself lives in `overlay_host_process`, because the
// update handoff has to stop it too.

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

// =============================================================================
// MAIN FUNCTION
// =============================================================================

fn main() {
    // Check for --http-only flag (browser dev mode)
    let args: Vec<String> = std::env::args().collect();
    let http_only_mode = args.iter().any(|arg| arg == "--http-only");
    // --- Step 1: Set up logging ---
    meowcal_sub::app_logging::init(http_only_mode);

    info!("🐱 Meowcal Sub starting up...");

    // If --http-only flag is set, run only the HTTP server
    if http_only_mode {
        info!("🌐 Running in HTTP-only mode (browser dev mode)");
        run_http_only_mode();
        return;
    }

    // --- Step 2: Build and run the Tauri app ---
    let app = tauri::Builder::default()
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
            commands::get_ocr_languages,
            commands::install_ocr_language,
            commands::list_translation_backends,
            commands::get_translation_diagnostics,
            legacy_translate_locally::open_translate_locally_download,
            legacy_translate_locally::get_translate_locally_download_info,
            legacy_translate_locally::download_translate_locally,
            commands::translate_once,
            // Foundry Local commands
            commands::get_foundry_local_status,
            commands::list_foundry_local_models,
            commands::refresh_foundry_local_status,
            commands::prepare_foundry_local,
            commands::make_foundry_ready,
            // Overlay commands
            meowcal_sub::overlay::commands::set_overlay_click_through,
            meowcal_sub::overlay::commands::set_overlay_window_clip,
            // Foundry setup wizard commands
            commands::open_foundry_wizard,
            commands::close_foundry_wizard,
            commands::wizard_install_engine,
            commands::wizard_start_service,
            commands::wizard_test_translation,
            // In-app update
            meowcal_sub::update_handoff::prepare_for_update,
        ])
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // The update check and its apply step. `process` is what restarts the
        // app into the version the installer just wrote.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState::default())
        .on_page_load(meowcal_sub::window_lifecycle::handle_page_load)
        // Set up the system tray icon
        .setup(move |app| {
            info!("Setting up system tray...");

            // Persisted settings, re-adopting an installed-but-unregistered engine (#65)
            let loaded_config = meowcal_sub::engine_recovery::load_with_engine(app.handle());

            {
                let state = app.state::<AppState>();
                *lock_or_recover(&state.config) = loaded_config.clone();
                if let Some(region) = loaded_config.last_capture_region {
                    *lock_or_recover(&state.capture_region) = Some(region);
                }

                // If the scale factor wasn't persisted yet (older configs), fall back to the
                // current window's scale factor so restored regions capture correctly.
                let scale_factor = loaded_config.last_capture_scale_factor.or_else(|| {
                    app.get_webview_window("main")
                        .and_then(|window| window.scale_factor().ok())
                });
                *lock_or_recover(&state.capture_scale_factor) = scale_factor.unwrap_or(1.0);
                state.startup_gate.mark_ready();
            }
            meowcal_sub::hy_mt_runtime::start_configured(
                loaded_config.translation.foundry_local.managed_runtime.clone(),
            );

            meowcal_sub::window_lifecycle::restore_and_show_main(
                app.handle(),
                &loaded_config.window_preferences,
            );

            // --- Spawn OverlayHost.exe + start IPC server (WinUI features) ---
            //
            // Premium legacy selector/overlay is the default now. The WinUI OverlayHost can still be
            // enabled for experimentation via env vars.
            let use_winui_selector = env_truthy("MEOWCAL_USE_WINUI_SELECTOR");
            let use_winui_overlay = env_truthy("MEOWCAL_USE_WINUI_OVERLAY");
            let should_spawn_overlay_host = use_winui_selector || use_winui_overlay;

            if should_spawn_overlay_host {
                meowcal_sub::overlay_host_process::spawn_and_manage(app);

                // --- Start IPC server ---
                let app_handle = app.handle().clone();
                let ipc_handler = Arc::new(move |message: IpcMessage| {
                    meowcal_sub::ipc::handle_ipc_message(&app_handle, message);
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
                app.manage(meowcal_sub::overlay_host_process::OverlayHostProcess::new(
                    None,
                ));
            }

            // Create menu items and the tray icon
            meowcal_sub::tray::setup(app)?;

            Ok(())
        })
        // Keep long-lived windows available from the tray.
        .on_window_event(|window, event| {
            meowcal_sub::window_lifecycle::handle_geometry_event(window, event);
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    meowcal_sub::window_lifecycle::handle_close_requested(window, api);
                }
                tauri::WindowEvent::Destroyed => {
                    meowcal_sub::overlay_host_process::stop(window)
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("Failed to build Meowcal Sub");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            meowcal_sub::window_lifecycle::persist_main_geometry_from_app(app_handle);
            meowcal_sub::hy_mt_runtime::shutdown_owned();
            meowcal_sub::overlay_host_process::stop(app_handle);
        }
    });
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
