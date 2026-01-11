// =============================================================================
// BUILD.RS - Build Script
// =============================================================================
// This file runs BEFORE your main code compiles. It sets up Tauri's build process.
// You usually don't need to modify this file.
// =============================================================================

fn main() {
    // This generates some code that Tauri needs to work properly
    tauri_build::build()
}
