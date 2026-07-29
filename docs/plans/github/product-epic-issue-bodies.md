# GitHub Drafts: Product Epic and Child Issues

**Status:** Exact draft; do not post without approval
**Repository:** `PeterShanxin/Meowcal-Sub`

Placeholder tokens such as `[PRODUCT_EPIC_NUMBER]` and `[A2_NUMBER]` are replaced with assigned GitHub issue numbers after each creation step.

---

## Product epic

### Title

`Epic: Make HY-MT a built-in local subtitle engine`

### Labels

`epic`, `type:feature`, `area:engine`, `priority:p0`

### Body

```markdown
## Problem

Meowcal Sub has a working Windows capture/OCR/overlay foundation and a proven local Tencent HY-MT prototype, but the product still exposes backend infrastructure and does not provide one reliable translation-engine lifecycle.

Current user-visible failures include:

- correct Chinese-to-English translations can be rejected as overlong;
- deterministic validation failures are retried three times;
- source OCR can be presented as if it were translated output;
- Windows reports equivalent Chinese OCR language tags that the UI marks as missing;
- setup exposes Foundry/model concepts instead of one supported engine;
- startup can show the main window centered and then move it to the saved position.

Make Meowcal Sub a private, app-managed Windows subtitle translator for watching Chinese or Japanese TV series in English. A normal user must not need to understand Foundry, llama.cpp, model files, endpoints, ports, or command lines.

## Product decisions

- Keep Tauri 2, Rust, and Windows OCR.
- Tencent HY-MT is the only supported normal-mode translation engine.
- The app owns runtime/model selection, download, verification, install, repair, rollback, health, warm-up, process lifecycle, and sample translation.
- Arbitrary endpoints and models move to disabled-by-default developer mode.
- The old Python/OpenSubtitles MeoCoSub2 rewrite is outside this epic.
- Deliver through bounded reviewable changes, not a wholesale rewrite.

## Child issues

- [ ] #15 — Normalize Windows OCR language aliases
- [ ] #[A2_NUMBER] — Make subtitle validation language-aware
- [ ] #[A3_NUMBER] — Separate retry classes and fallback states
- [ ] #[A4_NUMBER] — Establish a curated subtitle translation evaluation set
- [ ] #[A5_NUMBER] — Define the versioned HY-MT engine manifest
- [ ] #[A6_NUMBER] — Implement engine install, adoption, verification, repair, and rollback
- [ ] #[A7_NUMBER] — Own the HY-MT runtime process lifecycle
- [ ] #[A8_NUMBER] — Replace backend setup with one translation-engine flow
- [ ] #[A9_NUMBER] — Restore the main window before first visibility
- [ ] #[A10_NUMBER] — Instrument and optimize the live subtitle pipeline
- [ ] #[A11_NUMBER] — Close the curated local translation release gate

## Dependency order

```text
#15 OCR aliases ─────────────────────┐
A2 output validation ───────────────┼─> A4 evaluation set
A3 retry/fallback states ───────────┘        |
                                             v
A5 engine manifest -> A6 install/repair -> A7 runtime ownership -> A8 setup UX
                                                                      |
A9 window lifecycle ---------------------------------------------------┼-> A11 release closure
A10 pipeline measurement ----------------------------------------------┤
A4 evaluation set -----------------------------------------------------┘
```

## Delivery rules

- One focused PR per child issue unless a child explicitly documents a smaller split.
- Visible behavior changes require fresh manual Windows validation.
- Correctness work lands before engine UX.
- The existing `feat/hymt-foundry` branch is implementation evidence; do not merge it wholesale.
- Do not claim performance improvement without before/after measurements.
- Do not close this epic before ARM64 and x64 evidence plus a real 30-minute episode regression exist.

## Definition of done

- One guided setup reaches a passing HY-MT sample translation.
- Returning launch requires no command line or model/backend knowledge.
- Correct Chinese/Japanese-to-English subtitle expansion is accepted.
- Deterministic validation rejection is not retried as a transient failure.
- Raw OCR is never presented as successful translation.
- The main window appears once at its final valid position.
- Exactly one app-owned runtime process exists during a session.
- Install, adoption, repair, rollback, restart, upgrade, and shutdown are verified.
- ARM64 and x64 performance results are recorded against the approved budgets.
- A real 30-minute TV episode regression passes after the final behavior change.

## Non-goals

- Cloud translation.
- A general local-model frontend.
- Python/OpenSubtitles rewrite.
- Automatic release/versioning.
- Silent production capture-text telemetry.

## Source documents

- `docs/plans/2026-07-29-curated-local-translation-app-spec.md`
- `docs/adr/0001-curated-local-translation-stack.md`
- `docs/plans/2026-07-29-wave-0-baseline.md`
```

---

## Existing Issue #15 update

### Action

Add one comment. Keep title, body, and open state unchanged.

### Comment

```markdown
## Redesign placement

This remains reproducible and is the first correctness item in the curated local translation redesign.

It will serve as **A1 — Normalize Windows OCR language aliases** under #[PRODUCT_EPIC_NUMBER].

Additional acceptance evidence before closure:

- one normalization owner is shared by dropdown rendering, missing-language warnings, installation completion, and OCR engine selection;
- `zh-CN`, `zh-Hans`, and `zh-Hans-CN` resolve to the intended Simplified Chinese OCR capability;
- `zh-TW`, `zh-Hant`, and `zh-Hant-TW` receive equivalent Traditional Chinese coverage;
- unit tests cover aliases and unrelated language tags;
- a live Windows check proves `AvailableRecognizerLanguages() -> zh-Hans-CN` is shown as installed for configured `zh-CN`;
- the visible warning is manually rechecked after the behavior change.

Do not close from unit tests alone; retain the live Windows gate.
```

---

## A2

### Title

`Make subtitle output validation language-aware`

### Labels

`type:bug`, `area:translation`, `priority:p0`, `gate:manual-validation`

### Milestone

`Wave 2 — Critical correctness`

### Body

```markdown
Parent: #[PRODUCT_EPIC_NUMBER]

## Problem

The current validator applies a universal character ceiling:

- `MAX_SUBTITLE_OUTPUT_RATIO = 4`
- `MIN_SHORT_SOURCE_OUTPUT_CHARS = 24`

For `先不提时钟塔`, HY-MT returns the valid 45-character translation `Let's not talk about the clock tower for now.` The app rejects it as `too_long`.

Session evidence recorded 148 `too_long` rejections and only 2 timeouts. The model endpoint is working; the application validator is wrong for natural Chinese-to-English expansion.

## Scope

- define subtitle-aware length checks by source/target writing-system characteristics;
- keep an absolute corruption guard without rejecting ordinary short-source expansion;
- retain empty, repetition-loop, prompt-echo, and wrong-language protection;
- return a typed/stable rejection reason;
- add realistic Chinese- and Japanese-to-English cases;
- document why each threshold exists.

## Non-goals

- Prompt redesign.
- Context-aware translation changes.
- Retry/fallback behavior; owned by #[A3_NUMBER].
- Broad model-quality scoring.

## Acceptance criteria

- `先不提时钟塔 -> Let's not talk about the clock tower for now.` passes.
- `谢谢 -> Thank you.` passes.
- representative short Japanese-to-English expansion passes.
- empty output, obvious repetition, prompt echo, and extreme corruption still fail.
- thresholds do not depend on byte length.
- tests name the source/target language pair and expected reason.
- no raw source text is added to production logs by default.

## Validation

- targeted validator unit tests;
- full Rust unit and integration suite;
- opt-in live HY-MT evaluation set;
- manual overlay check with at least one previously rejected short Chinese subtitle.
```

---

## A3

### Title

`Separate translation retry classes and fallback display states`

### Labels

`type:bug`, `area:pipeline`, `priority:p0`, `gate:manual-validation`

### Milestone

`Wave 2 — Critical correctness`

### Body

```markdown
Parent: #[PRODUCT_EPIC_NUMBER]

## Problem

Meowcal currently retries deterministic output rejection as if it were a transient runtime failure. After retries fail, `MockBackend` returns the source OCR string, which can look like a translated result.

This creates latency, repeated identical work, and misleading overlay output.

## Scope

- define retryable transport/runtime errors separately from non-retryable validation results;
- never repeat an identical request solely because output validation is deterministic;
- replace mock-backend success semantics with explicit pipeline/display states:
  - `translated`
  - `warming`
  - `temporarilyUnavailable`
  - `sourceOnly`
  - `stopped`
- ensure frontend and backend consume stable shared codes;
- preserve or clearly label the last valid translation during temporary failure;
- record counts/timing without production source text.

## Non-goals

- Changing validator thresholds; owned by #[A2_NUMBER].
- Engine process recovery; owned by #[A7_NUMBER].
- Visual redesign beyond the minimum explicit state treatment.

## Acceptance criteria

- deterministic validation rejection triggers zero identical retries;
- transient connection reset/timeout follows one documented bounded policy;
- raw OCR is never labeled or styled as successful translation;
- duplicate OCR cooldown does not suppress recovery forever;
- stale late results cannot overwrite a newer subtitle/session;
- tests cover every display state and retry class.

## Validation

- manager/pipeline unit tests;
- frontend payload/presentation tests;
- simulated timeout, connection reset, deterministic rejection, and recovery;
- manual overlay check for temporary engine unavailability.
```

---

## A4

### Title

`Establish a curated subtitle translation evaluation set`

### Labels

`type:maintenance`, `area:translation`, `priority:p1`

### Milestone

`Wave 2 — Critical correctness`

### Body

```markdown
Parent: #[PRODUCT_EPIC_NUMBER]

## Problem

Translation changes are tested with isolated synthetic strings. The repository lacks one versioned set representing real subtitle and OCR shapes, so model, prompt, normalization, and validation regressions are difficult to compare.

## Scope

Create a local, privacy-safe evaluation dataset and runner covering:

- short and long Simplified Chinese;
- short Japanese;
- spaced CJK OCR;
- mixed Chinese/Japanese subtitle regions;
- multiline text;
- names, honorifics, idioms, punctuation, and recurring terms;
- OCR noise and desktop-text contamination;
- expected output shape and acceptable validation result;
- per-case latency and rejection reason.

Use authored or explicitly approved samples. Do not commit private production screen text.

## Non-goals

- Claiming a single exact translation is the only correct wording.
- Automatic semantic grading by another cloud model.
- Production telemetry.

## Acceptance criteria

- dataset format is documented and versioned;
- runner separates deterministic unit fixtures from opt-in live-engine cases;
- expected invariants allow reasonable wording variation;
- results include output shape, validator decision, stable reason, and latency;
- the previously rejected `先不提时钟塔` case is included;
- CI runs deterministic fixtures;
- live-engine results are captured as explicit release evidence.
```

---

## A5

### Title

`Define the versioned HY-MT engine manifest`

### Labels

`type:feature`, `area:engine`, `priority:p0`

### Milestone

`Wave 3 — Curated engine`

### Body

```markdown
Parent: #[PRODUCT_EPIC_NUMBER]

## Problem

The HY-MT prototype hardcodes model/runtime metadata in Rust. It has a model SHA-256 but no complete versioned compatibility, license, upgrade, or rollback contract.

## Scope

Define a manifest containing:

- manifest schema and engine version;
- HY-MT model/quantization identity;
- architecture-specific runtime assets;
- URLs, file sizes, and SHA-256 hashes for every installed artifact;
- Windows, architecture, RAM, disk, and capability requirements;
- launch arguments and supported features;
- compatibility and last-known-good rollback metadata;
- model/runtime license and distribution references;
- stable support/error codes.

Define whether the manifest ships with the app, can be refreshed, and how authenticity is established.

## Non-goals

- Download/install implementation.
- User-facing setup UI.
- Supporting arbitrary third-party models.

## Acceptance criteria

- one typed parser/validator owns manifest interpretation;
- unknown schema/architecture/runtime is rejected safely;
- every installed executable/model artifact has a cryptographic hash;
- rollback target cannot be overwritten before a new engine passes verification;
- ARM64 OpenCL and x64 Vulkan entries are represented;
- licensing/distribution review is explicit;
- tests cover valid, corrupt, unsupported, and downgrade/rollback cases.

## Decision required

Before implementation, document and review the manifest authenticity/update model. A plain unsigned remote manifest is not sufficient for executable replacement.
```

---

## A6

### Title

`Implement HY-MT install, adoption, verification, repair, and rollback`

### Labels

`type:feature`, `area:engine`, `priority:p0`, `gate:manual-validation`

### Milestone

`Wave 3 — Curated engine`

### Body

```markdown
Parent: #[PRODUCT_EPIC_NUMBER]
Depends on: #[A5_NUMBER]

## Problem

Users need one reliable app-owned setup path. The prototype can download and hash the model, but lacks a complete transactional install/adoption/repair/rollback lifecycle.

## Scope

- free-space and compatibility preflight;
- resumable or safely restartable downloads;
- runtime and model size/hash verification;
- adopt an existing app-managed installation only after manifest verification;
- install into versioned staging;
- promote only after verification and sample readiness;
- retain last-known-good engine;
- repair missing/corrupt files;
- roll back failed install/upgrade;
- expose stable progress/state to UI;
- mirror behavior in a support PowerShell script.

## Non-goals

- Runtime process lifecycle after promotion; owned by #[A7_NUMBER].
- Final UI flow; owned by #[A8_NUMBER].
- General package manager.

## Acceptance criteria

- interrupted download cannot replace the active engine;
- wrong size/hash never reaches `ready`;
- existing valid prototype artifacts can be adopted;
- corrupt active engine offers repair;
- failed upgrade restores the known-good version;
- disk paths avoid user-chosen arbitrary executables in normal mode;
- cancellation leaves a recoverable state;
- logs redact private screen text and contain support codes;
- clean install, adopt, corrupt, repair, failed-upgrade, and rollback tests pass.
```

---

## A7

### Title

`Own the HY-MT runtime process lifecycle`

### Labels

`type:feature`, `area:engine`, `priority:p0`, `gate:manual-validation`

### Milestone

`Wave 3 — Curated engine`

### Body

```markdown
Parent: #[PRODUCT_EPIC_NUMBER]
Depends on: #[A5_NUMBER], #[A6_NUMBER]

## Problem

The prototype starts `llama-server.exe` on fixed port 11436 and health-checks it, but process ownership is incomplete. Multiple callers can start the runtime, fixed ports can collide, and shutdown/repair must never kill unrelated processes.

## Scope

- one runtime owner and one state machine;
- reserve/select a validated loopback port;
- spawn and retain the exact child handle/identity;
- prevent duplicate app-owned processes;
- distinguish owned process, compatible adopted service, port collision, stale process, and foreign process;
- health, warm-up, sample test, restart, and bounded recovery;
- normal shutdown and crash cleanup;
- repair/update coordination;
- structured process/resource diagnostics.

## Non-goals

- Killing processes by image name.
- Binding outside loopback.
- Supporting arbitrary server executables in normal mode.
- Setup UI.

## Acceptance criteria

- exactly one app-owned runtime exists during a session;
- foreign process on preferred port is not terminated;
- two simultaneous start requests converge on one owned process;
- shutdown targets only the owned child;
- stale/crashed runtime reaches a documented recoverable state;
- repair/update cannot replace files used by an unmanaged process;
- sleep/resume and app restart are tested;
- process and port tests are deterministic where mocked and manually verified on Windows.
```

---

## A8

### Title

`Replace backend setup with one local translation-engine flow`

### Labels

`type:feature`, `area:ui`, `priority:p0`, `gate:manual-validation`

### Milestone

`Wave 4 — Product UX`

### Body

```markdown
Parent: #[PRODUCT_EPIC_NUMBER]
Depends on: #[A6_NUMBER], #[A7_NUMBER]

## Problem

The current setup describes Foundry Local, models, service URLs, warm-up, and backend fallback. The approved product supports one app-managed translation engine.

## Scope

Normal mode:

- explain privacy, download size, expected setup time, and hardware support;
- show one engine lifecycle state;
- install, retry, repair, and test through one guided flow;
- show source/target language and OCR readiness;
- expose one clear Start/Stop session action;
- surface concise support codes.

Developer mode:

- disabled by default;
- clearly unsupported;
- contains endpoint/model overrides and verbose diagnostics;
- cannot silently change normal-mode support claims.

## Non-goals

- Advertising every diagnostic or internal state.
- Visual style rewrite unrelated to the flow.
- Generic model marketplace.

## Acceptance criteria

- normal setup contains no Foundry, llama.cpp, endpoint, port, or model-ID requirement;
- each engine state has one primary next action;
- first setup ends with a passing sample translation;
- returning user can start without reopening setup;
- repair resumes from the correct state;
- developer mode is opt-in and reversible;
- keyboard/focus and readable status/error behavior are manually checked;
- screenshots and a full first-run recording accompany the PR.
```

---

## A9

### Title

`Restore the main window before first visibility`

### Labels

`type:bug`, `area:lifecycle`, `priority:p1`, `gate:manual-validation`

### Milestone

`Wave 5 — Lifecycle and performance`

### Body

```markdown
Parent: #[PRODUCT_EPIC_NUMBER]

## Problem

The Tauri config creates the main window visible and centered. Rust setup later restores persisted bounds. Users can see the window jump to the center and then return to its saved position.

## Scope

- create the main window hidden;
- load and validate persisted bounds before first show;
- clamp to an attached monitor work area;
- apply DPI-aware size/position/maximized state;
- show exactly once after final bounds;
- debounce move/resize persistence;
- recover from missing monitor and corrupt/off-screen coordinates;
- instrument launch stages without storing private screen content.

## Non-goals

- Overlay geometry redesign.
- Tray behavior changes beyond required launch ordering.
- Startup performance claims without measurement.

## Acceptance criteria

- zero visible position corrections on returning launch;
- first launch uses one stable safe default;
- missing secondary monitor moves the window on-screen;
- single monitor, mixed-DPI, negative coordinates, unplugged monitor, maximized, and corrupt config cases are covered;
- unit tests cover bound selection/clamping;
- manual Windows recordings prove first and returning launch behavior.
```

---

## A10

### Title

`Instrument and optimize the live subtitle pipeline`

### Labels

`type:maintenance`, `area:pipeline`, `priority:p1`, `gate:manual-validation`

### Milestone

`Wave 5 — Lifecycle and performance`

### Body

```markdown
Parent: #[PRODUCT_EPIC_NUMBER]
Depends on: #[A2_NUMBER], #[A3_NUMBER], #[A7_NUMBER]

## Problem

Direct ARM64 HY-MT latency is promising, but the app lacks authoritative whole-pipeline timings and resource evidence. Optimization claims cannot be separated from OCR, capture, queue, retry, and overlay behavior.

## Baseline

On Snapdragon X Elite ARM64 with HY-MT1.5 1.8B Q4_K_M:

- warm direct endpoint p50: 487 ms;
- warm direct endpoint p95: 671 ms;
- ten representative subtitles returned normal English.

This is not capture-to-overlay evidence.

## Scope

- stage timings for capture, change detection, preprocessing, OCR, normalization, dedupe, queue, model, validation, and overlay;
- session/capture IDs for cancellation and stale-result suppression;
- frame/text dedupe and duplicate-call measurement;
- bounded queue/backpressure;
- CPU, memory, process-count, and cold/warm lifecycle evidence;
- ARM64 and x64 benchmark protocol;
- privacy-safe logs/support bundles.

## Acceptance criteria

- first interactive paint, returning launch, OCR p95, model p50/p95, capture-to-overlay p95, unchanged-frame CPU, duplicate-call rate, memory, and process count are measured;
- stale result cannot overwrite newer session/subtitle state;
- unchanged OCR produces zero duplicate model calls inside the dedupe window;
- optimization PRs include before/after data using the same protocol;
- production logs do not contain source/translated screen text by default;
- documented budget exceptions name owner and follow-up.
```

---

## A11

### Title

`Close the curated local translation release gate`

### Labels

`type:maintenance`, `area:release`, `priority:p0`, `gate:manual-validation`

### Milestone

`Wave 7 — Release closure`

### Body

```markdown
Parent: #[PRODUCT_EPIC_NUMBER]
Depends on: #15, #[A2_NUMBER], #[A3_NUMBER], #[A4_NUMBER], #[A6_NUMBER], #[A7_NUMBER], #[A8_NUMBER], #[A9_NUMBER], #[A10_NUMBER]

## Purpose

Prove the approved user promise on supported Windows hardware after the final behavior-changing work. This issue is evidence and release closure, not a place to hide unfinished product fixes.

## Automated gates

- clean-checkout unified verification;
- deterministic translation evaluation;
- install/repair/rollback integration tests;
- process ownership tests;
- config migration tests;
- frontend/browser smoke;
- documentation and maintainability checks.

## Manual Windows matrix

- clean install;
- existing-engine adoption;
- corrupt-file repair;
- failed-upgrade rollback;
- restart and exact process cleanup;
- OCR language alias behavior;
- single and mixed-DPI monitors;
- missing/unplugged monitor;
- pause, seek, scene change, window movement;
- sleep/resume;
- uninstall/upgrade;
- ARM64 and x64;
- real 30-minute TV episode.

## Acceptance criteria

- one guided setup reaches a passing sample translation;
- returning launch requires no command line;
- raw OCR is never mislabeled as translation;
- exactly one app-owned runtime process exists;
- main window appears once at its final valid position;
- all required performance measurements exist;
- support/repair script matches app behavior;
- code, UI, config, CI, docs, packaging, and GitHub status contain no known contradiction;
- remaining risks are explicit and accepted;
- parent epic closes only after this evidence is attached.
```
