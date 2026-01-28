# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Meowcal-Sub is a local LLM-powered subtitle translation app for Windows, built with Tauri 2.0 (Rust backend + vanilla HTML/CSS/JS frontend). It captures any screen region, performs OCR, translates via local LLM backends (like Foundry Local), and displays subtitles in a floating overlay. All processing runs locally for privacy.

Key features include context-aware translation (with memory + recent subtitle context), hardware-accelerated screen capture (Windows.Graphics.Capture API), and automatic backend fallback chain.

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

**Environment Variables:**
- `CARGO_TARGET_DIR` - Set to avoid OneDrive file locking (e.g., `D:\cargo-build`)
- `MEOWCAL_LOG_DIR` - Override log directory (default: `%APPDATA%\com.meowcal.sub\logs`)
- `MEOWCAL_LOG_FILTER` or `RUST_LOG` - Override log filter (default: `meowcal_sub=debug,translation_io=info,tauri=info,axum=info`)

**Logs:**
- Location: `%APPDATA%\com.meowcal.sub\logs\meowcal-sub_<timestamp>.log`
- Format: Per-session log files with full timestamp (e.g., `meowcal-sub_2025-01-23_14-30-45.log`)
- Retention: 7 days (auto-cleanup on startup)
- Level: DEBUG for meowcal_sub, INFO for most dependencies

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
| `main.rs` | Entry point, tray icon setup, logging config (supports `--http-only` for browser dev mode) |
| `commands.rs` | Tauri IPC commands (JS ↔ Rust bridge) |
| `config.rs` | Settings structs & JSON persistence to APPDATA |
| `http_server.rs` | Axum HTTP server for browser dev mode (REST API) |
| `lib.rs` | Library exports for commands and config |
| `capture/` | Screen capture: `graphics_capture.rs` (primary, HW-accelerated D3D11) + `win32.rs` (GDI fallback) + `d3d.rs` (Direct3D helpers) |
| `ocr/` | Windows.Media.Ocr WinRT bindings (`windows_ocr.rs`) |
| `llm/` | Translation system: `manager.rs` (orchestrator), `foundry_local.rs`, `offline_mt.rs`, `phi_silica.rs`, `context.rs` (context-aware memory), `prompt_router.rs` (dynamic prompt selection) |
| `overlay/` | Floating subtitle window management |

### Translation Backend Fallback Chain

1. **Foundry Local** (primary) - OpenAI-compatible local LLM endpoint with context-aware translation
   - Supports memory context (summarized history) + recent subtitles
   - Dynamic prompt routing based on content characteristics
   - Configurable context budget (% of model window) and summarization cooldown
2. **Offline MT** - translateLocally binary wrapper (local ONNX models)
3. **Windows AI** - Phi Silica via Windows.AI.LanguageModel (experimental)
4. **Passthrough** - Returns OCR text if all else fails

**Context-Aware Translation (Foundry Local only):**
The app maintains a rolling context buffer of recent subtitles and periodically summarizes them into long-term memory. This context is injected into translation prompts to improve consistency and handle references. Key components:
- `context.rs` - Manages context buffer, memory summaries, and token budget
- `prompt_router.rs` - Selects prompts based on source text characteristics
- Context levels: `off`, `memoryOnly`, `memoryAndRecent`
- Automatic context reset after idle gaps (configurable via `contextResetGapMs`)

### Frontend (src/)

Three-window model:
- `index.html` - Main settings window
- `selector.html` - Full-screen transparent area selection
- `overlay.html` - Floating subtitle display

**Frontend Scripts (src/scripts/):**
- `main.js` - Settings window logic
- `selector.js` - Area selection UI
- `overlay.js` - Subtitle display logic
- `tauri-bridge.js` - Unified API bridge that auto-detects Tauri vs browser mode
  - In Tauri mode: uses `window.__TAURI__.invoke()`
  - In browser mode: makes HTTP requests to `localhost:3001/api/*`
- `settings.js` - Settings management utilities

Uses vanilla JS with no npm dependencies. All Tauri IPC goes through `tauri-bridge.js` for browser dev mode compatibility.

## Coding Conventions

- **Logging**: Use `tracing` crate (`info!`, `debug!`, `warn!`) - logs to `%APPDATA%\\com.meowcal.sub\\logs\\meowcal-sub_<timestamp>.log` (override with `MEOWCAL_LOG_DIR`)
- **Comments**: Heavy inline comments for beginner-friendliness
- **Errors**: Use `thiserror` for custom error types
- **Async**: `tokio` runtime for async operations
- **Frontend IPC**: `window.__TAURI__` API, no npm dependencies

## Claude Code Skills & Superpowers

**Built-in Superpowers** (invoked automatically by Claude):
- `brainstorming` - Explores requirements before implementation
- `writing-plans` - Creates implementation plans
- `systematic-debugging` - Bug diagnosis and fixes
- `test-driven-development` - TDD workflow
- `requesting-code-review` - Pre-merge verification
- `receiving-code-review` - Handle review feedback
- `verification-before-completion` - Pre-commit verification
- `executing-plans` - Execute implementation plans
- `using-git-worktrees` - Isolated workspace management

**Custom Skills** (available in `.claude/skills/`):

| Command | Purpose | Source |
|---------|---------|--------|
| `/ui` | UI spec & image generation prompts | Custom |
| `/pdf` | PDF manipulation, form extraction, document generation | [Anthropic Skills](https://github.com/anthropics/skills) |
| `/docx` | Word document creation and editing | [Anthropic Skills](https://github.com/anthropics/skills) |
| `/webapp-testing` | Web app testing with Playwright (useful for browser dev mode) | [Anthropic Skills](https://github.com/anthropics/skills) |
| `/mcp-builder` | Create MCP servers to integrate external services | [Anthropic Skills](https://github.com/anthropics/skills) |
| `/skill-creator` | Create new custom skills | [Anthropic Skills](https://github.com/anthropics/skills) |

Each skill is a subdirectory containing `SKILL.md` with the skill definition.

## Debugging

- **Backend logs**: `%APPDATA%\\com.meowcal.sub\\logs\\meowcal-sub_<timestamp>.log` (DEBUG level; override with `MEOWCAL_LOG_DIR`)
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
- Overlay appearance (font, colors, positioning)
- Translation backend configuration:
  - Foundry Local endpoint URL
  - Context-aware settings (`enableContextAware`, `contextLevel`, `contextRecentCount`, `contextBudgetPercent`, etc.)
  - Offline MT binary path
  - Feature flags for each backend
  - Timeouts and retry behavior

## Platform Requirements

- Windows 10/11
- Visual Studio Build Tools (for compilation)
- Local LLM backend (e.g., Foundry Local) for translation
