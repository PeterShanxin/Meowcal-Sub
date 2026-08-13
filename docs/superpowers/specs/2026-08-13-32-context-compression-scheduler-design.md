# #32 Wave 5 — Context compression scheduler boundary (ContextCompressionScheduler)

**Date:** 2026-08-13
**Parent:** #36
**Issue:** #32 (partial)
**Starting main:** `b415eda73bde7b2d2b05ec0faa9cd402b7863244`
**Branch:** `refactor/32-context-compression-scheduler`
**Worktree:** `C:\FormerD\Repos\wt-meowcal-32-next`

## Goal

Extract the **context compression / summarization state machine** — the async
schedule-and-summarize task currently inline in the `start_translation` capture
loop — out of `commands.rs` into one new #32-owned owner,
`llm/context_summarization.rs` / `ContextCompressionScheduler`, with the
backend dependency behind one narrow `ContextSummarizer` trait.

This is a **structural** wave. It does not change cooldown values, stability
delay, retry counts/delays, drain/restore/cap semantics, stop checks, warning
strings/order, memory-update semantics, or any observable behavior.

`TranslationManager` remains the context-storage owner (drain, restore, cap,
memory, needs-compression, prompt materialization). `commands.rs` keeps only
the schedule call site and the shared generation counter.

## Audit outcome (candidates A-E)

The candidate audit compared five options against current main `b415eda`.
Scoring: 5 = best for the criterion; risk rows score higher when the risk is
lower. Decision rule: architecture ROI > line-count reduction.

| # | Criterion | A scheduler | B frame policy | C session runner | D backend fallback | E no wave |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Ownership clarity | 5 | 3 | 2 | 2 | 0 |
| 2 | State-machine cohesion | 5 | 3 | 4 | 3 | 0 |
| 3 | Adapter/application coupling reduction | 4 | 2 | 3 | 1 | 0 |
| 4 | Deterministic characterization potential | 5 | 2 | 1 | 3 | 0 |
| 5 | Visible-behavior risk (higher = lower) | 4 | 3 | 2 | 4 | 5 |
| 6 | Async/cancellation risk (higher = lower) | 3 | 5 | 2 | 4 | 5 |
| 7 | Mutable-state ownership | 5 | 3 | 3 | 3 | 0 |
| 8 | New-file size fit | 4 | 4 | 1 | 3 | 5 |
| 9 | Complexity removed from commands/manager | 4 | 2 | 5 | 2 | 0 |
| 10 | Future-wave undo/rework (higher = lower) | 4 | 4 | 3 | 2 | 5 |
| 11 | Fake-abstraction risk (higher = lower) | 4 | 2 | 2 | 1 | 5 |
| 12 | Value independent of line-count | 5 | 2 | 1 | 1 | 0 |
| | **Total** | **52** | **35** | **29** | **29** | **30** |

### Selected: A — ContextCompressionScheduler

`commands.rs` currently owns, inline in the capture loop (lines ~893-999 of
1209): the needs-compression gate, wall-clock cooldown, the
`compression_in_flight` lifecycle, the last-scheduled timestamp, the 900 ms
stability delay, generation snapshot/invalidation, stop checks, history
drain, `FoundryLocalBackend` construction + service refresh + availability,
the 3-attempt retry loop, restore/cap failure semantics, and the success
memory update. That is a real state machine with zero automated
characterization today: the Wave-4 audit (spec 2026-08-12) rated it "tests:
none, risk: high". Extracting it makes every branch deterministically
testable with virtual time and removes `commands.rs`'s direct
`FoundryLocalBackend` dependency for summarization.

### Rejected

- **B — frame policy subcut:** the per-frame decision sequence is already
  delegated to small owners (`ocr_gate`, `Notices`, `RecentLines`,
  `repeat_policy`, `Translator::try_spawn`); what remains inline is I/O-glue
  whose inputs are the stateful owners themselves. A typed decision owner
  would relocate `if/else` chains without owning their state, and
  re-ordering risk is high. Fake abstraction.
- **C — full session runner:** would need `AppHandle`, capture, OCR,
  region state, and the manager — a parameter-bag wrapper around
  AppState/Tauri, >400 lines, weakly testable. The seams do not support it
  yet. Rejected per the candidate criteria.
- **D — backend-fallback extraction from `manager.rs`:** re-passing
  config, registry, diagnostics, timeout policy, context storage/tier,
  planner inputs, and display mapping splits one coherent owner into two.
  Confirms #32/#36 guidance not to start a fifth manager refactor.
- **E — no wave:** unnecessary; A passes the fake-abstraction test.

## Ownership map

Measured on starting main: `commands.rs` 1209 (ratchet 1209),
`manager.rs` 726, `context.rs` 732.

| # | Cluster | Current owner | Desired owner (this wave) | Mutable state | Async | External behavior | Tests | Risk |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| S1 | Needs-compression gate | `commands.rs` loop (`manager.needs_context_compression()`) | `ContextCompressionScheduler::schedule_if_needed` | none (reads manager/context) | sync | nothing scheduled when compression not needed | NEW | none |
| S2 | Cooldown decision | `commands.rs` (`last_summary_scheduled_ms`, `now_ms` wall clock) | scheduler (owns `last_scheduled_ms`; caller supplies `now_ms`) | atomic in scheduler | sync | second schedule suppressed inside cooldown; `cooldown==0` bypass | NEW | none |
| S3 | In-flight lifecycle | `commands.rs` (`compression_in_flight` + `CompressionFlagGuard`) | scheduler (`in_flight` + same Drop guard, moved) | atomic in scheduler | sync/spawn | duplicate scheduler suppressed; flag always released incl. panic | NEW | none |
| S4 | Stability delay | inline 900 ms sleep | scheduler task | none | sleep | unchanged | NEW (virtual time) | none |
| S5 | Generation snapshot / invalidation | inline (`context_generation` load) | scheduler task (reads shared `Arc<AtomicU64>`; loop keeps bumping it) | shared atomic (loop-owned, scheduler reads) | sync | generation change after schedule skips before drain | NEW | none |
| S6 | Stop checks | inline `stop_rx` borrows | scheduler task (receives `stop_rx` clone per schedule) | watch channel (loop-owned) | sync | stop before work skips; stop during retry leaves drain as today | NEW | none |
| S7 | History drain | `manager.get_history_for_summarization()` | scheduler task | context storage (manager-owned) | sync | same drain (keep last 3, clear flag) | NEW | none |
| S8 | Backend construction / refresh / availability | `commands.rs` (`FoundryLocalBackend::new` + `refresh_service_status` + `is_available`) | `FoundryContextSummarizer` (llm-owned; a fresh instance per scheduled run via an injected factory, backend built lazily per instance) | `OnceLock` inside the per-run instance | sync init | one fresh backend per summarization run, so a service restarted on a new port is re-detected; disabled/unavailable = silent restore+cap | NEW | none |
| S9 | Retry loop | inline `for attempt in 1..=3` | scheduler task | none | async await | 3 attempts, 500 ms delay, per-attempt stop check, warn strings unchanged | NEW | none |
| S10 | Empty-summary / failure handling | inline | scheduler task | none | async | empty or error warns with attempt number, retries; terminal restores history + caps budget | NEW | none |
| S11 | Success memory update | `manager.update_context_memory` | scheduler task | context storage (manager-owned) | sync | unchanged (set_memory already caps internally) | NEW | none |

`commands.rs` keeps: the session loop, the generation counter bumps (region
change + post-claim, both before the schedule call in the same frame), and a
one-call schedule site. `TranslationManager` keeps all context storage.

## Design

New file `src-tauri/src/llm/context_summarization.rs` (target ≤ 400 lines
including its tests; test file stays under the same ceiling):

```text
SummarizerError          Unavailable | Failed(LlmError)

ContextSummarizer        async_trait, Send+Sync:
                         async fn summarize(&self, history: &[String])
                             -> Result<String, SummarizerError>

FoundryContextSummarizer TranslationConfig:
                         enabled check -> Unavailable
                         lazy backend (OnceLock): new + refresh_service_status
                             on first summarize of the instance
                         is_available -> Unavailable
                         backend.summarize_context -> Failed on Err

CompressionFlagGuard     moved verbatim (Drop releases in_flight)

ContextCompressionScheduler {
    manager:         Arc<TranslationManager>
    make_summarizer: Arc<dyn Fn() -> Arc<dyn ContextSummarizer> + Send + Sync>
                     // injected factory: one fresh summarizer per scheduled run,
                     // preserving the old per-run service re-detection
    generation:      Arc<AtomicU64>          // loop-owned, scheduler reads
    cooldown_ms:     u64
    in_flight:       Arc<AtomicBool>
    last_scheduled_ms: AtomicU64
}
    schedule_if_needed(&self, now_ms: u64, stop_rx: watch::Receiver<bool>)
        - needs_compression gate
        - cooldown gate (0 bypass; saturating_sub)
        - in_flight swap gate
        - store last_scheduled; debug log; spawn task
    spawned task (order pinned to current behavior):
        1. CompressionFlagGuard
        2. sleep STABILITY_DELAY_MS (900)
        3. stop check -> return
        4. generation != scheduled_generation -> skip log, return
        5. drain history; empty -> return
        6. summarizer = make_summarizer()  // fresh per run
        7. retry loop 1..=3:
           - stop check -> return (no restore, as today)
           - summarize:
             Ok(non-empty) -> update_context_memory, return
             Ok(empty)     -> warn "attempt {n} returned empty output"
             Err(Unavailable) -> restore + cap, return (silent, as today)
             Err(Failed(e))   -> warn "attempt {n} failed: {e}"
           - attempt == 3 -> restore + cap, return
           - sleep RETRY_DELAY_MS (500)
```

Constants move with the machine: `CONTEXT_SUMMARY_MAX_RETRIES`,
`CONTEXT_SUMMARY_RETRY_DELAY_MS`, `CONTEXT_SUMMARY_STABILITY_DELAY_MS` become
module consts of the new owner. `commands.rs` loses its two session-local
atomics (`compression_in_flight`, `last_summary_scheduled_ms`), the guard
struct, the consts, and the whole scheduling block; the loop keeps
`context_generation` and calls:

```rust
let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)
    .unwrap_or_default().as_millis() as u64;
context_compression.schedule_if_needed(now_ms, stop_rx.clone());
```

### Documented characterization (behavior preserved by tests)

- availability is evaluated inside the attempt boundary (attempt 1) rather
  than once before the loop; `Unavailable` never retries, exactly like the
  old pre-loop check. No observable change: the old code also returned
  before any summarize call.
- stop during retry keeps the drained history drained (no restore), exactly
  as today.
- repeated `schedule_if_needed` calls never duplicate mutable state:
  in-flight suppresses a second spawn, cooldown suppresses a third.

## Tests

`src-tauri/src/llm/context_summarization_tests.rs` (≤ 400 lines), all with
`#[tokio::test(start_paused = true)]` virtual time or sync assertions, fake
summarizer, real `TranslationManager`/`TranslationContext` (no fake manager),
`now_ms` supplied synthetically. Characterization list:

1. no compression needed -> nothing scheduled, summarizer never called
2. cooldown suppresses a second schedule (and `cooldown==0` bypasses)
3. in-flight suppresses a duplicate schedule
4. stability delay: summarizer not called before 900 ms elapse
5. generation changed during stability delay -> skip before drain (context
   keeps its history)
6. stop before work -> no drain, no summarize
7. empty history -> no summarize
8. disabled summarizer (Unavailable) -> history restored + capped, silent
9. unavailable summarizer (Unavailable) -> history restored + capped
10. successful summary -> memory updated, needs_compression cleared
11. empty summary -> warn path, retried
12. transient failure then success -> 2 attempts, memory updated
13. terminal failure -> history restored + capped after 3 attempts
14. stop during retry -> drained history stays drained (no restore)
15. in-flight flag released after: success, terminal failure, stop,
    panic-in-task path
16. repeated schedule cycles do not duplicate mutable state
17. each scheduled run gets a fresh summarizer instance from the factory
    (per-run service re-detection, as before the extraction)

## Files changed (planned)

- NEW `src-tauri/src/llm/context_summarization.rs`
- NEW `src-tauri/src/llm/context_summarization_tests.rs`
- `src-tauri/src/llm/mod.rs` — module + exports
- `src-tauri/src/commands.rs` — remove inline machine, call scheduler
- `config/maintainability-baseline.json` — lower `commands.rs` ceiling to
  measured value

## Verification

- `cargo fmt --check`
- `cargo clippy --locked --lib --bins -- -D warnings`
- `cargo test --locked --lib`
- `npm run maintainability`
- `git diff --check`
- `.\scripts\verify.ps1 -Stage All`
- ratchet negative proof: measured `commands.rs` line count, prove one line
  above the new ceiling fails `npm run maintainability`, then restore

No native GUI launch from this session; owner manual smoke is a separate
final gate (structural wave: the manual checklist is optional and the owner
decides at PR review; behavior is pinned by characterization tests).
