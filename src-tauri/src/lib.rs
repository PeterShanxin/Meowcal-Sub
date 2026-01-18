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

/// App configuration - settings like language preferences
pub mod config;

/// Tauri commands - functions that JavaScript can call
pub mod commands;

/// HTTP server for browser dev mode
pub mod http_server;

// =============================================================================
// RE-EXPORTS
// =============================================================================
// This makes commonly-used types available at the top level of our crate.
// Instead of: use meowcal_sub::config::AppConfig;
// You can use: use meowcal_sub::AppConfig;

pub use config::AppConfig;
