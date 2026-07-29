# Wave 0 Baseline: Curated Local Translation Redesign

**Measured:** 2026-07-29
**Branch:** `docs/curated-redesign-wave0`
**Baseline commit:** `0d35889`
**Remote baseline:** `origin/main` at `7a41e93`
**Scope:** Read-only product, repository, runtime, and GitHub audit. No product behavior changes.

## Executive finding

Meowcal Sub has a working Windows capture/OCR/translation foundation and a promising local HY-MT prototype. It does not yet implement the approved product promise as one supported, app-managed translation engine.

Three conditions block direct feature delivery:

1. correctness bugs make valid HY-MT output fall back to source OCR;
2. the HY-MT prototype extends existing monoliths and lacks a complete engine lifecycle;
3. repository verification, documentation, and collaboration contracts do not cover the frontend or a clean local checkout.

## Repository state

| Item | Evidence | Classification |
|---|---|---|
| Local `main` | `0d35889`, one commit ahead of `origin/main` | Must reconcile before a remote PR |
| Remote `main` | `7a41e93` | Current GitHub default |
| Redesign worktree | `docs/curated-redesign-wave0` from local `main` | Isolated Wave 0 lane |
| HY-MT worktree | `feat/hymt-foundry` at `1f49771` | Clean local prototype; no PR |
| Old MeoCoSub2 plan | untracked `docs/plans/2026-03-04-meocosub2-design.md` in primary worktree | Conflicting historical concept |
| Approved redesign spec | untracked `docs/plans/2026-07-29-curated-local-translation-app-spec.md` in primary worktree | Approved source; must enter version control |

The local-only `0d35889` commit updates `package.json` and Tauri packaging to `0.5.0`. `src-tauri/Cargo.toml` remains `0.1.0`, so version sources already disagree.

## Current architecture

### Product path

`capture -> preprocess -> Windows OCR -> normalize/dedupe -> Foundry-style translator -> validate -> overlay`

### Primary hotspots

Measured from tracked files at `0d35889`:

| File | Lines | Main concerns |
|---|---:|---|
| `src-tauri/src/commands.rs` | 2,899 | IPC adapters, downloads, runtime setup, capture session, config, overlay control |
| `src/scripts/main.js` | 2,325 | setup UI, state, readiness, event handlers, settings persistence |
| `src-tauri/src/llm/foundry_local.rs` | 1,790 | CLI discovery, service lifecycle, API transport, prompt requests, summarization |
| `src-tauri/src/llm/manager.rs` | 1,314 | orchestration, retry, context tiers, validation, fallback, diagnostics |
| `src/scripts/overlay.js` | 1,293 | overlay state, input, positioning, resize, events, clipping |
| `src/scripts/selector.js` | 820 | selection state, input, resize, drag |
| `src-tauri/src/llm/context.rs` | 732 | history, memory, compression, prompt budgets |
| `src-tauri/src/http_server.rs` | 732 | browser-mode API adapters |
| `src-tauri/src/config.rs` | 682 | all persisted settings and defaults |
| `src/scripts/wizard.js` | 663 | legacy Foundry-centric installation flow |

Tracked repository summary:

- 232 files;
- 26 Rust files;
- 6 JavaScript files;
- 27 Markdown files;
- 72 Rust tests currently executed by the normal Rust test commands;
- zero JavaScript test calls on `main`.

### Boundary problems

- `commands.rs` and `main.js` combine adapters with business state machines.
- Foundry CLI/service concepts own the translation abstraction despite the approved HY-MT product decision.
- Fallback is modeled as `MockBackend`, returning source OCR as if it were a backend translation result.
- Config has no explicit schema version or migration owner.
- Backend and frontend duplicate readiness/presentation logic.
- Runtime process ownership, rollback, and stale-process cleanup do not have one module owner.

## Correctness baseline

### HY-MT output rejection

The model endpoint returns valid English:

`先不提时钟塔 -> Let's not talk about the clock tower for now.`

The manager rejects this 45-character translation because a six-character Chinese source receives a 24-character ceiling. Existing session evidence recorded:

- 148 `too_long` rejections;
- 2 timeouts;
- three attempts for deterministic validation rejection;
- source OCR returned through passthrough after rejection.

This is application validation failure, not model failure.

### OCR language availability

GitHub Issue #15 remains reproducible. Windows can report Simplified Chinese as `zh-Hans-CN`, while frontend availability uses exact matching:

`installedSet.has(lang.value)`

The same exact check remains on `feat/hymt-foundry`. Keep Issue #15 open until alias tests and live Windows verification pass.

### Startup window movement

`src-tauri/tauri.conf.json` creates the main window:

- centered;
- visible immediately.

`src-tauri/src/main.rs` later loads config and applies the persisted size and position during setup.

This ordering directly explains the reported center-then-return movement. No launch-timing instrumentation or window-restoration tests currently cover it.

## Documentation and product contradictions

| Source | Current statement | Live state / approved direction |
|---|---|---|
| `CLAUDE.md` | Offline MT and Phi Silica are active fallback backends | Their modules are absent from `src-tauri/src/llm` |
| `README.md` | Foundry Local or arbitrary compatible endpoint | HY-MT is approved as sole normal-mode engine |
| `README.md` | Offline MT setup and Phi Silica roadmap | UI removal already merged in PR #14 |
| `package.json` | NPU/Copilot+ description, version `0.5.0` | Current runtime prototype uses llama.cpp/OpenCL; x64 is also in scope |
| `src-tauri/Cargo.toml` | version `0.1.0` | JS and Tauri config use `0.5.0` |
| old MeoCoSub2 plan | Approved Python/OpenSubtitles rewrite | Explicit non-goal of approved redesign |

Classification:

- old MeoCoSub2 document: historical alternative, superseded by ADR-0001;
- README/CLAUDE corrections: land with normative docs in Wave 1;
- version ownership: decide and enforce one source in Wave 1;
- no historical plan should be deleted without an explicit archive decision.

## Verification baseline

### Existing CI

GitHub Actions runs on Windows:

- `cargo fmt --check`;
- `cargo clippy -- -D warnings`;
- `cargo test --lib`;
- `cargo test --test integration_ipc`.

Required checks on `main`:

- `Lint & Format`;
- `Tests`.

### Local clean-worktree result

First attempt failed before code checks:

`resource path resources\OverlayHost.exe doesn't exist`

CI creates ignored placeholder resources, but the documented local commands do not. After reproducing the CI prerequisite:

| Check | Result | Duration |
|---|---|---:|
| `cargo fmt --check` | pass | 0.3 s |
| `cargo clippy -- -D warnings` | pass | 10.3 s |
| `cargo test --lib` | 56 pass | 53.7 s |
| `cargo test --test integration_ipc` | 16 pass | 28.3 s |

Gaps:

- no unified root verification command;
- no frontend format or lint contract;
- no frontend tests on `main`;
- no browser-mode smoke test in CI;
- no documentation consistency check;
- no maintainability or coverage ratchet;
- no automated Windows capture/OCR/overlay E2E in normal CI;
- local clean-checkout instructions do not reproduce CI resource setup.

## HY-MT prototype reconciliation

Branch `feat/hymt-foundry` contains two commits:

- `301e1b6 feat: add managed HY-MT subtitle translation`
- `1f49771 fix: make managed HY-MT a first-class backend`

Patch size:

- 18 files;
- 1,383 insertions;
- 87 deletions;
- 618 changed lines concentrated in `commands.rs`.

Validation:

- 3 frontend presentation tests pass;
- 62 Rust unit tests pass;
- 1 live HY-MT test is ignored;
- 16 integration tests pass;
- format and clippy pass.

### Reuse candidates

- model ID, file size, URL, and SHA-256;
- ARM64 OpenCL and x64 Vulkan runtime asset selection;
- app cache layout;
- loopback-only runtime arguments;
- managed config persistence tests;
- download progress plumbing;
- sample translation command;
- UI presentation helper and its initial tests;
- startup-config readiness guard;
- live-test scaffold for installed HY-MT.

### Redesign before reuse

- move 592 added command lines into engine/install services;
- replace fixed port ownership with reserved/validated process state;
- add exact child-process ownership and normal shutdown;
- add manifest versioning, runtime hash verification, rollback, and repair state;
- separate Foundry CLI discovery from curated HY-MT runtime;
- remove duplicate process-start paths;
- fix language aliases, output validation, retry classes, and fallback state first;
- replace ignored live test with an explicit manual/opt-in gate and recorded evidence;
- migrate UI from backend presentation to one engine lifecycle state.

Conclusion: do not merge or rebase the branch wholesale. Extract reviewed units after shared contracts exist.

## Performance baseline

### Measured hardware

- Windows 11 build `10.0.26200`;
- ARM64;
- Snapdragon X Elite X1E80100;
- 12 logical processors;
- 31.6 GB RAM;
- HY-MT1.5 1.8B Q4_K_M;
- llama.cpp `b10155` OpenCL Adreno runtime.

### Warm direct-endpoint benchmark

Method:

- one temporary runtime process;
- loopback endpoint;
- deterministic prompt;
- ten representative Simplified Chinese subtitle lines;
- process stopped and port released after measurement.

| Metric | Result | Spec budget |
|---|---:|---:|
| Minimum | 447 ms | — |
| p50 | 487 ms | <= 800 ms |
| p95 | 671 ms | <= 1,800 ms |
| Maximum | 671 ms | — |

All ten responses were non-empty, single-result English translations. This proves only warm model endpoint latency on one ARM64 machine.

### Missing performance evidence

- cold startup-to-health;
- first interactive paint;
- returning launch;
- OCR and normalization p95;
- unchanged-frame CPU cost;
- capture-to-overlay p95;
- duplicate model-call rate;
- memory and GPU use;
- 30-minute stability;
- x64 runtime results.

No whole-app performance target is achieved until these are instrumented and measured.

## GitHub baseline

- open PRs: 0;
- open issues: Issue #15 only;
- merge commit, squash, and rebase are all enabled;
- automatic branch deletion is disabled;
- auto-merge is disabled;
- active ruleset requires `Lint & Format` and `Tests`;
- required checks are not strict with the latest `main`;
- repository-role bypass exists;
- no required review, resolved-conversation, ownership, issue-form, or PR-template contract is present.

No GitHub state was changed during Wave 0.

## Decisions carried forward

1. Tauri/Rust/Windows OCR stay.
2. HY-MT is the sole supported normal-mode translation engine.
3. Generic endpoints move to unsupported developer mode.
4. Python/OpenSubtitles remains outside this epic.
5. Repository foundation precedes broad decomposition.
6. Correctness precedes engine UX.
7. HY-MT prototype is mined, not merged wholesale.
8. Issue #15 stays open.
9. No remote PR starts until local `main` divergence and approved spec tracking are reconciled.

## Wave 0 exit status

Evidence baseline: complete.
Architecture decision: recorded in ADR-0001.
Backlog proposal: recorded for review.
External writes: none.
Product implementation: not started.
