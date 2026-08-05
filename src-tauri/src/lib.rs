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

/// Screen capture functionality - takes screenshots of selected areas
pub mod capture;

/// OCR (Optical Character Recognition) - reads text from images
pub mod ocr;

/// Decides which recognised lines are reliable enough to translate
pub mod ocr_gate;

/// Tells a re-read of the subtitle already on screen from fresh dialogue
pub mod ocr_stability;

/// LLM (Large Language Model) - translates text using AI
pub mod llm;

/// Overlay window management - shows floating subtitles
pub mod overlay;

/// Ownership of the WinUI OverlayHost child process
pub mod overlay_host_process;
pub mod pipeline_pacing;
pub mod pipeline_session;
pub mod pipeline_translation;
pub mod startup_gate;
pub mod subtitle_eval;

/// App configuration - settings like language preferences
pub mod config;

/// Reading and writing config.json without losing it
pub mod config_store;
pub mod engine_artifact_io;
pub mod engine_config;
pub mod engine_install_transaction;
pub mod engine_manifest;
pub mod engine_preflight;

/// Finding an engine that is installed but no longer registered
pub mod engine_recovery;

/// Tauri commands - functions that JavaScript can call
pub mod commands;
mod event_payloads;

/// HTTP server for browser dev mode
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

/// Quiescing the app so an update installer can replace its files
pub mod update_handoff;

pub mod window_lifecycle;
/// Subprocesses that do not flash a console window over playback
pub mod windowless_command;
pub mod wizard_contracts;

// =============================================================================
// RE-EXPORTS
// =============================================================================
// This makes commonly-used types available at the top level of our crate.
// Instead of: use meowcal_sub::config::AppConfig;
// You can use: use meowcal_sub::AppConfig;

pub use config::AppConfig;
