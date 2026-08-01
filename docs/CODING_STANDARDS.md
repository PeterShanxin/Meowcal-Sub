# Coding Standards

These rules apply to production code, tests, scripts, and repository tooling.
They are intentionally stricter around privacy, process ownership, and visible
subtitle behavior.

## General

- Prefer the smallest change that satisfies the reviewed contract.
- Make ownership explicit: one module owns each state machine and lifecycle.
- Use typed data and stable result codes across backend/UI boundaries.
- Keep adapters thin; business rules belong in testable services.
- Preserve backward-compatible configuration through explicit migrations.
- Do not log secrets or private screen text by default.
- Explain non-obvious invariants and safety constraints, not line-by-line syntax.

## Rust

- `cargo fmt --check` and `cargo clippy -- -D warnings` must pass.
- Do not use `unwrap`, `expect`, or `panic!` on user, I/O, runtime, network, or
  persisted-state paths.
- Use `thiserror` or a typed error enum at domain boundaries. Preserve stable
  error/rejection codes for the UI and tests.
- Do not hold a synchronous mutex guard across `.await`.
- Move blocking filesystem, process, hashing, and Windows API work off async
  executor threads where it can stall the UI or pipeline.
- Own child processes by exact handle/PID and stop only processes created or
  explicitly adopted by the app.
- Bind local services to loopback and treat ports as reserved resources.
- Tauri command handlers should validate/convert input, call a service, and map
  output. New state machines do not belong in `commands.rs`.

## Frontend

- ADR-0003 owns the approved incremental frontend stack: Vite multi-page output,
  TypeScript state/controllers, and Lit custom elements for migrated surfaces.
  Keep selector/overlay legacy behavior behind dedicated Vite entries until #34
  migrates them deliberately; do not create a second state owner.
- Route backend calls through `src/scripts/tauri-bridge.js`.
- Keep DOM lookup/event wiring separate from state transitions and presentation
  helpers.
- Do not duplicate backend readiness rules in display strings.
- Escape or assign user/model text through text-safe DOM APIs; never inject it
  as HTML.
- Support keyboard operation, visible focus, semantic labels, and reduced
  motion for new interactions.
- Browser-mode tests cover presentation and bridge contracts only; they are not
  evidence for Windows-native behavior.

## Translation pipeline

- Model each stage explicitly:
  `capture -> preprocess -> OCR -> normalize -> dedupe -> translate -> validate -> overlay`.
- Make cancellation, stale results, retries, and fallback states observable and
  testable.
- Retry only failures classified as transient and remain within a bounded
  latency/attempt budget.
- Deterministic output rejection is not retried unchanged.
- Validation must account for source and target writing systems.
- Raw OCR may be shown only as an explicit `sourceOnly` state, never as a
  successful translation.
- Context is source-only, bounded, and disabled by default until a named
  evaluation proves a benefit within latency budgets.

## Tests

- Add a regression test that fails before each bug fix.
- Test stable behavior and contracts, not implementation trivia.
- Cover success, typed failure, cancellation, and boundary conditions for new
  state machines.
- Keep network/model tests deterministic by default. Live engine tests must be
  opt-in and record their environment.
- Prove every new threshold or ratchet can fail.
- A passing automated suite does not replace required manual Windows evidence.

## Performance

- Record baseline method, hardware, dataset, and percentile before optimizing.
- Measure capture, OCR, translation, validation, and overlay separately.
- Do not trade correctness or privacy for unmeasured speed.
- Use bounded queues, dedupe, cancellation, and stale-result suppression before
  adding concurrency.

## Repository hygiene

- Use UTF-8, LF-compatible text, and PowerShell-safe commands.
- Do not commit ignored build resources, model binaries, generated installers,
  logs, or private OCR samples.
- Keep package versions synchronized according to `CONTRIBUTING.md`.
- Existing hotspot files are tracked debt. Touched production code must not
  increase an approved ceiling without an explicit reviewed exception.
- New production files and legacy exceptions are enforced by
  `config/maintainability-baseline.json`; update a reduced ceiling or warning
  budget in the same pull request that creates the improvement.
