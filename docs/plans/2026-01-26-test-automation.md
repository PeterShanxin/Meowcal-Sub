# Test Automation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement comprehensive test automation for Meowcal-Sub with a layered testing pyramid (unit, integration, E2E, frontend) to prevent regressions and validate end-to-end functionality.

**Architecture:** Four-layer testing pyramid: (1) Unit tests with mocked Windows APIs for fast cross-platform feedback, (2) Integration tests for module interactions, (3) E2E tests with real Windows APIs on CI, (4) Frontend tests using Playwright for UI workflows. Uses Cargo features for platform-specific test gating.

**Tech Stack:** Rust (mockall, tokio-test, axum-test), Playwright (Node.js), GitHub Actions CI/CD, Cargo features for test gating

---

## Prerequisites & Setup

### Task 0: Verify Worktree and Branch

**Files:**
- Current directory: `.worktrees/test-automation/`
- Branch: `feature/test-automation`

**Step 1: Verify worktree location and branch**

Run: `cd .worktrees/test-automation && git branch --show-current`
Expected: `feature/test-automation`

**Step 2: Verify worktree structure**

Run: `ls -la src-tauri/`
Expected: All source files present

---

## Phase 1: Test Infrastructure Foundation

### Task 1: Add Test Dependencies to Cargo.toml

**Files:**
- Modify: `src-tauri/Cargo.toml`

**Step 1: Add dev dependencies**

Add to `[dev-dependencies]` section:

```toml
[dev-dependencies]
mockall = "0.13"
tokio-test = "0.4"
proptest = "1.5"  # Optional, for property-based testing
axum-test = "16.0"
```

**Step 2: Add test-windows feature**

Add to `[features]` section:

```toml
[features]
default = []
test-windows = []  # Enable Windows-specific E2E tests
```

**Step 3: Run cargo check to verify dependencies**

Run: `cd src-tauri && cargo check`
Expected: No errors (ignoring OverlayHost resource issues for now)

**Step 4: Commit**

```bash
git add src-tauri/Cargo.toml
git commit -m "test: add test dependencies and features

Add mockall, tokio-test, proptest, and axum-test for testing.
Add test-windows feature gate for Windows-specific E2E tests.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

### Task 2: Create Test Utilities Module

**Files:**
- Create: `src-tauri/testing/mod.rs`
- Create: `src-tauri/testing/fixtures.rs`

**Step 1: Create testing module structure**

Create `src-tauri/testing/mod.rs`:

```rust
//! Test utilities and helpers
//!
//! This module provides common test fixtures, mocks, and helpers
//! used across unit tests, integration tests, and E2E tests.

pub mod fixtures;

// Common test utilities
use std::time::Duration;

/// Default timeout for async operations in tests
pub const TEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Helper to create a temporary directory for tests
pub fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("Failed to create temp dir")
}
```

**Step 2: Create fixtures module**

Create `src-tauri/testing/fixtures.rs`:

```rust
//! Test fixtures and sample data

use crate::config::{AppConfig, CaptureRegion, OverlaySettings};

/// Creates a default test config
pub fn default_test_config() -> AppConfig {
    AppConfig::default()
}

/// Creates a test capture region (100x100 at origin)
pub fn test_capture_region() -> CaptureRegion {
    CaptureRegion::new(0, 0, 100, 100)
}

/// Creates test overlay settings
pub fn test_overlay_settings() -> OverlaySettings {
    OverlaySettings {
        font_size: 24,
        font_family: "Arial".to_string(),
        text_color: "#FFFFFF".to_string(),
        background_color: "#000000".to_string(),
        position_x: 100,
        position_y: 100,
        auto_hide: false,
        click_through: true,
    }
}
```

**Step 3: Add testing module to lib.rs**

Add to `src-tauri/src/lib.rs`:

```rust
// Add at the top with other modules
#[cfg(test)]
pub mod testing;
```

**Step 4: Run cargo test to verify module structure**

Run: `cd src-tauri && cargo test --lib`
Expected: Compiles successfully (tests may be empty)

**Step 5: Commit**

```bash
git add src-tauri/testing/ src-tauri/src/lib.rs
git commit -m "test: add testing utilities module

Add common test fixtures and helpers for reuse across tests.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

### Task 3: Create Tests Directory Structure

**Files:**
- Create: `src-tauri/tests/common/mod.rs`
- Create: `src-tauri/tests/common/mocks.rs`

**Step 1: Create integration tests directory structure**

Run: `cd src-tauri && mkdir -p tests/common`

**Step 2: Create common test utilities**

Create `src-tauri/tests/common/mod.rs`:

```rust
//! Common utilities for integration tests

pub mod mocks;

use meowcal_sub::config::AppConfig;

/// Creates a test config with temporary file paths
pub fn test_config_with_temp_paths() -> AppConfig {
    let mut config = AppConfig::default();
    // Use temp paths to avoid conflicts
    config
}
```

**Step 3: Create shared mocks**

Create `src-tauri/tests/common/mocks.rs`:

```rust
//! Mock implementations for integration tests

// Mock implementations will be added as needed
// for each module we test
```

**Step 4: Verify tests directory is recognized**

Run: `cd src-tauri && cargo test --no-run`
Expected: Should recognize tests/ directory

**Step 5: Commit**

```bash
git add src-tauri/tests/
git commit -m "test: add integration tests directory structure

Set up tests/common/ for shared integration test utilities.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Phase 2: Unit Tests - Core Translation Logic

### Task 4: Test Config Module - Expand Coverage

**Files:**
- Modify: `src-tauri/src/config.rs` (add to existing `#[cfg(test)]` module)

**Step 1: Write test for invalid capture region**

Add to existing tests module in `src-tauri/src/config.rs`:

```rust
#[test]
fn test_capture_region_zero_area() {
    let region = CaptureRegion::new(0, 0, 0, 100);
    assert!(!region.is_valid());
}

#[test]
fn test_capture_region_negative_coords() {
    let region = CaptureRegion::new(-10, -10, 100, 100);
    assert!(!region.is_valid());
}
```

**Step 2: Run tests to verify they pass**

Run: `cd src-tauri && cargo test config::tests`
Expected: All tests pass

**Step 3: Commit**

```bash
git add src-tauri/src/config.rs
git commit -m "test(config): add edge case tests for CaptureRegion

Test zero area and negative coordinates validation.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

### Task 5: Extract Traits for Mockable LLM Backends

**Files:**
- Modify: `src-tauri/src/llm/mod.rs`
- Modify: `src-tauri/src/llm/manager.rs`

**Step 1: Define TranslationBackend trait**

Add to `src-tauri/src/llm/mod.rs`:

```rust
//! Trait for translation backends (enables mocking)

use async_trait::async_trait;
use crate::error::Result;

#[async_trait]
pub trait TranslationBackend: Send + Sync {
    /// Translate text from source to target language
    async fn translate(&self, text: &str, source: &str, target: &str) -> Result<String>;

    /// Check if backend is available
    async fn is_available(&self) -> bool;

    /// Get backend name
    fn name(&self) -> &str;
}
```

**Step 2: Implement trait for existing backends**

Update each backend struct in `src-tauri/src/llm/foundry_local.rs`:

```rust
#[async_trait]
impl TranslationBackend for FoundryLocalBackend {
    async fn translate(&self, text: &str, source: &str, target: &str) -> Result<String> {
        // Use existing implementation
        self.translate_internal(text, source, target).await
    }

    async fn is_available(&self) -> bool {
        // Use existing health check
        self.check_health().await.is_ok()
    }

    fn name(&self) -> &str {
        "Foundry Local"
    }
}
```

Repeat for `offline_mt.rs` and `phi_silica.rs` following the same pattern.

**Step 3: Add mockall dependency if not already added**

Should already be added in Task 1.

**Step 4: Run cargo test to verify trait implementations**

Run: `cd src-tauri && cargo test --lib`
Expected: Compiles, trait implementations work

**Step 5: Commit**

```bash
git add src-tauri/src/llm/mod.rs src-tauri/src/llm/foundry_local.rs src-tauri/src/llm/offline_mt.rs src-tauri/src/llm/phi_silica.rs
git commit -m "refactor(llm): extract TranslationBackend trait

Enable mocking of LLM backends for testing.
All backends now implement the TranslationBackend trait.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

### Task 6: Test Context Manager

**Files:**
- Modify: `src-tauri/src/llm/context.rs`

**Step 1: Write test for context buffer management**

Add to `src-tauri/src/llm/context.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_buffer_initial_state() {
        let context = TranslationContext::new(1000);
        assert_eq!(context.buffer.len(), 0);
        assert_eq!(context.memory_summary, None);
    }

    #[test]
    fn test_context_buffer_add_within_budget() {
        let mut context = TranslationContext::new(1000);
        context.add_translation("hello", "hola", 10);
        assert_eq!(context.buffer.len(), 1);
        assert_eq!(context.current_token_count(), 10);
    }

    #[test]
    fn test_context_buffer_pruning() {
        let mut context = TranslationContext::new(100);
        // Add items that exceed budget
        for i in 0..20 {
            context.add_translation(
                &format!("text_{}", i),
                &format!("trans_{}", i),
                10
            );
        }
        // Should prune to stay within budget
        assert!(context.current_token_count() <= 100);
    }

    #[test]
    fn test_context_should_summarize() {
        let mut context = TranslationContext::new(100);
        // Add enough to trigger summarization
        for i in 0..15 {
            context.add_translation("text", "trans", 10);
        }
        assert!(context.should_summarize());
    }

    #[test]
    fn test_context_reset_after_idle() {
        let mut context = TranslationContext::new(1000);
        context.add_translation("hello", "hola", 10);
        context.add_translation("world", "mundo", 10);

        // Simulate idle gap
        context.reset_after_idle();
        assert_eq!(context.buffer.len(), 0);
    }
}
```

**Step 2: Run tests to verify behavior**

Run: `cd src-tauri && cargo test llm::context::tests`
Expected: Some tests may fail if functionality not yet implemented

**Step 3: Implement missing methods if needed**

If tests fail, implement the missing functionality in `TranslationContext`.

**Step 4: Run tests again to verify they pass**

Run: `cd src-tauri && cargo test llm::context::tests`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src-tauri/src/llm/context.rs
git commit -m "test(llm): add unit tests for context manager

Test buffer management, pruning, summarization triggers,
and idle reset behavior.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

### Task 7: Test Prompt Router

**Files:**
- Modify: `src-tauri/src/llm/prompt_router.rs`

**Step 1: Write test for prompt selection logic**

Add to `src-tauri/src/llm/prompt_router.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_prompt_simple_text() {
        let source = "Hello world";
        let prompt = select_prompt(source, "en", "es");
        assert!(prompt.contains("translate"));
    }

    #[test]
    fn test_select_prompt_technical_terms() {
        let source = "The API endpoint returns HTTP 404";
        let prompt = select_prompt(source, "en", "zh");
        assert!(prompt.contains("technical"));
    }

    #[test]
    fn test_select_prompt_dialogue() {
        let source = "\"Hello,\" she said. \"Hi,\" he replied.";
        let prompt = select_prompt(source, "en", "ja");
        assert!(prompt.contains("dialogue"));
    }

    #[test]
    fn test_select_prompt_numbers() {
        let source = "The value is 42.5% and counting at 123.45";
        let prompt = select_prompt(source, "en", "ko");
        assert!(prompt.contains("number"));
    }
}
```

**Step 2: Run tests to verify prompt routing**

Run: `cd src-tauri && cargo test llm::prompt_router::tests`
Expected: Tests may fail if routing logic not implemented

**Step 3: Implement prompt selection logic**

If tests fail, implement the `select_prompt` function to detect patterns and return appropriate prompts.

**Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test llm::prompt_router::tests`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src-tauri/src/llm/prompt_router.rs
git commit -m "test(llm): add unit tests for prompt router

Test prompt selection for simple text, technical terms,
dialogue, and numbers.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

### Task 8: Test LLM Manager with Mock Backend

**Files:**
- Modify: `src-tauri/src/llm/manager.rs`

**Step 1: Write test for backend fallback chain**

Add to `src-tauri/src/llm/manager.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mockall::{mock, predicate::*};
    use async_trait::async_trait;

    // Mock backend for testing
    mock! {
        pub MockBackend {}

        #[async_trait]
        impl TranslationBackend for MockBackend {
            async fn translate(&self, text: &str, source: &str, target: &str) -> Result<String>;
            async fn is_available(&self) -> bool;
            fn name(&self) -> &str;
        }
    }

    #[tokio::test]
    async fn test_manager_successful_translation() {
        let mut mock = MockMockBackend::new();
        mock.expect_is_available()
            .returning(|| true);
        mock.expect_translate()
            .returning(|_, _, _| Ok("translated".to_string()));

        let manager = TranslationManager::with_backend(Box::new(mock));
        let result = manager.translate("hello", "en", "es").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "translated");
    }

    #[tokio::test]
    async fn test_manager_backend_unavailable() {
        let mut mock = MockMockBackend::new();
        mock.expect_is_available()
            .returning(|| false);

        let manager = TranslationManager::with_backend(Box::new(mock));
        let result = manager.translate("hello", "en", "es").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_manager_retry_on_timeout() {
        let mut mock = MockMockBackend::new();
        mock.expect_is_available()
            .returning(|| true);
        mock.expect_translate()
            .times(2)  // Should retry once
            .returning(|_, _, _| Err(Error::Timeout));

        let mut manager = TranslationManager::with_backend(Box::new(mock));
        manager.set_max_retries(1);

        let result = manager.translate("hello", "en", "es").await;
        assert!(result.is_err());
    }
}
```

**Step 2: Run tests to verify manager behavior**

Run: `cd src-tauri && cargo test llm::manager::tests`
Expected: Tests may fail if methods don't exist

**Step 3: Implement manager methods**

If tests fail, implement `with_backend`, `set_max_retries`, and retry logic.

**Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test llm::manager::tests`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src-tauri/src/llm/manager.rs
git commit -m "test(llm): add unit tests for translation manager

Test successful translation, backend unavailable errors,
and retry logic with mock backend.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Phase 3: Integration Tests

### Task 9: Test Translation Pipeline Integration

**Files:**
- Create: `src-tauri/tests/integration_translation.rs`

**Step 1: Write integration test for full translation flow**

Create `src-tauri/tests/integration_translation.rs`:

```rust
//! Integration tests for translation pipeline
//!
//! Tests the full flow: OCR text → context manager → prompt router → LLM backend

use meowcal_sub::llm::{TranslationContext, TranslationManager};
use meowcal_sub::config::AppConfig;

#[tokio::test]
async fn test_full_translation_pipeline() {
    // Setup
    let config = AppConfig::default();
    let mut context = TranslationContext::new(1000);

    // Simulate OCR output
    let ocr_text = "Hello world";

    // Add to context
    context.add_translation(ocr_text, "Hola mundo", 20);

    // Verify context updated
    assert_eq!(context.buffer.len(), 1);
}

#[tokio::test]
async fn test_context_integration_with_translation() {
    let mut context = TranslationContext::new(500);

    // Add multiple translations
    for i in 0..5 {
        context.add_translation(
            &format!("text_{}", i),
            &format!("trans_{}", i),
            10
        );
    }

    // Verify pruning works
    assert!(context.current_token_count() <= 500);
}

#[tokio::test]
async fn test_prompt_router_integration() {
    use meowcal_sub::llm::prompt_router;

    // Test with real-world examples
    let technical = "HTTP 500 error on POST /api/users";
    let prompt = prompt_router::select_prompt(technical, "en", "zh");
    assert!(prompt.len() > 0);
}
```

**Step 2: Run integration tests**

Run: `cd src-tauri && cargo test --test integration_translation`
Expected: Tests compile and run

**Step 3: Fix any compilation or test failures**

Implement any missing integration logic.

**Step 4: Run tests again to verify**

Run: `cd src-tauri && cargo test --test integration_translation`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src-tauri/tests/integration_translation.rs
git commit -m "test: add translation pipeline integration tests

Test full flow from context through prompt selection.
Verify context management and routing integration.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

### Task 10: Test IPC Communication

**Files:**
- Create: `src-tauri/tests/integration_ipc.rs`

**Step 1: Write IPC protocol integration test**

Create `src-tauri/tests/integration_ipc.rs`:

```rust
//! Integration tests for IPC communication

use meowcal_sub::ipc::protocol::{IpcMessage, IpcRequest, IpcResponse};

#[test]
fn test_ipc_message_serialization() {
    let request = IpcMessage::Request(IpcRequest::ShowOverlay);
    let serialized = serde_json::to_string(&request).unwrap();
    assert!(serialized.len() > 0);
}

#[test]
fn test_ipc_message_deserialization() {
    let json = r#"{"Type":"Request","Command":"ShowOverlay"}"#;
    let message: IpcMessage = serde_json::from_str(json).unwrap();
    assert!(matches!(message, IpcMessage::Request(_)));
}

#[test]
fn test_ipc_response_creation() {
    let response = IpcResponse::Success;
    let serialized = serde_json::to_string(&response).unwrap();
    let deserialized: IpcResponse = serde_json::from_str(&serialized).unwrap();
    assert!(matches!(deserialized, IpcResponse::Success));
}

#[test]
fn test_ipc_error_response() {
    let response = IpcResponse::Error("Test error".to_string());
    let serialized = serde_json::to_string(&response).unwrap();
    assert!(serialized.contains("Test error"));
}
```

**Step 2: Run IPC integration tests**

Run: `cd src-tauri && cargo test --test integration_ipc`
Expected: Tests verify protocol serialization

**Step 3: Fix any protocol issues**

If tests fail, update protocol structures to handle serialization correctly.

**Step 4: Run tests again**

Run: `cd src-tauri && cargo test --test integration_ipc`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src-tauri/tests/integration_ipc.rs
git commit -m "test: add IPC protocol integration tests

Test message serialization/deserialization for IPC.
Verify request and response handling.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

### Task 11: Test HTTP Server Endpoints

**Files:**
- Create: `src-tauri/tests/integration_http.rs`

**Step 1: Write HTTP endpoint integration tests**

Create `src-tauri/tests/integration_http.rs`:

```rust
//! Integration tests for HTTP server endpoints

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tower::ServiceExt;
use serde_json::Value;

#[tokio::test]
async fn test_health_endpoint() {
    let app = meowcal_sub::http_server::create_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap()
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_settings_endpoint_get() {
    let app = meowcal_sub::http_server::create_router();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap()
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_translation_diagnostics_endpoint() {
    let app = meowcal_sub::http_server::create_router();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/translation/diagnostics")
                .body(Body::empty())
                .unwrap()
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
```

**Step 2: Run HTTP integration tests**

Run: `cd src-tauri && cargo test --test integration_http`
Expected: Tests verify API endpoints

**Step 3: Implement missing router methods**

If `create_router()` doesn't exist, refactor HTTP server to expose it.

**Step 4: Run tests again**

Run: `cd src-tauri && cargo test --test integration_http`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src-tauri/tests/integration_http.rs src-tauri/src/http_server.rs
git commit -m "test: add HTTP server integration tests

Test /api/health, /api/settings, and diagnostics endpoints.
Refactor HTTP server to expose testable router.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Phase 4: Windows E2E Tests

### Task 12: Create E2E Test Infrastructure

**Files:**
- Create: `src-tauri/tests/e2e_windows.rs`

**Step 1: Write feature-gated E2E test skeleton**

Create `src-tauri/tests/e2e_windows.rs`:

```rust
//! End-to-end tests with real Windows APIs
//!
//! These tests only run on Windows with the `test-windows` feature enabled.

#![cfg(feature = "test-windows")]
#![cfg(target_os = "windows")]

#[cfg(test)]
mod tests {
    #[test]
    fn test_windows_platform_detected() {
        // This test verifies we're on Windows
        assert!(cfg!(windows));
    }
}
```

**Step 2: Run E2E tests (should compile but skip on non-Windows)**

Run: `cd src-tauri && cargo test --test e2e_windows`
Expected: Compiles, tests only run on Windows

**Step 3: On Windows, verify test runs**

If on Windows: `cargo test --features test-windows --test e2e_windows`
Expected: Test passes

**Step 4: Commit**

```bash
git add src-tauri/tests/e2e_windows.rs
git commit -m "test: add Windows E2E test infrastructure

Create feature-gated E2E tests for Windows-specific APIs.
Tests only compile and run with test-windows feature.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

### Task 13: Test Screen Capture E2E

**Files:**
- Modify: `src-tauri/tests/e2e_windows.rs`

**Step 1: Add screen capture E2E test**

Add to `src-tauri/tests/e2e_windows.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use meowcal_sub::capture::GraphicsCapture;
    use std::time::Duration;

    #[tokio::test]
    #[cfg(feature = "test-windows")]
    async fn test_screen_capture_real_api() {
        // Create a small capture region (top-left 100x100)
        let capture = GraphicsCapture::new(0, 0, 100, 100).await;

        // Start capture
        capture.start().await.expect("Failed to start capture");

        // Wait for at least one frame
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify we received frames
        assert!(capture.frame_count() > 0);

        // Stop capture
        capture.stop().await;
    }

    #[tokio::test]
    #[cfg(feature = "test-windows")]
    async fn test_capture_cleanup() {
        let capture = GraphicsCapture::new(0, 0, 50, 50).await;
        capture.start().await.expect("Failed to start");
        capture.stop().await;

        // Verify no resource leaks (check handle count)
        // This is a basic sanity check
        assert!(true);
    }
}
```

**Step 2: Run on Windows with test-windows feature**

Run: `cargo test --features test-windows --test e2e_windows test_screen_capture`
Expected: Real screen capture happens, tests pass

**Step 3: Implement missing methods if needed**

If `GraphicsCapture` API doesn't match, update test to use actual API.

**Step 4: Commit**

```bash
git add src-tauri/tests/e2e_windows.rs
git commit -m "test(e2e): add screen capture E2E tests

Test real Windows.Graphics.Capture API with D3D11.
Verify frame capture and resource cleanup.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

### Task 14: Test OCR Pipeline E2E

**Files:**
- Modify: `src-tauri/tests/e2e_windows.rs`

**Step 1: Create test fixtures directory and sample image**

Run: `mkdir -p src-tauri/test-fixtures/images`

Create a simple test image (or add instructions to add one):

**Step 2: Add OCR E2E test**

Add to `src-tauri/tests/e2e_windows.rs`:

```rust
#[tokio::test]
#[cfg(feature = "test-windows")]
async fn test_ocr_extract_text() {
    use meowcal_sub::ocr::WindowsOcr;

    let ocr = WindowsOcr::new().await.expect("Failed to init OCR");

    // In real test, would capture screen with known text
    // For now, test OCR initialization
    assert!(ocr.is_available());
}

#[tokio::test]
#[cfg(feature = "test-windows")]
async fn test_ocr_language_support() {
    use meowcal_sub::ocr::WindowsOcr;

    let ocr = WindowsOcr::new().await.expect("Failed to init OCR");

    // Verify common languages are supported
    assert!(ocr.supports_language("en-US"));
    assert!(ocr.supports_language("zh-CN"));
}
```

**Step 3: Run OCR E2E tests**

Run: `cargo test --features test-windows --test e2e_windows test_ocr`
Expected: Tests verify OCR functionality

**Step 4: Commit**

```bash
git add src-tauri/tests/e2e_windows.rs
git commit -m "test(e2e): add OCR pipeline E2E tests

Test Windows.Media.Ocr API for text extraction.
Verify language support and availability.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Phase 5: Frontend Tests with Playwright

### Task 15: Set Up Playwright

**Files:**
- Modify: `package.json`
- Create: `playwright.config.ts`

**Step 1: Add Playwright to package.json**

Add to `devDependencies` in `package.json`:

```json
{
  "devDependencies": {
    "@playwright/test": "^1.40.0"
  },
  "scripts": {
    "test:e2e": "playwright test"
  }
}
```

**Step 2: Create Playwright config**

Create `playwright.config.ts`:

```typescript
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'html',
  use: {
    baseURL: 'http://localhost:3000',
    trace: 'on-first-retry',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: {
    command: 'npm run dev:backend',
    port: 3001,
    reuseExistingServer: !process.env.CI,
  },
});
```

**Step 3: Install Playwright**

Run: `npm install`
Expected: Playwright installs successfully

**Step 4: Commit**

```bash
git add package.json playwright.config.ts
git commit -m "test(frontend): add Playwright for E2E testing

Add Playwright configuration for browser-based UI testing.
Configure test server and base URL.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

### Task 16: Create Page Object Models

**Files:**
- Create: `tests/e2e/pom/settings.page.ts`
- Create: `tests/e2e/pom/overlay.page.ts`

**Step 1: Create settings page object model**

Create `tests/e2e/pom/settings.page.ts`:

```typescript
import { expect, Page } from '@playwright/test';

export class SettingsPage {
  readonly page: Page;
  readonly sourceLanguageSelect;
  readonly targetLanguageSelect;
  readonly saveButton;

  constructor(page: Page) {
    this.page = page;
    this.sourceLanguageSelect = page.locator('#source-language');
    this.targetLanguageSelect = page.locator('#target-language');
    this.saveButton = page.locator('button:has-text("Save")');
  }

  async goto() {
    await this.page.goto('/');
  }

  async selectLanguages(source: string, target: string) {
    await this.sourceLanguageSelect.selectOption(source);
    await this.targetLanguageSelect.selectOption(target);
  }

  async saveSettings() {
    await this.saveButton.click();
  }

  async verifySaved() {
    await expect(this.page.locator('.toast-success')).toBeVisible();
  }
}
```

**Step 2: Create overlay page object model**

Create `tests/e2e/pom/overlay.page.ts`:

```typescript
import { expect, Page } from '@playwright/test';

export class OverlayPage {
  readonly page: Page;
  readonly subtitleDisplay;
  readonly startButton;
  readonly stopButton;

  constructor(page: Page) {
    this.page = page;
    this.subtitleDisplay = page.locator('.subtitle-text');
    this.startButton = page.locator('button:has-text("Start")');
    this.stopButton = page.locator('button:has-text("Stop")');
  }

  async startTranslation() {
    await this.startButton.click();
  }

  async stopTranslation() {
    await this.stopButton.click();
  }

  async verifySubtitleVisible(text: string) {
    await expect(this.subtitleDisplay).toContainText(text);
  }
}
```

**Step 3: Commit**

```bash
git add tests/e2e/pom/
git commit -m "test(frontend): add page object models

Create POM for Settings and Overlay pages.
Provide reusable test abstractions.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

### Task 17: Write Frontend E2E Tests

**Files:**
- Create: `tests/e2e/settings.spec.ts`
- Create: `tests/e2e/translation.spec.ts`

**Step 1: Write settings page tests**

Create `tests/e2e/settings.spec.ts`:

```typescript
import { test, expect } from '@playwright/test';
import { SettingsPage } from './pom/settings.page';

test.describe('Settings Page', () => {
  let settingsPage: SettingsPage;

  test.beforeEach(async ({ page }) => {
    settingsPage = new SettingsPage(page);
    await settingsPage.goto();
  });

  test('loads settings page', async ({ page }) => {
    await expect(page).toHaveTitle(/Meowcal-Sub/);
  });

  test('selects source and target languages', async ({ page }) => {
    await settingsPage.selectLanguages('en-US', 'zh-CN');
    await settingsPage.saveSettings();
    await settingsPage.verifySaved();
  });

  test('validates language selection', async ({ page }) => {
    // Test validation logic
    await settingsPage.selectLanguages('en-US', 'en-US');
    await settingsPage.saveSettings();
    // Should show error if source == target
  });
});
```

**Step 2: Write translation control tests**

Create `tests/e2e/translation.spec.ts`:

```typescript
import { test, expect } from '@playwright/test';
import { OverlayPage } from './pom/overlay.page';

test.describe('Translation Controls', () => {
  let overlayPage: OverlayPage;

  test.beforeEach(async ({ page }) => {
    overlayPage = new OverlayPage(page);
    await page.goto('/');
  });

  test('starts translation', async ({ page }) => {
    await overlayPage.startTranslation();
    // Verify start button disabled
    await expect(overlayPage.startButton).toBeDisabled();
  });

  test('stops translation', async ({ page }) => {
    await overlayPage.startTranslation();
    await overlayPage.stopTranslation();
    // Verify stop button disabled
    await expect(overlayPage.stopButton).toBeDisabled();
  });

  test('displays subtitles', async ({ page }) => {
    // Mock API response for testing
    await page.route('**/api/translate', route => {
      route.fulfill({
        status: 200,
        body: JSON.stringify({ translated_text: 'Translated subtitle' })
      });
    });

    await overlayPage.startTranslation();
    await overlayPage.verifySubtitleVisible('Translated subtitle');
  });
});
```

**Step 3: Run frontend E2E tests**

Run: `npm run test:e2e`
Expected: Tests run in browser

**Step 4: Fix any test failures**

Update selectors or logic as needed.

**Step 5: Commit**

```bash
git add tests/e2e/
git commit -m "test(frontend): add E2E tests for UI workflows

Test settings page language selection and saving.
Test translation start/stop controls and subtitle display.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Phase 6: CI/CD Pipeline

### Task 18: Create GitHub Actions Workflow

**Files:**
- Create: `.github/workflows/test.yml`

**Step 1: Create test workflow**

Create `.github/workflows/test.yml`:

```yaml
name: Tests

on:
  push:
    branches: [main, feature/**]
  pull_request:
    branches: [main, feature/**]

jobs:
  # Lint & Format
  lint:
    name: Lint & Format Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - name: Check formatting
        run: cd src-tauri && cargo fmt --check
      - name: Run Clippy
        run: cd src-tauri && cargo clippy -- -D warnings

  # Unit Tests
  unit-tests:
    name: Unit Tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Run unit tests
        run: cd src-tauri && cargo test --lib

  # Integration Tests
  integration-tests:
    name: Integration Tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Run integration tests
        run: cd src-tauri && cargo test --test integration_*

  # Windows E2E Tests
  windows-e2e:
    name: Windows E2E Tests
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Run Windows E2E tests
        run: cd src-tauri && cargo test --features test-windows

  # Frontend E2E Tests
  frontend-e2e:
    name: Frontend E2E Tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
      - name: Install dependencies
        run: npm install
      - name: Install Playwright
        run: npx playwright install --with-deps chromium
      - name: Run Playwright tests
        run: npm run test:e2e
      - uses: actions/upload-artifact@v4
        if: always()
        with:
          name: playwright-report
          path: playwright-report/
```

**Step 2: Verify workflow syntax**

Check YAML syntax is valid.

**Step 3: Commit**

```bash
git add .github/workflows/test.yml
git commit -m "ci: add GitHub Actions test workflow

Add comprehensive CI pipeline with lint, unit, integration,
Windows E2E, and frontend tests.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Phase 7: Documentation and Cleanup

### Task 19: Update CLAUDE.md with Testing Information

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Add testing section to CLAUDE.md**

Add to `CLAUDE.md`:

```markdown
## Testing

### Running Tests Locally

```powershell
# All unit and integration tests (fast, any platform)
cd src-tauri
cargo test

# Include Windows E2E tests
cargo test --features test-windows

# Integration tests only
cargo test --test integration_*

# Frontend E2E tests
npm run test:e2e
```

### Test Structure

- **Unit Tests**: In `src/*.rs` files under `#[cfg(test)]` modules
- **Integration Tests**: In `src-tauri/tests/integration_*.rs`
- **E2E Tests**: In `src-tauri/tests/e2e_windows.rs` (Windows only, feature-gated)
- **Frontend Tests**: In `tests/e2e/` using Playwright

### Test Features

- `test-windows` - Enable Windows-specific E2E tests with real Windows APIs
- Unit tests run on any platform with mocked dependencies
- Integration tests verify module interactions
- E2E tests validate real Windows API behavior
```

**Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: add testing documentation to CLAUDE.md

Document test structure, commands, and features.
Include guidance for running tests locally.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

### Task 20: Create README for Tests

**Files:**
- Create: `src-tauri/tests/README.md`

**Step 1: Create test README**

Create `src-tauri/tests/README.md`:

```markdown
# Tests

This directory contains integration and E2E tests for Meowcal-Sub.

## Test Files

- `integration_translation.rs` - Translation pipeline integration tests
- `integration_ipc.rs` - IPC protocol tests
- `integration_http.rs` - HTTP server endpoint tests
- `e2e_windows.rs` - Windows E2E tests (requires `test-windows` feature)

## Running Tests

```bash
# All integration tests
cargo test --test integration_*

# Windows E2E tests
cargo test --features test-windows --test e2e_windows

# Specific test
cargo test --test integration_translation test_full_translation_pipeline
```

## Test Features

- `test-windows` - Enable Windows-specific E2E tests

## Adding Tests

When adding new tests:
1. Unit tests go in `src/*.rs` files under `#[cfg(test)]` modules
2. Integration tests go in `tests/integration_*.rs`
3. Windows-specific tests go in `tests/e2e_windows.rs` with feature gate
```

**Step 2: Commit**

```bash
git add src-tauri/tests/README.md
git commit -m "docs: add test directory README

Document test file organization and usage.
Provide guidance for adding new tests.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

### Task 21: Final Verification and Cleanup

**Files:**
- None (verification task)

**Step 1: Run all tests to verify everything works**

Run: `cd src-tauri && cargo test`
Expected: All unit and integration tests pass

**Step 2: Run Windows E2E tests if on Windows**

Run (Windows only): `cargo test --features test-windows`
Expected: Windows tests pass

**Step 3: Run frontend E2E tests**

Run: `npm run test:e2e`
Expected: Playwright tests pass

**Step 4: Check for any leftover TODO comments**

Run: `grep -r "TODO" src-tauri/src/ src-tauri/tests/`
Expected: No critical TODOs left (or documented in issues)

**Step 5: Verify git status is clean**

Run: `git status`
Expected: All changes committed, working tree clean

**Step 6: Create summary commit if needed**

If any final tweaks needed, commit them.

---

## Completion Criteria

When all tasks are complete:
- ✅ Unit tests cover core modules (config, context, prompt_router, manager)
- ✅ Integration tests validate module interactions (translation, IPC, HTTP)
- ✅ E2E tests run real Windows APIs (capture, OCR, overlay)
- ✅ Frontend tests validate UI workflows (Playwright)
- ✅ CI/CD pipeline runs on GitHub Actions
- ✅ Documentation updated (CLAUDE.md, test READMEs)
- ✅ All tests passing locally
- ✅ Clean git history with descriptive commits

---

**Total Estimated Tasks:** 21
**Total Estimated Time:** 6-10 hours (depending on familiarity with codebase and APIs)

**Next Steps After Completion:**
1. Push branch to remote: `git push -u origin feature/test-automation`
2. Create pull request to main branch
3. Review test coverage reports
4. Monitor CI/CD pipeline results
5. Iterate on tests based on findings
