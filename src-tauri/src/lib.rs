// =============================================================================
// LIB.RS - Module Declarations
// =============================================================================
// This file is the "table of contents" for our Rust code.
// It tells Rust which modules (folders/files) exist in our project.
//
// Think of it like: "Hey Rust, here are all the folders that contain code"
// =============================================================================

// --- Public Modules ---
// These are the main parts of our app, organized by functionality:

/// Shared application state that persists across Tauri commands
pub mod app_state;

/// Where session logs go, and how old ones are retired
pub mod app_logging;

/// Screen capture functionality - takes screenshots of selected areas
pub mod capture;

/// Opt-in switches read from the environment
pub mod env_flags;

/// OCR (Optical Character Recognition) - reads text from images
pub mod ocr;

/// Scores how badly OCR mangled a line, so a worse re-read cannot replace a better one
pub mod ocr_corruption;

/// Decides which recognised lines are reliable enough to translate
pub mod ocr_gate;

/// Enumerating and installing the Windows OCR language packs
pub mod ocr_language_packs;

/// Decides when to tell the viewer why nothing is showing
pub mod pipeline_notices;

/// Compares a read against the last few lines, not just the last one
pub mod ocr_recent_lines;

/// What to do with a read that repeats a line already translated
pub mod pipeline_repeat_policy;

/// Tells a re-read of the subtitle already on screen from fresh dialogue
pub mod ocr_stability;

/// LLM (Large Language Model) - translates text using AI
pub mod llm;

/// Overlay window management - shows floating subtitles
pub mod overlay;

/// Ownership of the WinUI OverlayHost child process
pub mod overlay_host_process;

/// Sending overlay messages to the WinUI OverlayHost
pub mod overlay_ipc;
pub mod pipeline_deadline;
pub mod pipeline_pacing;
pub mod pipeline_session;
pub mod pipeline_translation;

/// The capture-area selector window and its desktop-snapshot background
pub mod selector_window;

/// Reading and writing the settings the UI edits
pub mod settings_service;
pub mod startup_gate;
pub mod subtitle_eval;

/// App configuration - settings like language preferences
pub mod config;

/// Reading config.json without losing it
pub mod config_store;

/// Writing config.json without overwriting something better
pub mod config_save;
pub mod engine_artifact_io;
pub mod engine_config;
pub mod engine_gpu_gate;
pub mod engine_install_transaction;
pub mod engine_launch;
pub mod engine_manifest;
pub mod engine_preflight;

/// Finding an engine that is installed but no longer registered
pub mod engine_recovery;
/// Engine readiness orchestration (status / refresh / prepare / make-ready)
pub mod engine_status;
pub mod hy_mt_paths;

/// Tauri commands - functions that JavaScript can call
pub mod commands;
mod event_payloads;

/// HTTP server for browser dev mode
pub mod http_config;
pub mod http_port;
pub mod http_server;
pub mod hy_mt_installer;
pub mod hy_mt_runtime;
pub mod legacy_translate_locally;
pub mod process_lifetime;
pub mod process_ownership;

/// IPC (Inter-Process Communication) with WinUI3 OverlayHost
pub mod ipc;

/// Synchronization utilities - safe mutex/RwLock handling with poison recovery
pub mod sync_utils;

/// What this machine can do, reported to the setup UI
pub mod system_info;

/// Quiescing the app so an update installer can replace its files
pub mod update_handoff;

/// System tray icon and menu
pub mod tray;
pub mod window_lifecycle;
/// Subprocesses that do not flash a console window over playback
pub mod windowless_command;
pub mod wizard_contracts;

/// Showing and hiding the setup wizard window
pub mod wizard_window;

// =============================================================================
// RE-EXPORTS
// =============================================================================
// This makes commonly-used types available at the top level of our crate.
// Instead of: use meowcal_sub::config::AppConfig;
// You can use: use meowcal_sub::AppConfig;

pub use config::AppConfig;
