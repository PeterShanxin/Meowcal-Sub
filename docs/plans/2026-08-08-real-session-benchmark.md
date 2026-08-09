# Real-Session Benchmark: HY-MT1.5 vs Hy-MT2

Status: COMPLETE (decision frozen 2026-08-09; see "Decision" below)
Date: 2026-08-08

## Goal

Decide whether the app's translation engine should ship Hy-MT2-1.8B-Q4_K_M
(config C) instead of the current HY-MT1.5-1.8B-Q4_K_M (config A), by running
the complete production pipeline (Prompt Router -> Foundry Local wire contract
-> production output validation) against **real production sessions** extracted
from the user's own app logs. No synthetic data, no manual re-typing.

## Configs

| Config | Model | Runtime | Notes |
|---|---|---|---|
| A | HY-MT1.5-1.8B-Q4_K_M | llama.cpp b10155 (app-managed) | as shipped today |
| B | HY-MT1.5-1.8B-Q4_K_M | llama.cpp b10327 (isolated, seed 2026) | engine control arm |
| C | Hy-MT2-1.8B-Q4_K_M | llama.cpp b10327 (isolated, seed 2026) | candidate |

- The wire contract is identical across configs: same prompt router, same
  max-tokens budget, same validation, only the model alias and endpoint
  differ.
- Config B isolates the runtime variable (b10327 vs b10155) while keeping the
  model fixed; C then swaps the model on the same runtime.
- A uses the app-managed runtime exactly as the app would start it (managed
  runtime record in the app config preserved).

## Session selection (from app log extraction)

Sessions are `meowcal-sub_<date>_<start>.log` pairs of
`Translated, source: <ocr>` / `Translation request ...` lines. The source text
is the exact OCR the app sent to its engine, preserved byte-for-byte.
Per-session timestamps and viewing-context notes are private and intentionally
not recorded here; only the selection methodology and aggregate counts are kept.

Selection bar: >= 15 min span, >= 100 pairs, spread over multiple days;
max 5 sessions, one per start-hour to avoid double-counting one binge.
Directions covered: en-US -> zh-CN (4 sessions) and zh-CN -> en-US mixed (1).

Total: 2,653 subtitle lines across 5 sessions.

## Datasets

- `eval-results/bench/datasets/s<i>_<session>.json` - full session dataset
  (`allowUnreferencedCases: true`, no hand-authored references).
- `eval-results/bench/datasets/s<i>_<session>_stability.json` - ~40-line
  stride subset for seed-stability probing.
- Each line re-classified with the **current** `is_untranslatable_text`
  semantics (2+ meaningful chars -> translate, else -> filter); the
  deterministic gate is clean on all 5 datasets (2,653 checks, 0 failures),
  proving the extraction is faithful.

## Harness changes (this work)

All backwards-compatible; shipped `evals/subtitle-eval-v1.json` unaffected.

1. `SubtitleEvalDataset.allow_unreferenced_cases` (default false) - lets
   real-session datasets omit reference translations; live grading still
   validates every output through the production validator.
2. CJK-aware live grading - target zh/ja/ko requires CJK letters in the
   output (`no_cjk_output`); latin targets keep `no_latin_output`.
3. `LiveCaseResult.output` - raw output text captured in every report for
   artifact generation without re-running engines.
4. Request-error tolerance in `subtitle-eval`: a timed-out or failed request
   records a failed case (`reason: request_error: ...`) instead of aborting
   the whole run.

## Scripts

The experiment used the scripts below. Only the reusable analysis layer is
committed to main (see "Tooling disposition"); the experiment-wiring scripts
remain in the benchmark worktree only.

- `scripts/extract-sessions.ps1` - log -> session JSON (OCR pairs). [experiment-only]
- `scripts/build-session-datasets.ps1` - session JSON -> eval datasets. [experiment-only]
- `scripts/run-engine-eval.ps1` - one config/dataset eval (server lifecycle). [experiment-only]
- `scripts/run-bench-batch.ps1` - full battery: 15 full-session runs + 30
  stability runs, manifest-driven. [experiment-only]
- `scripts/analyze-bench.ps1` - manifest + reports -> aggregate.md/json. [committed]

## Execution plan

1. [x] Harness changes + tests (8 tests pass, incl. CJK + reference-free).
2. [x] Build release binary; deterministic gate clean on all datasets.
3. [x] Config B smoke (MT1.5 on b10327, seeded) - healthy, p50 462 ms.
4. [x] Battery: full sessions (A/B/C x 5) + stability (B/C x seeds x 5).
5. [x] Aggregate: latency, throughput, failure modes, stability.
6. [x] Blind pairwise qualitative review + examples.
7. [x] Artifacts + verdict.

## Open items

- Config A runs use the managed runtime; the managed server persists between
  A runs (expected - it is the app behavior). B/C servers are stopped after
  each run.

## Decision (frozen 2026-08-09)

After both the historical-visible blind review (`review/blind-*`,
`review/attribution.*`) and the independent historical-free cross-check
(`review/crosscheck-*`, scored blind in
`review/crosscheck-scoresheet-independent.md`):

- **KEEP HY-MT1.5-1.8B-Q4_K_M as production/default.**
- **DO NOT promote Hy-MT2-1.8B-Q4_K_M as a production replacement yet.**
- **DO NOT add a user-facing MT1.5/MT2 model selector.**
- **DO NOT characterize MT2 as simply a worse translator** - clean/general
  translation quality was approximately tied (see corrections below).
- Keep the isolated MT2 evaluation path/artifacts (machine-local, not in the
  repo) for possible future targeted experiments.

Rationale summary: on ordinary/cleaner lines the two models are
statistically indistinguishable (historical-free means: a 3.38, b 3.30,
c 3.34; pairwise a-vs-c 16/19/15, b-vs-c 16/17/17). MT2 has genuine
semantic wins on some meaningful cases. But MT2 is materially worse on
OCR/noisy inputs (noise-pool mean 1.90 vs 2.30/2.40) and showed worse
latency-tail behavior in the full-session battery (p95 c-s1 3333 ms vs
~890 ms for a/b; one 60 s timeout). Meowcal consumes OCR-derived text in
production, so OCR/noise robustness is part of product quality; that
dominates the decision.

## Methodology corrections (preserved from the review process)

1. **Historical-visible review may carry incumbent/reference anchoring.**
   The original blind review (`blind-*`) showed the historical production
   translation on every case, exposing reviewers to the incumbent output.
2. **Historical-free independent review found translation quality
   approximately tied.** A fresh 50-case pack without historical output,
   fresh per-case X/Y/Z permutations, scored blind by an independent
   reviewer, separated the configs only on the noise pool. The MT1.5<->MT2
   gap (0.04 mean) is smaller than the within-MT1.5 runtime-arm gap (0.08).
3. **MT2 remains worse for OCR/noise and latency tail.** noise-pool mean
   1.90 vs a/b 2.30/2.40 (serious 8/10 vs 7/6); p95 tail and one hard
   timeout in the long session.
4. **A vs B is NOT a seed-only comparison.** Config A runs the app-managed
   llama.cpp b10155 (unseeded, persistent server); config B runs isolated
   b10327 with seed 2026. Runtime revision and server lifecycle also differ,
   so a-vs-b differences must not be attributed to seeding alone.

## Future evaluation funnel (recommended)

Avoid another 6-hour battery for initial model screening:

- **Stage 1 - ~50-case historical-free blind screening.** Build a compact
  pack (like `build-crosscheck-pack.ps1`), score blind, decode. Roughly an
  hour of reviewer time. Only proceed if the candidate stays competitive.
- **Stage 2 - ~100-150 targeted cases** (disagreement + OCR/noise + general)
  only if the candidate remains competitive after Stage 1.
- **Stage 3 - full-session sustained benchmark** (the 2,653-line battery)
  only for finalists.

## Tooling disposition (landed 2026-08-09)

Classification of the benchmark scripts against the reusable-infra landing.

**COMMITTED to main (reusable infrastructure):**
- `scripts/build-crosscheck-pack.ps1` - historical-free blind pack builder.
  Seeded RNG (deterministic, reproducible), scoped writes to
  `review/crosscheck-*`, no private/historical data in the pack (key kept
  separate). Independently verified: decoding the completed scoresheet
  through its key reproduces every frozen number exactly, proving the
  pack/key pair is correct. Core Stage-1 tool for the funnel. Caveat: it
  consumes the first review's artifacts (`blind-pack/blind-key/
  blind-results/src-class-*`), so it is a second-pass tool, not standalone.
- `scripts/analyze-bench.ps1` - aggregate latency/failure/stability; can
  reconstruct the run list from report filenames when `bench-manifest.json`
  is missing, so it works directly with `subtitle-eval --report` output.
- `scripts/aggregate-blind.ps1` - decode blind scores -> results.
- `scripts/attribute-failures.ps1` - meaningful/noise failure attribution.
- Rust harness changes in `src-tauri/src/subtitle_eval.rs` and
  `src-tauri/src/bin/subtitle-eval.rs` (allow-unreferenced datasets, CJK-
  aware grading, per-case output capture, request-error tolerance; 4 new
  tests).

**NOT COMMITTED (experiment-wiring, kept in the benchmark worktree only):**
- `scripts/run-engine-eval.ps1` - per-config eval driver (a/b/c), server
  lifecycle, seed pinning, `-AllowFailures`. Machine-local default paths; main
  intentionally does not ship a config-specific eval driver. The reusable
  `subtitle-eval` binary covers single-config evaluation.
- `scripts/run-bench-batch.ps1` - battery orchestrator, manifest-driven,
  seeded; writes per-run reports/logs. Experiment-specific orchestration.
- `scripts/extract-sessions.ps1` - depends on private/local app logs
  (`%APPDATA%\com.meowcal.sub\logs`) and writes the user's watched content
  + historical translations to session JSONs (gitignored). Regex parsing
  duplicates the app's log format knowledge and is fragile to log changes.
- `scripts/build-session-datasets.ps1` - replicates the Rust
  `is_untranslatable_text` gate in PowerShell (divergence risk).
- `scripts/make-review-pack.ps1` - historical-visible by design (bakes the
  historical translation into the pack/scoresheet), which the independent
  cross-check identified as an anchoring risk; also uses non-seedable
  `Get-Random`, so runs are not reproducible. If a large historical-free
  pack is ever needed, extend `build-crosscheck-pack.ps1` instead.
- `scripts/score-review.ps1` - duplicates `aggregate-blind.ps1`'s decoding
  and never produced the shipped artifacts (`verdict.json` is absent).
- `scripts/slice-blind-pack.ps1` - one-off convenience (text slices); the
  scoresheet path covers the reviewer-facing need; unused by the
  cross-check flow.
