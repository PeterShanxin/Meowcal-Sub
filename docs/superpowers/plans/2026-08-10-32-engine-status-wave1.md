# #32 Wave 1 Engine Status Implementation Plan

> **For agentic workers:** Execute task-by-task. Spec:
> `docs/superpowers/specs/2026-08-10-32-engine-status-wave1-design.md`

**Goal:** Extract engine status/readiness orchestration while preserving every
documented Tauri↔HTTP difference.

**Architecture:** `engine_status` owns orchestration and
`EngineStatusSnapshot`. Adapters keep Foundry-named wire DTOs and call
profile-specific entry points (`*_tauri` / `*_http`).

**Tech Stack:** Rust / Tauri 2 / Axum browser adapter

## Global Constraints

- Preserve matrix differences; never silently converge HTTP onto Tauri managed.
- No transport/manager/loop/GPU/#103-107 work.
- New production file ≤ 400 lines.
- No new private-text logging.
- Lower ratchets only when measured.

---

### Task 1: Domain types + managed snapshot + characterization tests

**Files:**
- Create: `src-tauri/src/engine_status.rs`
- Create: `src-tauri/src/engine_status_tests.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] Add `EngineStatusSnapshot`, `AdapterProfile`, managed builder
- [ ] Tests for managed phase/notes/probe=None and profile flags
- [ ] Register module

### Task 2: Legacy orchestration entry points

- [ ] `status_no_probe`, `refresh`, `prepare`, `make_ready` with profile flags
- [ ] Characterization tests for prepare notes + make-ready early-exit flags

### Task 3: Wire adapters

- [ ] `commands.rs` thin Tauri mapping + startup_gate
- [ ] `http_server.rs` thin HTTP mapping
- [ ] Remove duplicated bodies

### Task 4: Ratchet + verify

- [ ] Measure lines; update baseline
- [ ] `cargo fmt`, clippy, tests, `verify.ps1 -Stage All`
