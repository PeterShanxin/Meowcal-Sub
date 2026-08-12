# #32 Wave 4 — Context-tier progression boundary (TranslationPlanner)

**Date:** 2026-08-12
**Parent:** #36
**Issue:** #32 (partial)
**Starting main:** `46fcb75ca6e2969415d5a7a890fe4df3a18ec5df`
**Branch:** `refactor/32-translation-planner-wave4`
**Worktree:** `C:\FormerD\Repos\wt-meowcal-32-wave4`

## Goal

Extract the **context-tier progression state machine** — the
Full → MemoryOnly → None degradation loop that runs *above* one
`TranslationAttemptRunner` call — out of `TranslationManager` into one new
#32-owned owner, `llm/translation_planner.rs` / `TranslationPlanner`.

This is a **structural** wave. It does not change backend order, tier order,
degradation conditions, warning strings/order, retry policy values, attempt
caps, the shared budget clock, validation, diagnostics, context storage, or any
observable behavior.

`TranslationManager` keeps backend selection/availability/fallback progression,
context storage and prompt materialization, the tier store, diagnostics
snapshotting, display mapping, and the passthrough/source-only surface.

## Ownership map A — translation planning layer (current main `46fcb75`)

Measured on main: `manager.rs` 856, `translation_attempt.rs` 272,
`context.rs` 732.

| # | Cluster | Current owner | Desired owner (this wave) | Mutable state | Dependencies | Async | External behavior | Tests | Risk |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | Backend ordering (`ordered_backend_ids`) | `manager.rs` | `manager.rs` (stays) | none | none | sync | FoundryLocal → Mock | fallback tests | none |
| A2 | Backend enabled (`is_enabled`) | `manager.rs` | stays | none | config | sync | `disabled` warning + `record_error` | existing | none |
| A3 | Backend availability (`is_available`) | `manager.rs` | stays | none | backend | sync | `not_available` | existing | none |
| A4 | Backend ready-state (`ready_state`) | `manager.rs` | stays | none | backend | sync | `not_ready` → `Warming` | existing | none |
| A5 | Backend fallback progression (outer loop) | `manager.rs` `translate_with_context` | stays | warnings (local) | config, backends, diagnostics | awaits | fallback order, warning order | existing | none |
| A6 | Context-tier type + initial selection (`ContextTier`, `from_config`) | `manager.rs` | **NEW `translation_planner.rs`** | none | config | sync | initial tier per config | NEW | low |
| A7 | Context-tier degradation (timeout / slow-success) | `manager.rs` `try_foundry_with_context` | **NEW `translation_planner.rs`** | `context_tier` (written via handle; storage stays in manager) | runner, prompts, budget | async loop | degrade warnings, persisted effective tier | NEW + existing | low |
| A8 | Context prompt materialization (`get_context_prompt`, memory-prompt prebuild) | `manager.rs` (reads `context` storage + tier) | stays (storage owner; planner receives prebuilt prompts) | none | context.rs, tier | sync | prompts per tier | existing | none |
| A9 | Context mutation (`record_ocr_line`, memory, reset) | `manager.rs` | stays | context storage | context.rs | sync | dedupe/memory state | existing | none |
| A10 | Attempt-runner invocation | `manager.rs` | **NEW `translation_planner.rs`** | none | translation_attempt | async | per-tier attempt runs | NEW | low |
| A11 | Warnings accumulation | `manager.rs` | split: tier-loop warnings in planner, outer-loop warnings stay | shared `Vec` (caller-owned) | — | — | exact strings + order | NEW + existing | low (order pinned) |
| A12 | Display/result mapping (`fallback_display_state`, `Translated` construction) | `manager.rs` | stays | none | warnings | sync | display states | existing | none |
| A13 | Diagnostics outside the attempt runner | `manager.rs` | stays | diagnostics state | sync_utils | sync | `record_error` codes | existing | none |
| A14 | Total budget creation (`min(timeout, backend_budget())`) | `manager.rs` | stays | none | pipeline_deadline | sync | deadline cap | existing | none |
| A15 | Backend-specific policy construction (`AttemptPolicy`) | `manager.rs` | stays | none | constants | sync | max_attempts 3/1, 600ms, 2500ms caps | existing | none |
| A16 | Passthrough / source-only | `manager.rs` | stays | none | — | async (mock) | `SourceOnly` outcomes | existing | none |

## Ownership map B — capture/OCR/translation session pipeline (current main `46fcb75`)

Measured on main: `commands.rs` 1209 (`start_translation` loop ≈ 591–1013),
`pipeline_translation.rs` 343, `pipeline_session.rs` 162,
`pipeline_deadline.rs` 188, `pipeline_notices.rs` 200,
`pipeline_repeat_policy.rs` 92.

| # | Cluster | Current owner | Desired owner (this wave) | Mutable state | Async | Tests today | Behavior risk |
| --- | --- | --- | --- | --- | --- | --- | --- |
| B1 | Capture cadence (interval/pacer) | `commands.rs` loop + `pipeline_pacing::Pacer` | stays (unchanged this wave) | pacer | sleep | pacing tests | medium |
| B2 | Capture/session IDs | `pipeline_session::PipelineClock` | stays | atomics | sync | 7 tests | none |
| B3 | OCR execution | `commands.rs` loop (`RecognitionMode::recognize`) | stays | — | async | ocr tests | high |
| B4 | OCR normalization/filtering | `ocr/` (`BandFilter`, `text_cleanup`) | stays | band state | sync | band tests | none |
| B5 | OCR dedupe (recent lines) | `commands.rs` + `ocr_recent_lines` + context dedupe | stays | `recent_lines` | sync | recent_lines tests | medium |
| B6 | Subtitle/non-subtitle gating | `commands.rs` + `ocr_gate` + `band_filter` | stays | band_filter | sync | gate tests | medium |
| B7 | Translation request dispatch | `commands.rs` (`try_spawn`) + `pipeline_translation::Translator` | stays | `in_flight` | spawn | guard tests | low |
| B8 | In-flight request identity | `pipeline_session` (`translation_id`) | stays | atomic | sync | tests | none |
| B9 | Stale-result rejection | `pipeline_translation::Translator::run` | stays | — | async | — | none |
| B10 | Cancellation / stop | `commands.rs` (`stop_rx`) + `pipeline_deadline` | stays | stop channel | select | deadline tests | medium |
| B11 | Display-event emission | `commands.rs` (notices) + `pipeline_translation` (payload emit) | stays | `Notices` | async emit | notices tests | medium |
| B12 | Source-only display | `manager.rs` passthrough | stays | — | — | tests | none |
| B13 | Repeated-result suppression | `commands.rs` + `pipeline_repeat_policy` + `ocr_stability` | stays | `last_attempt_at` | sync | policy tests | medium |
| B14 | Region-change invalidation | `commands.rs` + `app_state::set_capture_region` | stays | atomics | sync | — | medium |
| B15 | Context summarization scheduler | `commands.rs` (spawned task) | stays | `compression_in_flight`, `context_generation` | spawn | none | high |
| B16 | Loop diagnostics/logging | `commands.rs` + `pipeline_translation` | stays | — | — | — | low |

The pipeline loop is already strongly decomposed around it
(`pipeline_*`, `ocr_*` helpers own cadence policy, identity, notices, dedupe
policy, gating). What remains in `commands.rs` is mostly glue with real I/O
(capture, OCR) that is not testable without the Tauri runtime — see candidate
comparison.

## Candidate comparison

| Criterion | A: full planner (backend + tier progression) | B: session loop extraction | C: **tier progression only (sub-cut of A)** | D: session sub-cut (e.g. summarization scheduler) | E: no extraction |
| --- | --- | --- | --- | --- | --- |
| State machines moved | 2 | 3+ | 1 | 1 partial | 0 |
| Seam quality | needs config + backend registry + context storage + tier + diagnostics = **every manager field** | `AppState` + stop channel + emitter + manager + clock + config values (8+ params) | planner = `{policy, diagnostics}`; per-run plan = grouped handle (5 params) | deep manager/backend coupling | n/a |
| Fake-abstraction test (§8) | **FAILS** — owns context storage and backend registry just to compile | n/a | **PASSES** — no storage, no registry, no duplicated tier state | n/a | n/a |
| Mutable-state coupling | `context_tier` shared + registry moved | atomics + app state interleave | `context_tier` single storage in manager; planner writes via handle (manager only reads it for prompt materialization) | `compression_in_flight` shared | n/a |
| Characterization coverage | partial | weak (loop glue untestable without Tauri) | strong: 10 planner-level + 2 manager-level deterministic tests | weak | none |
| Cancellation/stale risk | low | high | low (no stop/stale surfaces in scope) | medium | n/a |
| Visible-behavior risk | medium (display mapping leaks in) | high | low (byte-identical loop; matrix below) | medium | none |
| Public-contract exposure | none | none | none (`ContextTier` re-export preserved) | none | n/a |
| Hotspot reduction | manager 856 → ~550 | commands 1209 → ~800 (over new-file ceiling, would force a split) | manager 856 → ~711 | small | none |
| Usefulness for #32 closure | premature | premature | direct (residual item 1: context-tier planner ownership) | marginal | none |
| Usefulness for #31 | none | commands shrink only | none | marginal | none |
| Likelihood a later wave undoes it | HIGH (backend/tier would be unwound into two owners) | medium | LOW (planner is the natural home; backend progression later composes it) | medium | n/a |
| Backend-order/context/retry policy risk | medium | medium | none moved | medium | n/a |

**Selection: C.** A is rejected by the brief's own warning-sign test: a planner
that owns *both* loops needs every `TranslationManager` field, making it a
second manager rather than a boundary. B is rejected because the remaining
`commands.rs` loop is Tauri-coupled glue over already-extracted helpers (its
extraction would exceed the 400-line new-file ceiling, need `AppState` plus a
stop channel plus an emitter as parameters, and add almost no new
characterization — the rules worth pinning are already pinned in
`pipeline_notices`, `pipeline_repeat_policy`, `ocr_stability`, `ocr_gate`).
D is weaker than C on every axis (scheduler is entangled with
`FoundryLocalBackend::summarize_context`, real I/O). E wastes the Wave-3
seam (`AttemptPolicy`/`AttemptBudget`/`AttemptRequest`) that exists precisely
so tier planning can compose on top of it.

## Selected boundary

```text
TranslationManager (manager.rs)
  backend choice / availability / fallback ordering (outer loop)
  context storage + prompt materialization (prebuilds memory prompt)
  context_tier atomic storage (single storage owner)
  passthrough + display-state mapping
  diagnostics snapshotting
        |
        v  one call per FoundryLocal sequence
TranslationPlanner (llm/translation_planner.rs)   NEW
  ContextTier state machine (enum, degrade, select_context)
  context-tier progression: timeout degrade, slow-success degrade,
    effective-tier persistence (writes the manager's store via handle)
  attempt-runner invocation per tier
  typed sequence outcome (TieredOutcome) + tier-loop warnings
        |
        v  per tier iteration
TranslationAttemptRunner (llm/translation_attempt.rs)  [unchanged]
  one backend/tier attempt/retry state machine
```

Dependency direction: `manager` → `translation_planner` →
`translation_attempt` → {`TranslatorBackend`, `output_validation`,
`transport_errors`, `prompt_router`}. The planner holds no locks across
`.await`, touches no context storage, no backend registry, no config, no
diagnostics directly (all writes stay inside the runner it drives).

### New module contract

`src-tauri/src/llm/translation_planner.rs` (new production file, ≤ 400 lines):

```rust
/// A success slower than this (ms, shared budget clock) degrades the tier.
pub(super) const CONTEXT_SLOW_DEGRADE_MS: u128 = 1800;   // moved from manager.rs

pub enum ContextTier { None = 0, MemoryOnly = 1, Full = 2 }   // moved from manager.rs
// from_u8 / from_config / degraded / has_context / select_context — moved verbatim

pub(super) struct TranslationPlanner {
    policy: AttemptPolicy,
    diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    slow_degrade_ms: u128,          // const default; test knob
}

pub(super) struct TieredPlan<'a> {
    pub(super) text: &'a str,
    pub(super) source_language: &'a str,
    pub(super) target_language: &'a str,
    pub(super) full_context_prompt: Option<&'a str>,
    pub(super) memory_only_prompt: Option<&'a str>,   // prebuilt by manager
    pub(super) initial_tier: ContextTier,              // loaded by manager
    pub(super) tier_store: &'a AtomicU8,               // manager-owned storage
}

pub(super) struct TieredOutcome {
    pub(super) translated: String,
}

impl TranslationPlanner {
    pub(super) fn new(policy: AttemptPolicy, diagnostics: Arc<Mutex<TranslationDiagnosticsState>>) -> Self;
    pub(super) async fn run_tiered_sequence(
        &self,
        backend: &dyn TranslatorBackend,
        plan: &TieredPlan<'_>,
        ready_state: ReadyState,
        budget: &AttemptBudget,
        warnings: &mut Vec<String>,
    ) -> Option<TieredOutcome>;   // None = every tier exhausted → next backend
}
```

`TranslationManager::try_foundry_with_context` keeps its outer signature
(including its existing `#[allow(clippy::too_many_arguments)]`) but its body
becomes: build `AttemptBudget` (same `started` clock, before the memory-prompt
prebuild — same order as today), load `initial_tier`, prebuild the memory
prompt from context storage, call `run_tiered_sequence`, and map
`TieredOutcome` → `TranslationOutcome { backend_used, warnings: take(warnings),
display_state: Translated }`. Display mapping stays in the manager.

## Before/after behavior matrix (verified by tests)

| Dimension | BEFORE (main `46fcb75`) | AFTER (this wave) |
| --- | --- | --- |
| Backend order | FoundryLocal → Mock | same (outer loop unchanged) |
| Disabled backend | `foundry_local: disabled` warning + `record_error(disabled)` | same |
| Unavailable backend | `not_available` | same |
| Not-ready backend | `not_ready` → `Warming` display | same |
| Context-tier order | Full → MemoryOnly → None (`degraded()`) | same enum moved verbatim |
| Initial tier | `from_config(context_level, enabled)` | same call, same values |
| Tier degrade on attempt timeout | store degraded tier + `context_degraded` warning + rerun | same, same store timing (mid-loop, before next attempt) |
| Tier degrade on slow success | `latency_ms > 1800` → store + `context_degraded_slow` | same threshold, same store timing (at success) |
| Contexted success store | `context_used` → store current tier | same (`else if context_used` branch) |
| Uncontexted success store | no store (tier unverified) | same |
| `recovered_after_retry` warning | pushed before the slow-degrade warning | same order |
| Terminal validation rejection | `Failed(TranslationError(…))` → `foundry_local: {err}` warning → next backend, no retry | same |
| Budget exhausted at tier-loop entry | `TimedOut { total_exhausted: true }` → break → next backend | same |
| Uncontexted timeout retries | up to `max_attempts`, `"{id}: timeout"` once | same (runner unchanged) |
| Warnings sequence seen by `fallback_display_state` | shared `Vec`, exact order | same (same vec, same pushes) |
| Diagnostics writes | all inside the attempt runner + outer loop | same (`record_error`/`record_success` call sites unchanged) |
| `info!` "Translation backend used" | from `manager.rs` module path | same fields/level/string, target changes to the new module path (default `meowcal_sub=debug` filter is module-agnostic; Wave-2/3 precedent) |
| Shared budget clock | `started` created before memory-prompt prebuild; latency measured against it | same |
| Attempt policy values | 3/1 attempts, 600ms delay, 2500ms caps, prompt chars from config | same (constructed in manager, unchanged) |
| Total budget | `min(backend_timeout, backend_budget())` | same |
| Mock fallback / passthrough | `mock: fallback used` / `no translation backend available` | same |
| Display mapping | `fallback_display_state` + `Translated` | same (manager) |
| Context storage | `context.rs`, untouched | same |
| Context mutation timing | manager methods | same |
| `get_context_prompt` (capture loop) | reads `context_tier` + storage | same |
| Public surface | `llm::*` exports | same (`ContextTier` re-export preserved) |
| Line counts | manager 856 | manager 726; planner 260; planner tests 382 + 199; tier tests 130 (all ≤ 400) |

## Mutable-state ownership

- `context_tier` (AtomicU8): **single storage owner stays `TranslationManager`**.
  The planner writes it through `TieredPlan::tier_store` at exactly the same
  three points as today (timeout degrade, slow-success degrade, contexted
  success). The manager never writes it outside the sequence; it only reads it
  for `get_context_prompt` (capture loop) and the initial-tier load. No
  duplicated tier state, no second owner of the same machine.
- Warnings: caller-owned (`&mut Vec<String>`), as today.
- Attempt runner state: unchanged (runner is knob-driven).
- No new module holds persistent mutable state beyond the planner struct
  itself (immutable policy + diagnostics handle).

## Async / cancellation / stale audit

- No locks held across `.await` in moved code (the tier loop held none; the
  planner holds none).
- No new sleeps, timeouts, or spawns. The `timeout()` calls stay in the
  attempt runner; the shared `AttemptBudget` clock is preserved.
- Cancellation: dropping the translate future still drops the in-flight
  request (unchanged; planner adds no cancellation surface).
- Stale-result rejection remains in `pipeline_translation.rs` /
  `pipeline_session.rs` — untouched.
- No `unwrap`/`expect`/`panic!` on I/O/runtime paths in new code
  (planner has none; test assertions use `panic!` only on deterministic
  in-memory paths, per repository policy).

## Diagnostics ownership

`TranslationDiagnosticsState` (in `llm/mod.rs`) stays the single diagnostics
state owner. The planner writes none directly; the attempt runner it drives
writes exactly the same keys at the same points. Tier-loop and outer-loop
writes stay in the manager. `record_abandoned` (deadline path) untouched.

## Why this cut minimizes future rework

- It is the **context-tier half of residual item 1**; the backend-fallback
  half remains in `manager.rs` and can later compose this planner exactly as
  the manager does today — the call contract is stable and typed.
- It leaves context storage, prompt materialization, and the tier store with
  their current single owners (the brief's §9 conservatism rule), so no
  later wave must unwind storage moves.
- The planner mirrors the established Wave-3 shape (struct holding
  policy + diagnostics, typed request/outcome, no shared state), so a later
  backend-progression extraction follows the same pattern instead of inventing
  a new one.
- `ContextTier` becomes testable in isolation for the first time.

## Rejected alternatives

- **A (full planner)**: needs every `TranslationManager` field
  (config, backends, context storage, tier, diagnostics) — the brief's
  fake-abstraction warning signs; display mapping and policy constants would
  leak in or duplicate.
- **B (session loop)**: >400-line new file, `AppState`/stop/emitter
  parameter coupling, near-zero new characterization (its rules are already
  extracted and tested), highest behavior risk on the live path.
- **D (summarization scheduler)**: entangled with `FoundryLocalBackend` I/O;
  weaker seam than C.
- **E (no extraction)**: wastes the Wave-3 seam and leaves residual item 1
  untouched.
- **Moving `context_tier` storage into the planner**: rejected — the capture
  loop's `get_context_prompt` reads it through the manager; splitting storage
  would create the two-owner state machine the brief warns about.

## Non-goals (exact)

- No backend-fallback extraction; no capture/OCR loop move; no Foundry rename;
  no HTTP managed-runtime parity.
- No retry redesign, no context algorithm change, no validation threshold
  change, no `AttemptPolicy` value change.
- No duplicate/noisy-translation fix (observation documented below only).
- No #59 / #60 / #103 / #105 / #107 fixes; no GPU/runtime change.
- No change to `TranslationManager`'s public surface or any caller
  (`commands.rs`, `pipeline_translation.rs`, `http_server.rs`).
- No logging/string/payload changes.

## Characterization plan

Reuses `translation_attempt::test_fixtures::{ScriptedBackend, ScriptedStep,
default_policy, budget}` (deterministic, virtual time via `start_paused`)
plus two local fakes in the planner test files (a small `SlowBackend` with a
per-step delay for the slow-success path and a prompt-recording backend to pin
tier→prompt selection). No external network, no real Foundry dependency.

New test files (each ≤ 400 lines, new-file ceiling):

- `src-tauri/src/llm/translation_planner_tests.rs` (module tests + shared
  fakes, 382 lines):

1. first tier succeeds → `Some`, no degradation warnings, tier store = Full,
   1 call, `enable_context` option true
2. timeout degrades one tier and the next tier succeeds →
   `foundry_local: context_degraded` once (preceded by the runner's
   `foundry_local: timeout`), tier store = MemoryOnly, 2 calls
3. slow success (virtual/real delay > threshold) →
   `foundry_local: context_degraded_slow` once, tier store = MemoryOnly
4. contexted success stores the effective tier; uncontexted success does not
   touch the store (tier unverified rule: MemoryOnly with no memory prompt)
5. `recovered_after_retry` warning precedes `context_degraded_slow` (order
   pinned)
6. each tier receives its own prompt (Full → full prompt, MemoryOnly → memory
   prompt) via the prompt-recording backend
7. a success writes the diagnostics success key (error cleared, latency key
   present)

- `src-tauri/src/llm/translation_planner_exhaustion_tests.rs` (199 lines):

8. all tiers time out → `None` (next backend), `timeout`/`context_degraded`
   interleaved per tier, tier store = None, 5 calls (1 Full + 1 MemoryOnly + 3
   uncontexted)
9. terminal validation rejection → `foundry_local: Translation failed: …
   rejected as corrupted (overlong output). …` warning, `None`, 1 call,
   diagnostics last error `low_quality_output`
10. exhausted budget at entry → `None`, 0 calls, `foundry_local: timeout`

Manager-level additions in `src-tauri/src/llm/manager_tier_tests.rs`
(130 lines; shared fixtures stay in `manager_tests.rs`) — pass against
current code, must keep passing after the move:

11. all tiers exhausted then Mock fallback → `TemporarilyUnavailable`,
    warnings contain `context_degraded` ×2 + `timeout` ×3
12. degraded tier persists across calls (two `translate_with_context` calls on
    the same manager; the second run's success carries no context prompt —
    the None tier — proving the stored tier was read at sequence start)
13. all existing manager tests unchanged (public-surface regression proof)

## Future diagnostic ownership (duplicate/noisy translation observation)

Pre-existing symptom, **not** fixed and not classified as a regression. From
the current code, the modules that would own the instrumentation to diagnose
it later:

- `session_id` / `capture_id`: `pipeline_session::PipelineClock` already owns
  both; the capture loop and `Translator::run` already log them.
- `translation_request_id` / `translation_completion_id`:
  `PipelineClock::begin_translation` already issues `translation_id`, but the
  spawned task's stale/cancel logs do **not** currently include it — the
  natural future home for a completion-side log line is
  `pipeline_translation::Translator::run` (which already logs session/capture).
- `normalized_ocr_hash`: `context.rs` `TranslationContext::hash_text` +
  `is_duplicate` (private) — a future dedupe log belongs at the
  `is_duplicate`/`record_ocr_line` call sites in the capture loop, using the
  context module's hashing.
- `stale`/`rejected` flag + `display_event_id`: `pipeline_translation::run`
  (stale discard path) and the notice emitters in the capture loop; there is
  no display-event id today — one would be added in the future, not now.

No IDs or logs are added by this wave.

## Manual validation consequence

The live context-tier orchestration path moves file (the loop that drives
degradation leaves `manager.rs`), so a **small owner smoke** is required on
the final PR head before merge:

1. one clean app launch;
2. one normal subtitle translation — source → translated output succeeds;
3. one stop/quit;
4. no stress/repeat loop (minimizes #107 exposure; do not reproduce
   #103/#105).

No deliberate reproduction of the duplicate/noisy-translation symptom in this
wave. The final #32 wave still requires the issue-level fresh native
translation regression before closure.

## Ratchet

- `config/maintainability-baseline.json`: lower `llm/manager.rs` ceiling from
  856 to the measured post-extraction line count of 726 (must equal measured).
- New production files `llm/translation_planner.rs` (260),
  `llm/translation_planner_tests.rs` (382),
  `llm/translation_planner_exhaustion_tests.rs` (199),
  `llm/manager_tier_tests.rs` (130) must each be ≤ 400 lines (new-file
  ceiling); no new legacy exception; no coverage-floor changes.
- Negative proof: temporarily set the manager ceiling one line below the
  measured count and verify `npm run maintainability` fails, then revert
  (removed before commit).

## Verification

- `cargo fmt --check`, `cargo clippy --locked --lib --bins -- -D warnings`
- focused `cargo test --locked --lib` during development
- full `.\scripts\verify.ps1 -Stage All` before push
- `git diff --check`

## Residual #32 after Wave 4

1. backend-fallback planner ownership (outer loop; now composes the planner)
2. capture → OCR → translate session-loop ownership in `commands.rs`
3. Foundry-specific normal product contracts (naming)
4. HTTP managed-runtime parity decision
5. final issue-level native translation regression before #32 closure
