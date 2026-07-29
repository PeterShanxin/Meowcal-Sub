# Meowcal Sub Curated Local Translation Redesign

**Date:** 2026-07-29
**Status:** Approved
**Product:** Meowcal Sub for Windows
**Primary use case:** Watch a Chinese or Japanese TV series with private, low-latency English subtitles

## 1. Product Decision

Meowcal Sub is a curated local subtitle translator, not a generic local-LLM frontend.

The app owns one supported translation stack:

- Tencent HY-MT as the translation model;
- an app-managed, hardware-compatible local runtime;
- Windows OCR for source-text recognition;
- a focused capture-to-subtitle pipeline;
- guided installation, repair, health checks, and sample translation.

Arbitrary OpenAI-compatible endpoints and model selection are not part of the normal product. If retained, they live behind an unsupported developer mode.

## 2. User Promise

A user should be able to install Meowcal Sub, complete one guided setup, select a subtitle region, and watch a series without understanding Foundry, llama.cpp, model formats, ports, or command lines.

All capture, OCR, and translation stay local.

## 3. Goals

1. Make HY-MT a first-class product component rather than a renamed generic backend.
2. Make initial setup and later repair reliable on supported Windows hardware.
3. Deliver readable subtitles at viewing speed without repeated model retries or raw diagnostic noise.
4. Preserve window and capture-region state without visible startup jumps.
5. Reduce startup work, capture-loop overhead, and translation latency.
6. Establish repository standards, quality gates, architectural boundaries, and measurable maintainability ratchets.
7. Migrate safely through small, reviewable waves instead of a rewrite.

## 4. Non-goals

- Supporting arbitrary local models in the main settings flow.
- Supporting cloud translation in the first redesign.
- Rewriting the application in Python.
- Adding OpenSubtitles acquisition or subtitle-file synchronization in this epic.
- Replacing Tauri, Rust, Windows OCR, or the current overlay solely for novelty.
- Mixing visible product changes with large behavior-preserving refactors.
- Automatic update, release, or telemetry services in the first delivery.

## 5. Supported Product Shape

### Normal mode

The normal UI exposes:

- source and target language;
- subtitle-region selection;
- translation-engine readiness;
- one Start/Stop control;
- overlay appearance;
- concise repair and diagnostics actions.

It does not expose:

- endpoint URLs;
- model identifiers;
- backend ordering;
- Foundry CLI concepts;
- raw timeout, token, or context-budget tuning.

### Developer mode

Developer mode may expose endpoint overrides, verbose logs, raw OCR, request/response inspection, and experimental runtimes. It must be visibly unsupported and disabled by default.

## 6. Primary User Flow

### First launch

1. App opens once at its final position; no center-then-restore movement.
2. App checks Windows OCR and the curated translation-engine manifest without blocking first paint.
3. Setup screen explains disk requirement, privacy, expected setup time, and supported hardware.
4. User selects source and target languages.
5. App installs or locates the required OCR language capability.
6. App downloads the approved runtime and HY-MT artifact.
7. App verifies hashes and free disk space, starts the runtime, performs a health probe, and runs a sample translation.
8. Setup finishes only when the sample translation passes semantic and output-shape checks.

### Returning launch

1. Window appears directly at the last valid on-screen position.
2. UI becomes interactive before model warm-up completes.
3. Translation runtime starts in the background only when needed or when an explicit warm-start preference is enabled.
4. Start becomes available when OCR, capture, and translation readiness are known.

### Watching

1. User selects or restores the subtitle capture region.
2. User starts translation.
3. App captures only at the configured cadence and skips unchanged frames/text.
4. OCR output is normalized and quality-filtered.
5. HY-MT returns one subtitle translation.
6. Output validation uses language-aware and subtitle-aware rules.
7. Overlay updates only for a new accepted subtitle.

### Failure and recovery

- Runtime missing or corrupt: show **Repair translation engine**.
- Runtime warming: keep current/last valid subtitle and show a subtle status.
- Translation timeout: retry only transient failures within a strict latency budget.
- Deterministic invalid output: do not repeat the same request three times.
- Translation unavailable: never present source OCR as if it were a successful translation. Label passthrough clearly or keep the last valid translation.
- OCR language alias mismatch: recognize equivalent Windows BCP-47 tags.

## 7. Curated Engine Lifecycle

### Versioned manifest

Ship or fetch a signed/versioned manifest containing:

- product engine version;
- model name and quantization;
- runtime variant and supported architecture;
- download URLs;
- expected file sizes;
- SHA-256 hashes;
- minimum RAM, disk, Windows version, and architecture;
- launch arguments and local port policy;
- compatibility and rollback metadata.

### Installation states

Use one state machine:

`notInstalled -> checking -> downloading -> verifying -> installing -> starting -> warming -> testing -> ready`

Terminal states:

`ready | repairRequired | unsupported | offline | failed`

Every state has one user action and one support code. UI and backend consume the same state source.

### Runtime ownership

- App owns process start, stop, restart, and stale-process detection.
- Bind to loopback only.
- Select an available port or reserve one safely.
- Prevent duplicate runtime processes.
- Shut down only the process instance owned by the app.
- Preserve a known-good engine during failed upgrades.

### Setup script

Provide a PowerShell support script for:

- unattended install/repair;
- hash verification;
- environment diagnostics;
- log bundle collection.

The script mirrors app behavior and is not the primary setup UX.

## 8. Translation Pipeline

### Pipeline stages

`capture -> frame change detection -> preprocess -> OCR -> text normalization -> dedupe -> translate -> validate -> overlay`

Each stage receives typed input/output and emits timing plus a stable result code.

### Translation behavior

- Temperature: deterministic.
- One focused translation prompt.
- Context off by default until measured to improve a named subtitle dataset without violating latency.
- Context, when enabled, contains source-only recent lines and has a strict character/token budget.
- Output length checks account for source and target writing systems.
- Repetition, empty output, prompt echo, and wrong-language checks remain.
- Quality rejection classes are divided into retryable and non-retryable.

### Subtitle-specific acceptance

For Chinese-to-English, short natural expansions such as:

`先不提时钟塔 -> Let's not talk about the clock tower for now.`

must pass. Validation must not use a universal `source characters x 4` ceiling.

### Fallback contract

Fallback is a state, not another pretend translation backend.

The overlay payload distinguishes:

- `translated`;
- `warming`;
- `temporarilyUnavailable`;
- `sourceOnly`;
- `stopped`.

Raw OCR is never silently labeled as translation.

## 9. Performance Budgets

Measure on at least one supported ARM64 device and one x64 device.

| Measure | Target |
|---|---:|
| First interactive paint | <= 500 ms |
| Returning launch to usable settings | <= 1 s |
| Window position correction after first visibility | 0 visible corrections |
| Unchanged-frame path | <= 10% of one CPU core average |
| OCR + normalization, p95 | <= 250 ms |
| Warm HY-MT translation, p50 | <= 800 ms |
| Warm HY-MT translation, p95 | <= 1.8 s |
| Capture-to-overlay, warm p95 | <= 2.2 s |
| Duplicate OCR request rate | 0 duplicate model calls inside dedupe window |
| Runtime process count | exactly 1 app-owned process |

Budgets become benchmarks or structured session metrics before optimization claims are accepted.

## 10. Window and State Correctness

Current startup creates the main window as visible and centered, then restores saved size and position during Rust setup. This explains the visible center-then-return movement.

Required behavior:

- create the main window hidden;
- load and validate saved bounds before first show;
- clamp bounds to an attached monitor work area;
- apply DPI-aware size and position;
- show once after final bounds are set;
- debounce persistence during move/resize;
- retain a safe default when a monitor disappears;
- test single monitor, mixed-DPI monitors, unplugged monitor, maximized state, and corrupt coordinates.

## 11. Architecture Direction

Keep Tauri and Rust. Refactor by boundaries.

### Backend modules

- `app_lifecycle`: startup, shutdown, window restoration, tray behavior;
- `engine`: manifest, installer, verifier, runtime process, health state;
- `capture`: region ownership, frame acquisition, change detection;
- `ocr`: language capability, alias normalization, recognition;
- `translation`: HY-MT request, prompt, response parsing, validation;
- `pipeline`: session state machine and stage orchestration;
- `overlay`: display-state contract and window behavior;
- `diagnostics`: structured events, support bundle, privacy redaction;
- `config`: versioned schema and migrations.

### Frontend modules

- setup flow;
- session controls;
- engine status and repair;
- language and OCR readiness;
- capture-region controls;
- overlay settings;
- developer diagnostics.

No new production module should become another command/UI monolith.

### Boundary rules

- Tauri commands remain thin adapters.
- Business logic lives outside `commands.rs`.
- DOM handlers remain thin adapters.
- State machines have one owner.
- Config access goes through typed services and explicit migrations.
- Backend/UI status strings derive from shared stable codes.

## 12. Repository Foundation

Adapt the proven LexBridge #80 and #107 model to this repository.

### Normative documents

- `CONTRIBUTING.md`;
- `docs/CODING_STANDARDS.md`;
- `docs/AGENT_GUIDE.md`;
- `docs/ARCHITECTURE.md`;
- `docs/MAINTAINABILITY_BASELINE.md`;
- `docs/adr/README.md` plus lightweight ADR template.

Historical plans remain under `docs/plans/` and do not override current normative docs.

### Unified verification

Create one documented command that mirrors CI:

- Rust format;
- Rust clippy with warnings denied;
- Rust unit and integration tests;
- frontend formatting/lint;
- frontend unit tests;
- browser-mode smoke tests;
- documentation link/contract checks;
- maintainability ratchets.

Windows-native manual/E2E tests remain explicit release gates where automation cannot reproduce capture, OCR, overlay, DPI, or runtime behavior.

### Maintainability baseline

Initial measured hotspots:

| File | Current lines |
|---|---:|
| `src-tauri/src/commands.rs` | 2,899 |
| `src-tauri/src/llm/foundry_local.rs` | 1,790 |
| `src-tauri/src/llm/manager.rs` | 1,314 |
| `src/scripts/main.js` | 2,325 |
| `src/scripts/overlay.js` | 1,293 |
| `src/scripts/wizard.js` | 663 |

Set a reviewed file-size ceiling. Existing exceptions are tracked debt; touched code must not increase them. Ratchets only move downward.

### Collaboration foundation

- conventional commit contract;
- PR template covering intent, scope, validation, risk, manual gate, and issue linkage;
- structured bug, feature, and maintenance issue forms;
- minimal label taxonomy;
- explicit code ownership;
- required resolved review conversations;
- documented merge strategy and branch cleanup;
- one focused PR per bounded wave;
- before/after snapshots for GitHub setting changes.

Do not require impossible self-review while solo-maintained. Define activation conditions for stronger human-approval gates.

## 13. Testing Strategy

### Unit

- language-tag aliases;
- manifest selection and hash verification;
- install-state transitions;
- process ownership and duplicate prevention;
- prompt construction and output parsing;
- language-aware subtitle validation;
- retry classification;
- dedupe and stale-result suppression;
- window-bound clamping and DPI conversion;
- config migrations.

### Integration

- mocked runtime download through ready state;
- app restart with installed engine;
- corrupt artifact repair and rollback;
- OCR text through overlay payload;
- timeout and non-retryable rejection behavior;
- HTTP/browser bridge contract parity.

### Curated translation evaluation

Maintain a versioned local dataset of real or representative OCR lines:

- short and long Simplified Chinese;
- spaced Chinese OCR;
- mixed Chinese/Japanese subtitles;
- names, honorifics, idioms, punctuation, and multiline text;
- OCR noise and desktop-text contamination.

Record correctness, rejection reason, latency, and output shape. Do not log private screen text by default in production.

### Manual Windows gate

- clean install;
- OCR language installation/alias detection;
- engine download and repair;
- first and returning launch;
- single/mixed-DPI multi-monitor behavior;
- 30-minute real TV episode session;
- pause, seek, scene changes, window movement, sleep/resume;
- uninstall/upgrade and stale-process cleanup.

## 14. Migration

1. Preserve existing config through a versioned migration.
2. Detect existing managed HY-MT artifacts and adopt them after hash verification.
3. Map old Foundry settings to developer mode; do not expose them in normal UI.
4. Replace `MockBackend` terminology with explicit source-only fallback state.
5. Remove dead Offline MT and Phi Silica code after reference and migration review.
6. Update README and canonical architecture only when behavior lands.
7. Treat the older Python/OpenSubtitles “MeoCoSub2” plan as a separate archived concept unless explicitly revived.

## 15. Delivery Waves

### Wave 0 — Triage and baseline

- reconcile `main`, the unpushed HY-MT worktree, open Issue #15, and the old MeoCoSub2 plan;
- capture maintainability and performance baselines;
- document product decision and architecture ADR;
- no product behavior changes.

### Wave 1 — Repository foundation

- contributor/coding/agent guidance;
- unified local and CI verification;
- formatting/lint/test contracts;
- PR/issue templates and minimal labels;
- maintainability baseline and ratchets.

### Wave 2 — Critical correctness

- fix Chinese OCR language aliases;
- fix HY-MT output validation and retry classification;
- make passthrough state explicit;
- add real subtitle evaluation cases.

### Wave 3 — Curated engine lifecycle

- manifest, installer, verifier, runtime ownership, health state, repair, and sample test;
- migrate current managed HY-MT work rather than duplicate it.

### Wave 4 — First-run and session UX

- one guided setup;
- one readiness model;
- simplified settings;
- developer mode;
- failure and repair UX.

### Wave 5 — Lifecycle and performance

- no-jump startup;
- lazy/background initialization;
- capture/OCR/translation instrumentation;
- dedupe, cancellation, stale-result protection, and performance budgets.

### Wave 6 — Modular decomposition

After shared contracts land, decompose disjoint hotspots in isolated worktrees:

- Rust command/lifecycle lane;
- engine/translation lane;
- main/setup frontend lane;
- overlay/capture lane.

Assign file ownership before parallel work. Shared config and event contracts have one owner.

### Wave 7 — Release closure

- clean-checkout rehearsal;
- full automated verification;
- real-device install/repair test;
- 30-minute episode manual regression;
- docs and live GitHub settings consistency review;
- package and upgrade validation.

## 16. Definition of Done

- New user reaches a passing sample translation through one guided setup.
- Returning user starts a session without command-line steps.
- HY-MT is the only supported normal-mode translation engine.
- Correct Chinese-to-English subtitle expansions are accepted.
- Deterministic validation failures are not retried as transient failures.
- Source OCR is never mislabeled as a translation.
- Main window shows once at its final valid position.
- Performance budgets have measured results on ARM64 and x64.
- No duplicate app-owned runtime process exists.
- Unified verification matches CI and proves each ratchet can fail.
- Core monoliths are reduced below the reviewed ceiling or retain explicit, shrinking exceptions.
- Canonical docs, code, CI, repository settings, and UI contain no known contradiction.
- A full episode manual regression passes after the final behavior-changing wave.

## 17. Current Backlog Triage

### Issue #15

Keep open and prioritize in Wave 2. The bug is reproducible and the HY-MT branch still uses exact `installedSet.has(lang.value)` matching. Close only after alias tests and a live `zh-Hans-CN -> zh-CN` verification pass.

### HY-MT feature branch

No open PR exists. Branch `feat/hymt-foundry` contains two local commits and a large mixed patch. Treat it as implementation evidence. Rebase or selectively extract it only after this spec and the engine boundary are approved.

### Main branch

Local `main` is ahead of `origin/main` by one version/bundle commit. Do not create a redesign PR until this divergence is intentionally pushed, moved, or otherwise reconciled.

### Old MeoCoSub2 plan

The untracked approved plan proposes a Python/OpenSubtitles rewrite. It conflicts with this spec's Tauri/Rust curated real-time translator direction. Archive or supersede it explicitly; do not leave two “approved” product directions.

## 18. Approval Decisions

Approval of this spec confirms:

1. Tauri/Rust remains the product.
2. HY-MT becomes the sole supported normal-mode translation engine.
3. Generic endpoints/models move to unsupported developer mode.
4. OpenSubtitles and batch subtitle acquisition stay out of this epic.
5. Repository foundation lands before broad decomposition.
6. Product changes ship through sequenced waves and separate manual gates.
