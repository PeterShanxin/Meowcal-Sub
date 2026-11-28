# Evaluation and benchmarks

Start here for a feature comparison, a performance experiment, or a release
check. Choose the layer that can reproduce the behavior. Use a scenario to
combine layers when one test cannot explain the result.

## Choose a validation layer

Each layer answers a different question. A pass in one layer is not evidence
for another.

| Layer | Use it for | It can establish | It cannot establish |
| --- | --- | --- | --- |
| Policy replay: [`realtime-subtitles/`](realtime-subtitles/README.md) | Queue, pacing, and display-duration changes | Missed, duplicated, and reordered cues, queue depth, skip counts, latency, and minimum visible time under authored, deterministic timing | Recognition accuracy, model quality, Windows capture, overlay rendering, or device speed |
| Translation evaluation: this directory's `subtitle-eval-v1.json` | Prompt, validator, and engine changes | Validator decisions on authored cases; in live mode, warm-model latency and output shape on one machine | Capture, OCR, queueing, or display behavior |
| Core OCR benchmark: [`docs/CORE_PERFORMANCE.md`](../docs/CORE_PERFORMANCE.md) | OCR process-boundary and recognition-parity changes | OCR latency and output parity on synthetic fixtures and one host | Screen capture, subtitle admission, translation, or full-session UX |
| Native fixture gate: [`band-gate/`](band-gate/README.md) | Any visible subtitle-pipeline change | Real Windows capture, OCR, admission, translation, and overlay results per authored cue, including negative and blank intervals | Reliability for every film, font, or caption style |
| [Native media playback](native-playback/README.md) | Releases and changes the maintainer asks to validate on real video | Behavior on the tested clips and hardware | Results for untested media or hardware |
| Diagnostics storage | [Detailed logging microbenchmark](diagnostic-logging/README.md) | Time spent encoding and saving synthetic image pairs as records accumulate | Capture cadence, OCR or translation accuracy, or subtitles displayed during playback |

## Choose a scenario

| Scenario | First comparison | If it regresses |
| --- | --- | --- |
| Fast dialogue or missing subtitles | [Native band gate](band-gate/README.md) and a fixed real-media clip | Use [policy replay](realtime-subtitles/README.md) for skipped, duplicated, and stale cues. Use the [OCR benchmark](../docs/CORE_PERFORMANCE.md) to separate recognition from admission. |
| New OCR model or preprocessing | Compare identical input frames with the [OCR benchmark](../docs/CORE_PERFORMANCE.md), then run the native gate | Separate OCR accuracy and per-frame latency from the number of valid cues that reach the translation queue. |
| Translation or prompt change | Run the subtitle translation evaluation below with fixed text | Use real playback to verify delayed output, and report valid translations separately from source-only or unavailable states. |
| Detailed logging slowdown | [Fresh versus populated log directory](diagnostic-logging/README.md); then compare Basic and Verbose on the same native clip | Measure time spent saving records and text-to-overlay recall separately. |

For a [real-media comparison](native-playback/README.md), use an authored or redistributable clip with known
cue times. Keep the Windows machine, signed Store build (or explicitly labeled
development build), model, player, playback speed, capture region and logging
history the same between runs unless one is the factor under test. Include short
cues and blank intervals. Count cues displayed on time, displayed late, skipped,
or incorrectly displayed during blank intervals. Record source appearance to
overlay timing. Run Basic and Verbose with comparable starting conditions and
alternate order to reduce warmup bias.

Add reusable inputs and runners under `evals/<scenario>/` when they belong to a
new scenario. Link to existing scripts rather than moving entry points used by CI
or other documentation. Keep private video, screenshots and raw diagnostic
records out of Git.

For a visible subtitle change, run policy replay first, then the native fixture
gate, then any required real-media acceptance. Report each layer separately.

## Methodology to record

Record these for every timing or resource result:

- **Commit and build mode.** Name the tested commit. State whether the app is a
  debug or release build; `.\dev-tauri.cmd` builds a debug app with a release
  Core. Never compare debug and release timings.
- **Hardware and software.** CPU, architecture, RAM, GPU and driver, Windows
  build, and the engine's acceleration policy (GPU or CPU).
- **Dataset.** The fixture, case set, or clip identifier and its duration.
- **Warmup.** Report first-call and initialization times separately from
  steady state. Discard or label warmup samples explicitly.
- **Timing boundaries.** Name where each timer starts and stops, for example
  capture start to overlay event, or request sent to response received.
- **Distribution.** Report sample count, p50, p95, and maximum, and keep slow
  samples.
- **Resources.** When resource use matters, sample CPU and memory of the app,
  Core, and engine processes at a fixed interval, and report background load.
- **Comparison.** Compare only runs with the same hardware, build mode,
  dataset, and boundaries, measured in alternating order.

Public evidence follows [`docs/evidence/README.md`](../docs/evidence/README.md).

## Subtitle translation evaluation

`subtitle-eval-v1.json` is the versioned, project-authored, privacy-safe
regression set for Chinese/Japanese-to-English TV subtitles. Cases cover short
and long dialogue, spaced OCR, mixed CJK, multiline text, names, honorifics,
idioms, punctuation, recurring terminology, desktop contamination, and symbol
noise.

Deterministic CI coverage checks:

- valid source shapes are not filtered;
- authored reasonable translations pass the production validator;
- empty, runaway, repetitive, prompt-echo, and wrong-language outputs return
  stable rejection reasons.

Run it from the repository root:

```powershell
.\scripts\run-subtitle-eval.ps1
```

The opt-in live mode reads an application configuration, starts the managed
runtime when needed, sends one fixed warmup request, translates every
non-filter case, shuts down the exact runtime it started, and emits a
privacy-safe JSON report containing case IDs, output shape, validator result,
rejection reason, and latency. It reads the configuration file and does not
write it.

Without `-ConfigPath`, live mode reads the installed application's
`%APPDATA%\com.meowcal.sub\config.json`. The evaluator starts the reviewed Core
in `src-tauri/resources/core/` (or `MEOWCAL_CORE_EXECUTABLE`) with Core's
`development` profile, and passes the engine
location recorded in that configuration, when one exists, as Core's storage
and import root. Pass a development configuration instead unless you intend to
read the installed application's settings and engine location:

```powershell
.\scripts\run-subtitle-eval.ps1 -Live -Runs 3 `
  -ConfigPath "$env:APPDATA\com.meowcal.sub.dev\config.json" `
  -ReportPath .\eval-results\arm64.json
```

The evaluator itself is a debug build (`cargo run` without `--release`); the
measured latency is dominated by the engine runtime, which is always a release
build. Do not commit reports containing machine-specific paths or private test
material. Live wording is intentionally not compared to one exact reference
sentence; grading uses product invariants and leaves semantic quality to the
named manual episode gate.

The live release budgets are p50 at or below 1.5 seconds and p95 at or below
3 seconds. A report is successful only when every translated attempt passes
the production validator and both latency budgets pass.
