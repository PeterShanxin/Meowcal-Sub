# #32 Wave 3 — Translation attempt / retry boundary

**Date:** 2026-08-11
**Parent:** #36
**Issue:** #32 (partial)
**Starting main:** `58303b47f6f07d776fac0c687d06af00b91e43aa`
**Branch:** `refactor/32-translation-attempt-wave3`
**Worktree:** `C:\FormerD\Repos\wt-meowcal-32-wave3`

## Goal

Extract the single-backend translation **attempt / transient-retry state
machine** out of `TranslationManager` into one new #32-owned owner,
`llm/translation_attempt.rs`.

`TranslationManager` keeps backend selection, availability gating, backend
fallback ordering, context-tier planning/degradation, context storage,
diagnostics snapshotting, and the passthrough/display-state mapping.

This is a **structural** wave. It does not change attempt counts, retry
delays, transient classification, validation thresholds, context tiers,
deadline arithmetic, error strings, result codes, or any observable behavior.

## Current TranslationManager responsibility map (main `58303b47`)

| # | Cluster | Current owner | Wave-3 owner | Mutable state | Tests |
| --- | --- | --- | --- | --- | --- |
| A | Backend selection (`ordered_backend_ids`, `backend_by_id`) | `manager.rs` | stays | none | existing fallback tests |
| B | Backend availability (`is_enabled`, `is_available`, `ready_state`) | `manager.rs` | stays | none | existing tests |
| C | Backend fallback ordering (outer loop) | `manager.rs` | stays | warnings | existing tests |
| D | Context-tier selection/degradation (`ContextTier`, `context_tier` AtomicU8, tier loop) | `manager.rs` | stays | `context_tier` | NEW characterization (this wave) |
| E | Context retrieval (memory-prompt prebuild, tier prompt selection) | `manager.rs` | stays | — | — |
| F | **Attempt execution** (per-attempt `timeout(…, translate_with_context_options)`) | `manager.rs` `try_translate_with_retries` | **NEW `translation_attempt.rs`** | none | NEW runner tests |
| G | Transient retry classification (calls `transport_errors::is_transient`) | `manager.rs` (call site) | **NEW module** (call site; classifier stays in `transport_errors`) | none | NEW runner tests |
| H | Retry count (`max_attempts = 1 + FOUNDRY_TRANSIENT_MAX_RETRIES`) | `manager.rs` (outer loop) | stays (knob passed in policy) | none | existing + NEW |
| I | Retry delay / backoff (`600ms × attempt`, budget-guarded) | `manager.rs` | **NEW module** (knob via policy) | none | NEW runner tests |
| J | Translation deadline / budget (`total_timeout = min(backend, backend_budget())`, shared `started`) | `manager.rs` | **NEW module** consumes shared budget | none | NEW runner tests |
| K | Output validation call-site | `manager.rs` | **NEW module** (rules stay in `output_validation.rs`) | none | NEW runner tests |
| L | Rejected-output handling (`low_quality_output`, terminal at tier) | `manager.rs` | **NEW module** (same classification) | none | existing manager test + NEW |
| M | Source-only / failure mapping (`fallback_display_state`, passthrough) | `manager.rs` | stays | none | existing tests |
| N | Diagnostics updates (record_error / record_success per path) | `manager.rs` | split: per-attempt writes move, tier/outer writes stay | `TranslationDiagnosticsState` (single owner, `mod.rs`) | NEW runner tests |
| O | Context mutation after success/failure (tier store on slow/timeout) | `manager.rs` tier loop | stays | `context_tier` | NEW characterization |
| P | Cancellation / stale interaction | `pipeline_translation.rs` / `pipeline_deadline.rs` | unchanged (drop future cancels; runner adds no cancellation) | — | existing |
| Q | Externally visible errors / result codes | `manager.rs` (`TranslationOutcome`) | unchanged public surface | — | existing contract |

`manager.rs` is 1021 lines on main (matches its recorded ceiling). The moved
cluster (F, G-write side, I, J-consumer, K, L, N-attempt) is lines 819–1002
(`try_translate_with_retries` + `TierAttemptResult`), ≈ 175 lines.

## Candidate comparison (A / B / C / D)

| Criterion | A: attempt/retry executor extraction | B: context-tier + fallback planner extraction | C: combined manager decomposition | D: capture/OCR/translate loop from commands.rs |
| --- | --- | --- | --- | --- |
| State machines moved | 1 (attempt/retry loop) | 2 (tier + fallback) | 3 | 3+ |
| Seam quality | policy + request + budget structs; `run(backend, request, budget, ready_state, warnings)` = 5 params | tier loop reads/writes `self.context` (RwLock) + `self.context_tier` (AtomicU8) + config — 8+ live manager-state couplings | many | session/notices/stop channels interleave |
| Characterization coverage | new module fully characterized with fake backends + paused time | partially characterized (manager tests) | same as A+B | partial |
| Retry semantic risk | low (byte-copy + matrix + tests) | low | low | high |
| Context mutation risk | **none moved** (tier loop stays) | high (tier + context storage move) | high | none |
| Diagnostics coupling | writer-only (single diagnostics owner preserved) | shared | shared | none |
| Cancellation/deadline risk | low (shared `started` clock preserved) | medium | medium | high |
| Public-contract exposure | none (module is `pub(super)` within `llm`) | none | none | none |
| Hotspot reduction | `manager.rs` 1021 → ~870 | `manager.rs` 1021 → ~700 | largest | `commands.rs` shrink |
| Later #32 usefulness | durable seam; Wave-4 tier/planner extraction composes on top | marginal extra | marginal extra | depends on A/B |
| Rework chance | low | moderate | high | high |

**Selection: A.** It is the single durable state-machine boundary: one
backend/tier attempt sequence with request, transient retry, validation
invocation, typed outcome, and attempt-level diagnostics. It is the boundary
the Wave-2 spec reserved for "candidate B — later wave". B is rejected because
the tier planner intentionally stays in `manager.rs` this wave (context
conservatism rule, §11 of the wave brief) — it owns `context_tier` mutation and
reads `context` storage. C is rejected because line count alone is not a
reason. D is rejected because manager/attempt ownership must settle first.

## Selected Wave-3 boundary

```text
TranslationManager (manager.rs)
  backend choice / availability / fallback ordering
  context-tier planning + degradation (tier loop, context_tier atomic)
  context storage (Arc<RwLock<TranslationContext>>)
  passthrough + display-state mapping
        |
        v  per tier iteration
TranslationAttemptRunner (llm/translation_attempt.rs)   NEW
  per-attempt request execution (timeout-bounded)
  transient retry classification (calls transport_errors::is_transient)
  retry count / delay budget guards
  validation invocation (calls output_validation::validate_translation_output)
  typed attempt outcome (Succeeded / TimedOut / Failed)
  attempt-level diagnostics writes (record_error / record_success)
        |
        +--> TranslatorBackend (trait)
        +--> output_validation (rules stay)
        +--> transport_errors (classification stays)
        +--> PromptRouterOptions (prompt_router stays)
```

Dependency direction: `manager` → `translation_attempt` → {`TranslatorBackend`,
`output_validation`, `transport_errors`, `prompt_router`}. The new module holds
no locks across `.await`, touches no context storage, no `context_tier`, no
backend registry, no config — it is fully knob-driven.

### New module contract

```rust
pub(super) struct AttemptPolicy {
    pub(super) max_attempts: usize,          // 3 for FoundryLocal, 1 otherwise (computed by manager)
    pub(super) retry_delay_ms: u64,          // FOUNDRY_TRANSIENT_RETRY_DELAY_MS (600)
    pub(super) contexted_attempt_cap_ms: u64,   // DEFAULT_BACKEND_TIMEOUT_MS (2500)
    pub(super) uncontexted_attempt_cap_ms: u64, // UNCONTEXTED_ATTEMPT_TIMEOUT_MS (2500)
    pub(super) prompt_max_context_chars: usize,
    pub(super) prompt_max_source_chars: usize,
}

pub(super) struct AttemptBudget {             // shared clock: created once per foundry sequence
    pub(super) started: Instant,
    pub(super) total_timeout: Duration,
}

pub(super) struct AttemptRequest<'a> {
    pub(super) text: &'a str,
    pub(super) source_language: &'a str,
    pub(super) target_language: &'a str,
    pub(super) context_prompt: Option<&'a str>,
    pub(super) context_used: bool,
}

pub(super) enum AttemptOutcome {
    Succeeded { translated: String, latency_ms: u128, recovered_after_retry: bool },
    TimedOut { total_exhausted: bool },
    Failed(LlmError),
}

pub(super) struct TranslationAttemptRunner {
    diagnostics: Arc<Mutex<TranslationDiagnosticsState>>,
    policy: AttemptPolicy,
}

impl TranslationAttemptRunner {
    pub(super) fn new(policy: AttemptPolicy, diagnostics: Arc<Mutex<TranslationDiagnosticsState>>) -> Self;
    pub(super) async fn run(
        &self,
        backend: &dyn TranslatorBackend,
        request: &AttemptRequest<'_>,
        budget: &AttemptBudget,
        ready_state: ReadyState,
        warnings: &mut Vec<String>,
    ) -> AttemptOutcome;
}
```

The runner is created once per foundry sequence (in the tier loop, replacing
the `try_translate_with_retries` call) and `run` is invoked once per tier
iteration. Constants `FOUNDRY_TRANSIENT_MAX_RETRIES`,
`FOUNDRY_TRANSIENT_RETRY_DELAY_MS`, `DEFAULT_BACKEND_TIMEOUT_MS`,
`UNCONTEXTED_ATTEMPT_TIMEOUT_MS`, `CONTEXT_SLOW_DEGRADE_MS`,
`MAX_TRANSLATION_INPUT_CHARS` stay in `manager.rs` (referenced by manager code
and `manager_tests.rs`); knobs reach the runner through `AttemptPolicy`.

## Before/after behavior matrix (verified by tests)

| Dimension | BEFORE (main 58303b47) | AFTER (this wave) |
| --- | --- | --- |
| Attempt count | `1 + FOUNDRY_TRANSIENT_MAX_RETRIES` = 3 for FoundryLocal; 1 for others | same |
| Retryable errors | `attempt < max_attempts && transport_errors::is_transient(&err)` | same (same classifier) |
| Terminal errors | non-transient, `TranslationError`, exhausted retries | same |
| Retry delay | `600ms × attempt` (600/1200), only when `remaining > delay` | same |
| Uncontexted timeout retry | immediate `continue` (no sleep) when `!context_used && attempt < max_attempts && remaining > 600ms` | same |
| Contexted timeout | no retry; returns `TimedOut { total_exhausted: false }` (tier loop degrades) | same |
| Attempt cap | `min(remaining, context_used ? 2500 : 2500)` | same (both caps passed in policy) |
| Budget | `total_timeout = min(backend_timeout, backend_budget())`; shared `started` across tiers | same (runner consumes `AttemptBudget { started, total_timeout }`) |
| Validation timing | after `Ok(Ok(translated))`, before success | same |
| Rejection | `record_error("low_quality_output")` + `warn!(quality_issue=…)` + `Failed(TranslationError(quality_issue_message))`, no retry | same |
| Validator result-code mapping | `quality_issue_message(reason)` per rejection enum | same (unchanged `output_validation.rs`) |
| Accepted output | `record_success(id, latency)` + `Succeeded { recovered_after_retry: attempt > 1 }` | same |
| Diagnostics: attempt timeout | `record_error(id, "timeout", Some(latency))` every attempt; final warning `"{id}: timeout"` once | same |
| Diagnostics: failure | `record_error(id, err.code(), Some(latency))` | same |
| Success latency | `started.elapsed()` (shared clock) | same |
| Warnings | `"{id}: timeout"` (timeout path), `"{id}: {err}"` (tier loop on `Failed`), `"{id}: context_degraded"`, `"{id}: context_degraded_slow"` (tier loop) | same strings, same call sites |
| Context mutation | tier stored only in tier loop (slow success / timeout degrade); never inside retry loop | same (`context_tier` untouched by new module) |
| Cancellation | dropping the translate future drops the in-flight request; no cancellation checks inside manager | same |
| Stale results | pipeline-level (`pipeline_translation.rs`); unchanged | same |
| Error strings / result codes | `TranslationOutcome`, warnings, `display_state` | same |
| Logging | same `tracing` targets/levels/strings | same (all runner diagnostics writes now use `lock_or_recover`; the success path previously used `lock().unwrap()` and now recovers a poisoned mutex instead of panicking — see Diagnostics ownership) |

## Retry semantics (traced from control flow, not comments)

Per tier iteration, for `attempt in 1..=max_attempts`:

1. `remaining_total = total_timeout - started.elapsed()`; if zero →
   `record_error(timeout)`, warn, push `"{id}: timeout"`, return
   `TimedOut { total_exhausted: true }`.
2. `attempt_timeout = min(remaining_total, cap)` where
   `cap = context_used ? contexted_attempt_cap_ms : uncontexted_attempt_cap_ms`.
3. `timeout(attempt_timeout, backend.translate_with_context_options(…,
   PromptRouterOptions { enable_context: context_used, … }))`.
4. `latency_ms = started.elapsed()` (shared clock — **load-bearing**: the tier
   loop's slow-degrade check `latency_ms > CONTEXT_SLOW_DEGRADE_MS` compares
   this value; a runner-local clock would change tier-degrade decisions).
5. `Ok(Ok)`: validate; on rejection → `record_error("low_quality_output")` +
   warn + `Failed(TranslationError(quality_issue_message))` (no retry); on
   acceptance → `record_success` + `Succeeded { recovered_after_retry: attempt > 1 }`.
6. `Ok(Err)`: `should_retry = attempt < max && is_transient`; record + warn;
   if retry and `remaining_total > delay` → sleep `delay` and `continue`;
   else `Failed(err)`.
7. `Err(_)` (timeout): record + warn; if
   `!context_used && attempt < max && remaining_total > 600ms` → `continue`
   (**no sleep**); else push `"{id}: timeout"` once, return
   `TimedOut { total_exhausted: false }`.
8. Loop end (unreachable in practice): `TimedOut { total_exhausted: true }`.

The `"{id}: timeout"` warning is pushed only on the non-continuing timeout
paths (step 1 and the tail of step 7), so an uncontexted run that times out on
attempts 1–2 and exhausts on 3 pushes it exactly once.

## Deadline / cancellation audit (exact semantics preserved)

- Backend HTTP timeout: `config.timeout_ms` inside the backend client
  (`foundry_local.rs`) — untouched.
- Manager budget: `backend_budget()` = `TRANSLATION_DEADLINE (5s) − 400ms`
  (`pipeline_deadline.rs`) — untouched.
- Shared `started`: created once in the tier loop before memory-prompt
  prebuild; budget decays across tiers and attempts — preserved via
  `AttemptBudget`.
- Sleeps: only the transient-retry sleep (`600 × attempt`) and no new sleeps.
- Refactor hazards checked: no deadline reset per retry (same `started`), no
  full-budget re-grant per retry (same `total_timeout`), no sleep after budget
  expiry (`remaining > delay` guards), no reclassification of timeout after
  wrapping (raw `timeout()` result), no extra attempt (same `max_attempts`),
  no lost early budget check (step 1 per attempt).
- Cancellation: no stop channel inside manager or runner; dropping the future
  drops the in-flight request (unchanged); pipeline-level cancellation in
  `pipeline_translation.rs` untouched.

## Validation ownership

`output_validation.rs` keeps every rule, threshold, and rejection enum.
The runner calls `validate_translation_output` at the same point and maps a
rejection to the same `TranslationError(quality_issue_message(…))`. No
thresholds, rejection rules, or heuristics change. Rejected output is never
retried unchanged (same as today).

## Context ownership

- Context **storage** (`Arc<RwLock<TranslationContext>>`): stays in manager.
- Context **tier planning** (`ContextTier`, `context_tier` AtomicU8): stays in
  manager (tier loop unchanged except the retry call).
- Context **mutation**: stays in manager — the runner never reads or writes
  context storage or `context_tier`.
- Context **summarization**: `commands.rs` + `context.rs` — untouched.
- Degraded-context retry: unchanged — the tier loop degrades, then re-invokes
  the runner with the lower tier's prompt.

## Diagnostics ownership

`TranslationDiagnosticsState` (in `llm/mod.rs`) remains the single owner of
the diagnostics state machine. The runner is a **writer** at exactly the same
points as today (per-attempt `record_error`/`record_success`); tier-loop and
outer-loop writes stay in manager. All runner diagnostics writes use the
repository-standard `lock_or_recover` helper (the pre-extraction success path
used `lock().unwrap()`; the new production owner follows CODING_STANDARDS and
recovers a poisoned diagnostics mutex instead of panicking — a deliberate
repository-standard hardening, not a translation behavior change).
`record_abandoned` (deadline-drop path, `pipeline_translation.rs`) is
untouched.

## Rejected alternatives

- **B (tier + fallback planner extraction)**: moves `context_tier` mutation
  and `context` reads with it; the tier planner is intentionally retained in
  `manager.rs` this wave so the runner owns exactly one state machine.
- **C (combined decomposition)**: line-count-driven; higher risk, no durable
  single boundary gained.
- **D (capture-loop extraction)**: deferred; requires manager/attempt
  ownership to settle first (later #32 wave).
- **Runner-local clock instead of shared `AttemptBudget`**: rejected — changes
  `latency_ms` semantics and the tier loop's slow-degrade decisions.
- **Merging `transport_errors` classification into the runner**: rejected —
  `is_transient` stays the single classifier (§9 of the wave brief).

## Exact non-goals

- No transport changes; no Foundry product rename; no HTTP managed-runtime
  parity; no capture-loop extraction.
- No retry redesign, no context algorithm redesign, no validation threshold
  changes.
- No #59 / #60 / #103 / #105 / #107 fixes; no GPU/runtime launch policy change.
- No change to `TranslationManager`'s public surface or to any caller
  (`commands.rs`, `pipeline_translation.rs`, `http_server.rs`).
- No new full-text logging; no removal of existing local debug logs.

## Characterization plan

New runner tests under `src-tauri/src/llm/` (deterministic fake
`TranslatorBackend` implementations in `translation_attempt_test_fixtures.rs`;
`tokio::time::start_paused` for all timing tests — no real sleeps, no flaky
timing; no unwrap/expect on I/O paths; no locks held across `.await`;
`translation_attempt_tests.rs` + `translation_attempt_timeout_tests.rs` hold
the scenarios, each ≤ 400 lines):

1. first attempt succeeds → `Succeeded`, 1 call, `record_success` written
2. transient error then success → `recovered_after_retry: true`, 2 calls, 600ms
   virtual delay between
3. transient errors exhaust retry count → `Failed(err)`, 3 calls
4. non-transient error → `Failed(err)`, exactly 1 call
5. validator accepts output → `Succeeded` with the translated text
6. validator rejects output → `Failed(TranslationError("…rejected as
   corrupted (overlong output)."))`, 1 call, `record_error("low_quality_output")`
7. rejected output is not retried even though attempts remain (1 call)
8. uncontexted timeout retries without sleep up to `max_attempts`, pushing
   `"{id}: timeout"` exactly once and returning `TimedOut { total_exhausted: false }`
9. contexted timeout does not retry → `TimedOut { total_exhausted: false }`, 1 call
10. budget exhausted at entry → `TimedOut { total_exhausted: true }`, 0 calls
11. transient error when `remaining <= delay` → no sleep, `Failed` immediately
12. retry delay is `retry_delay_ms × attempt` (virtual-time assertions)
13. diagnostics record the same error/latency keys as today
14. cap selection honors `context_used` (min(remaining, cap))
15. `PromptRouterOptions { enable_context }` mirrors `context_used`

Manager-side characterization added to `manager_tests.rs` (pass against
current code, must keep passing after the move):

16. context tier degrades on timeout (Full → MemoryOnly) — asserts
    `foundry_local: context_degraded` warning and fallback display state
17. context tier degrades on slow success — asserts
    `foundry_local: context_degraded_slow` warning and `Translated` outcome
18. existing fallback/validation tests keep passing unchanged (public-surface
    regression proof)

## Ratchet

- `config/maintainability-baseline.json`: lower `llm/manager.rs` ceiling from
  1021 to the measured post-extraction line count (must equal measured).
- New `llm/translation_attempt.rs` must be ≤ 400 lines (new-file ceiling).
- No new legacy exception; no coverage-floor changes.
- Negative proof: temporarily raise the manager ceiling by one line and verify
  the maintainability gate fails, then revert (removed before commit).

## Manual validation consequence

The live translate attempt/retry path moves (execution leaves
`FoundryLocalBackend` call site unchanged, but the retry loop that drives it
changes file), so a **small owner smoke** is required before merge on the final
PR head:

1. one clean app launch;
2. one normal subtitle translation — source → translated output succeeds;
3. one stop/quit;
4. no stress/repeat loop (minimizes #107 exposure; do not reproduce #103/#105).

No long benchmark. The final #32 wave still requires the issue-level fresh
native translation regression before closure.

## Verification

- `cargo fmt --check`, `cargo clippy --locked --lib --bins -- -D warnings`
- focused `cargo test --locked --lib` during development
- full `.\scripts\verify.ps1 -Stage All` before push
- `git diff --check`

## Residual #32 after Wave 3

1. Context-tier planner + backend-fallback planner extraction (now has a clean
   seam: `AttemptPolicy`/`AttemptBudget`/`AttemptRequest` are reusable inputs)
2. Capture → OCR → translate session-loop ownership in `commands.rs`
3. Foundry-specific normal product contracts (naming)
4. HTTP managed-runtime parity decision
5. Final issue-level native translation regression before #32 closure
