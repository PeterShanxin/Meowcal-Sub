# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Meowcal-Sub is a local subtitle translation app for Windows ARM64 Copilot+ PCs, built with Tauri 2.0 (Rust backend + vanilla HTML/CSS/JS frontend). All OCR and translation runs locally for privacy.

## Build & Development Commands

```powershell
# IMPORTANT: Set custom target dir to avoid OneDrive file locking issues
$env:CARGO_TARGET_DIR = "D:\cargo-build"
npx tauri dev

# OR use the helper script (recommended - handles env + ARM64 toolchain)
.\dev-tauri.cmd

# Production build
npx tauri build
```

**Rust commands** (run from `src-tauri/`):
```powershell
cargo test      # Run tests
cargo clippy    # Lint
cargo fmt       # Format
```

## Browser Dev Mode (for AI Agents)

Browser dev mode allows AI agents (like Claude with browser automation) to test the UI through a standard browser while connecting to the real Rust backend via HTTP.

```powershell
# Start both backend and frontend
.\dev-browser.cmd

# Or start separately:
npm run dev:backend   # Rust HTTP server on localhost:3001
npm run dev:browser   # Static frontend on localhost:3000
```

**Architecture:**
```
┌─────────────────┐         ┌─────────────────┐
│  Browser        │──HTTP──▶│  Rust Backend   │
│  localhost:3000 │         │  localhost:3001 │
└─────────────────┘         └─────────────────┘
```

**Key files:**
- `src-tauri/src/http_server.rs` - Axum HTTP server with REST API
- `src/scripts/tauri-bridge.js` - Unified API bridge (auto-detects Tauri vs browser)
- `dev-browser.cmd` - Combined launcher script

**API Endpoints:** `GET /api/health`, `/api/settings`, `/api/translation/diagnostics`, `/api/foundry-local/status`, etc.

**Limitations:** Tauri-only features (area selector, screen capture, overlay) return 501 with helpful messages.

## Architecture

### Backend (src-tauri/src/)

| Module | Purpose |
|--------|---------|
| `main.rs` | Entry point, tray icon setup, logging config |
| `commands.rs` | Tauri IPC commands (JS ↔ Rust bridge) |
| `config.rs` | Settings structs & JSON persistence to APPDATA |
| `http_server.rs` | HTTP API for browser dev mode (Axum) |
| `capture/` | Screen capture: `graphics_capture.rs` (primary, HW-accelerated) + `win32.rs` (GDI fallback) |
| `ocr/` | Windows.Media.Ocr WinRT bindings |
| `llm/` | Translation backends with auto-fallback chain |
| `overlay/` | Floating subtitle window management |

### Translation Backend Fallback Chain

1. **Foundry Local** (primary) - OpenAI-compatible local endpoint
2. **Windows AI / Phi Silica** - Copilot Runtime (placeholder until APIs stable)
3. **Offline MT** - translateLocally binary wrapper
4. **Edge Translator** - WebView2-based (experimental)
5. **Passthrough** - Returns OCR text if all else fails

### Frontend (src/)

Three-window model:
- `index.html` - Main settings window
- `selector.html` - Full-screen transparent area selection
- `overlay.html` - Floating subtitle display

Uses vanilla JS with `invoke()` for Tauri IPC. No framework.

## Coding Conventions

- **Logging**: Use `tracing` crate (`info!`, `debug!`, `warn!`) - logs to `src-tauri/logs/meowcal-sub.log`
- **Comments**: Heavy inline comments for beginner-friendliness
- **Errors**: Use `thiserror` for custom error types
- **Async**: `tokio` runtime for async operations
- **Frontend IPC**: `window.__TAURI__` API, no npm dependencies

## Claude Code Skills

Custom slash commands available in `.claude/commands/`:

| Command | Purpose |
|---------|---------|
| `/dev` | Unified orchestrator - auto-routes to PM, UI, Tech, Fix, or Review workflow based on request |
| `/pm` | PRD & requirements clarification |
| `/ui` | UI spec & image generation prompts |
| `/tech` | Architecture & implementation planning |
| `/fix` | Bug diagnosis, fixes, tests, refactoring |
| `/review` | Release gate review (P0/P1/P2 findings) |

The `/dev` command supports chaining workflows (e.g., PM → UI → Tech) with HANDOFF.v1 structured output.

## Debugging

- **Backend logs**: `src-tauri/logs/meowcal-sub.log` (rolling daily, DEBUG level)
- **Frontend logs**: Browser DevTools (Ctrl+Shift+I in dev mode)
- **Translation diagnostics**: `get_translation_diagnostics` command shows backend availability

Common error codes: `not_supported`, `not_ready`, `not_available`, `timeout`, `backend_not_registered`

## Key Dependencies

- `tauri` v2 with `tray-icon` feature
- `windows` v0.61 (WinRT/Win32 bindings)
- `tokio` v1 (async runtime)
- `reqwest` with native-tls (HTTP client)
- `axum` + `tower-http` (HTTP server for browser dev mode)
- `tracing` + `tracing-appender` (logging)

## Configuration

App settings persist to `%APPDATA%\com.meowcal.sub\config.json`. Key settings:
- Source/target languages
- Capture interval (ms)
- Overlay appearance
- Translation backend preferences

## Platform Requirements

- Windows 11 24H2 (Build 26100+)
- Copilot+ PC (Snapdragon X, Intel Core Ultra, or AMD Ryzen AI)
- Visual Studio Build Tools with ARM64 support
