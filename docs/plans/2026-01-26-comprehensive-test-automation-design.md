# Comprehensive Test Automation Design

**Date:** 2026-01-26
**Author:** Claude Code (Brainstorming Session)
**Status:** Approved

## Overview

This document outlines a comprehensive test automation strategy for Meowcal-Sub following a layered testing pyramid approach. The design provides coverage at all levels: unit tests with mocked Windows APIs for fast feedback, integration tests for module interactions, E2E tests with real Windows APIs for system-level validation, and frontend tests using Playwright for UI workflows.

## Goals

1. **Prevent regressions** - Fast feedback loop during development with tests running on every commit/PR
2. **Validate end-to-end functionality** - Test real user workflows from screen capture to subtitle display
3. **Ensure code quality** - High test coverage, performance benchmarks, and architectural integrity
4. **Support development workflow** - Easy to run locally, integrated with CI/CD

## Architecture

### Testing Pyramid

```
           ┌─────────────┐
           │   E2E Tests │  ← Real Windows APIs, Playwright UI
           │  (3-5 min)  │
          ┌──────────────┌────────────┐
          │Integration   │  Frontend  │
          │  Tests       │    Tests   │  ← Module interactions,
          │  (1-2 min)   │  (2-3 min) │     HTTP endpoints, UI workflows
         ┌────────────────────────────┐
         │     Unit Tests (fast)      │  ← Individual functions,
         │      (~30 sec)             │     mocked dependencies
         └────────────────────────────┘
```

## Layer 1: Unit Tests (Mocked Windows APIs)

**Purpose:** Test individual functions and modules in isolation with mocked dependencies. Fast feedback (seconds), runs on any platform.

**Modules to Test:**

- **`llm/context.rs`** - Context buffer management, summarization logic, token budget calculations, idle gap detection
- **`llm/prompt_router.rs`** - Prompt selection logic for various source text patterns
- **`llm/manager.rs`** - Backend fallback chain, retry logic, timeout handling, error propagation
- **`config.rs`** - Serialization/deserialization, validation, edge cases (existing tests to be expanded)
- **`commands.rs`** - Command handlers with mocked Tauri APIs
- **`ipc/protocol.rs`** - Message serialization/deserialization, validation
- **`ipc/server.rs`** - Named pipe server request/response handling
- **`http_server.rs`** - Axum endpoint handlers with mocked dependencies

**Mocking Strategy:**

- Extract traits for Windows API interactions: `OcrEngine`, `ScreenCapture`, `OverlayWindow`
- Use `mockall::automock` attribute to generate mocks
- Dependency injection for testability
- Tests run via `cargo test` (all platforms)

**Tooling:**
- `mockall` - Mock generation
- `tokio-test` - Async testing utilities

## Layer 2: Integration Tests

**Purpose:** Validate that multiple modules work together correctly. Focus on critical workflows and cross-module interactions.

**Test Scenarios:**

- **Translation Pipeline** - Full flow: OCR text → context manager → prompt router → LLM backend → result. Uses mock LLM backend but real context/prompt routing logic.
- **Context Management Integration** - Verify context manager updates summaries, prunes buffers, and resets after idle gaps with real translation sequences.
- **IPC Communication** - Named Pipe IPC server with mock OverlayHost, testing message serialization and request/response handling.
- **HTTP Server Endpoints** - Test `/api/settings`, `/api/translation/diagnostics`, `/api/foundry-local/status` using `axum-test`.
- **Config Persistence** - End-to-end save/load with temporary file fixtures.

**Organization:**

```
src-tauri/tests/
├── common/
│   ├── mod.rs         # Shared fixtures, utilities
│   └── mocks.rs       # Mock implementations
├── integration_translation.rs
├── integration_ipc.rs
└── integration_http.rs
```

**Execution:** `cargo test --test integration_*` (1-2 minutes)

## Layer 3: E2E Tests (Windows-Specific)

**Purpose:** Validate the app works with real Windows APIs, graphics hardware, and system resources. Only runs on Windows via Cargo feature.

**Test Scenarios:**

- **Screen Capture** - Real screen region capture using Windows.Graphics.Capture API with D3D11 devices
- **OCR Pipeline** - Capture screen region with known text, run Windows.Media.Ocr, verify extraction
- **Overlay Window** - Create real overlay window, test text display, click-through, auto-hide, cleanup
- **Full Translation Flow** - Capture screen → OCR → translate (mock LLM) → display in overlay
- **Resource Cleanup** - Start/stop capture repeatedly, verify no memory/handle leaks

**Infrastructure:**

- Feature-gated: `#[cfg(feature = "test-windows")]` and `#[cfg(target_os = "windows")]`
- Test fixtures in `src-tauri/test-fixtures/images/` (synthetic images with known text)
- Helper functions to spawn windows, create test regions, clean up resources
- Runs on GitHub Actions `windows-latest` runner

**Execution:** `cargo test --features test-windows --test e2e_*` (3-5 minutes)

## Layer 4: Frontend Tests (Playwright)

**Purpose:** Test UI workflows through browser dev mode, ensuring overlay, settings, and area selector behave correctly.

**Test Scenarios:**

- **Settings Window** - Form validation, language selection, save/load config, UI updates
- **Translation Controls** - Start/stop buttons, API calls, UI state changes
- **Area Selector** - Selection workflow, rectangle drawing, coordinate calculation, persistence
- **Browser Dev Mode** - Fallback behavior for unsupported features (helpful messages, 501 errors)
- **Overlay Window** - Subtitle display, font size/color changes, positioning, auto-hide

**Organization:**

```
tests/e2e/
├── pom/
│   ├── settings.page.ts
│   ├── overlay.page.ts
│   └── selector.page.ts
├── settings.spec.ts
├── translation.spec.ts
└── overlay.spec.ts
```

**Execution:** `npm run test:e2e` (2-3 minutes)

**Tooling:**
- `playwright` - E2E browser automation
- Page Object Model pattern for maintainability

## CI/CD Pipeline

**Workflow:** `.github/workflows/test.yml`

**Jobs (run in parallel):**

1. **Lint & Format Check** (all platforms, ~2 min)
   - `cargo fmt --check`
   - `cargo clippy`

2. **Unit Tests** (ubuntu-latest, ~30 sec)
   - `cargo test` (excludes Windows-specific)

3. **Integration Tests** (ubuntu-latest, ~2 min)
   - `cargo test --test integration_*`

4. **Windows E2E Tests** (windows-latest, ~5 min)
   - `cargo test --features test-windows`

5. **Frontend E2E Tests** (ubuntu-latest with xvfb, ~3 min)
   - Start backend HTTP server
   - `npm run test:e2e`

6. **Coverage Report** (optional, main branch only)
   - `cargo tarpaulin` or `llvm-cov`
   - Upload to Codecov

**Total Runtime:** ~8-10 minutes with parallel jobs

**Triggers:** Every push, pull request to main/feature branches

## File Structure

```
src-tauri/
├── src/
│   ├── config.rs           # With #[cfg(test)] modules
│   ├── llm/
│   │   ├── context.rs      # With #[cfg(test)] modules
│   │   ├── prompt_router.rs # With #[cfg(test)] modules
│   │   └── ...
│   └── ...
├── tests/
│   ├── common/
│   │   ├── mod.rs          # Shared fixtures
│   │   └── mocks.rs        # Mock implementations
│   ├── integration_translation.rs
│   ├── integration_ipc.rs
│   ├── integration_http.rs
│   └── e2e_windows.rs      # Feature-gated
├── testing/
│   ├── mod.rs              # Test helpers, trait definitions
│   └── fixtures.rs         # Test data
└── test-fixtures/
    └── images/             # For OCR tests

tests/e2e/                   # Frontend Playwright tests
├── pom/
│   ├── settings.page.ts
│   ├── overlay.page.ts
│   └── selector.page.ts
├── settings.spec.ts
├── translation.spec.ts
└── overlay.spec.ts
```

## Cargo Features

```toml
[features]
default = []
test-windows = []  # Enable Windows-specific E2E tests
```

## Test Commands

```bash
# All unit + integration (fast, any platform)
cargo test

# Include Windows E2E tests
cargo test --features test-windows

# Integration tests only
cargo test --test integration_*

# Frontend E2E tests
npm run test:e2e
```

## Dependencies

**Rust:**
- `mockall` - Mock generation
- `proptest` - Property-based testing (optional, for complex logic)
- `axum-test` - HTTP endpoint testing
- `tokio-test` - Async testing utilities

**Frontend:**
- `playwright` - E2E browser automation

## Next Steps

1. Create new git branch: `feature/test-automation`
2. Set up test infrastructure (Cargo features, dependencies, directory structure)
3. Implement unit tests for core modules (context, prompt_router, manager)
4. Implement integration tests for translation pipeline and IPC
5. Set up Windows E2E tests with real API calls
6. Configure Playwright for frontend testing
7. Create GitHub Actions workflow
8. Iterate and refine based on test results
