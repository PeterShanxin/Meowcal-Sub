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

/// LLM (Large Language Model) - translates text using AI
pub mod llm;

/// Overlay window management - shows floating subtitles
pub mod overlay;
pub mod pipeline_session;
pub mod subtitle_eval;

/// App configuration - settings like language preferences
pub mod config;
pub mod engine_artifact_io;
pub mod engine_config;
pub mod engine_install_transaction;
pub mod engine_manifest;
pub mod engine_preflight;

/// Tauri commands - functions that JavaScript can call
pub mod commands;
mod event_payloads;

/// HTTP server for browser dev mode
pub mod http_server;
pub mod hy_mt_installer;
pub mod hy_mt_runtime;

/// IPC (Inter-Process Communication) with WinUI3 OverlayHost
pub mod ipc;

/// Synchronization utilities - safe mutex/RwLock handling with poison recovery
pub mod sync_utils;
pub mod window_lifecycle;
pub mod wizard_contracts;

// =============================================================================
// RE-EXPORTS
// =============================================================================
// This makes commonly-used types available at the top level of our crate.
// Instead of: use meowcal_sub::config::AppConfig;
// You can use: use meowcal_sub::AppConfig;

pub use config::AppConfig;
