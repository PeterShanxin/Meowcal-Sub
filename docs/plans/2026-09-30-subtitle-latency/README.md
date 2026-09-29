# Subtitle latency investigation — 2026-09-30

Native baseline defects are reproduced. No native speedup is established: candidate
runtime measurements remain outstanding. The three fixes are independent draft
changes; this report is the shared evidence record.

| Change | PR | Native candidate gate |
|---|---|---|
| R1 — completion deadline | [#269](https://github.com/PeterShanxin/Meowcal-Sub/pull/269) | Pending, including Core release/pin |
| R2 — capture teardown | [#270](https://github.com/PeterShanxin/Meowcal-Sub/pull/270) | Pending |
| R3 — readiness refresh | [#271](https://github.com/PeterShanxin/Meowcal-Sub/pull/271) | Pending |

## Environment and method

- Baseline: `171b1ec14484ca22c896412d03705d0db2880005`, app 0.8.6,
  reviewed Core `core-v0.1.4`. PR #263 is already merged; its context replay
  validation and recovery remain in place.
- Windows 11 ARM64, build 26200, 32 GiB RAM; Qualcomm Adreno X1-85,
  driver 31.0.148.0; Balanced power plan, plugged in, battery 99–98%.
  About 10 GiB free before model loading and 7.3 GiB while loaded.
- Native debug application, development profile, built with
  `npm run tauri:build -- --debug --no-bundle --config src-tauri/tauri.dev.conf.json`.
  Baseline executable SHA-256:
  `ECEF5283AD96B48E3AAD7A92DE22A44033255A7B55D9B94486EB0F622CB986D0`.
- HY-MT1.5-1.8B Q4_K_M, llama b10155 OpenCL Adreno ARM64, GPU enabled;
  en-US → zh-CN, context disabled, capture interval 250 ms, `RUST_LOG=info`.
- Physical capture rectangle `(100,500,1200,120)`, scale factor 1,
  2560×1440 display, native WinForms fixture: Arial 44 px white text on black.
  Five fixed cues, two passes, four seconds per cue. No VM or installer used.
- Warm means the same ready Core/llama processes remain loaded between samples.
  Process-cold means no app, Core, or llama process exists before launch; it does
  not mean the OS file cache is cold. One first preparation took 9,282.7 ms;
  it is not a repeatable cold-start distribution. A separate cold-start harness
  failed to find the main WebView and produced no valid timing samples.

Readiness and deadline tests invoke real native application commands through
WebView2 CDP. They are not browser bridge mocks, but they bypass capture/OCR.
The pipeline test uses actual Windows capture, OCR, GPU translation and overlay.
Source fixture `Refresh()` completion to overlay double `requestAnimationFrame`
is a **render proxy**, not a measurement of readable desktop pixels. `modelMs`
includes manager waits, retries and validation. `totalMs` begins at the accepted
frame and excludes the earlier capture/stability wait.

## Baseline and candidate comparison

All values are milliseconds. Quantiles use linear interpolation. These small
samples describe these runs only; p95 is not a reliable population tail estimate.
Raw values and exact text/output are in [baseline](baseline/summary.json).

| Measurement | n | Baseline p50 / p95 | Baseline range | Candidate native result |
|---|---:|---:|---:|---|
| Source paint → overlay render proxy | 10 | 891 / 1001.1 | 616–1020 | Not measured |
| Accepted-frame `totalMs` | 10 | 543.5 / 588.6 | 369–594 | Not measured |
| Manager `modelMs` | 10 | 530 / 572.3 | 354–575 | Not measured |
| Quiet native translation RPC | 5 | 503.8 / 539.6 | 340.4–545.7 | Not measured |
| Translation with three concurrent refreshes | 5 | 494.7 / 543.5 | 344.4–552 | Not measured |
| Long request, temporarily unavailable | 5 | 4613 / 4619.1 | 4610.9–4620.3 | Not measured |
| Short request immediately after long request | 5 | 1530 / 3560.1 | 1186.3–3911 | Not measured |
| Normal Stop completion | 10 | 249.6 / 265.7 | 236.9–272.3 | Not measured |

The pipeline delivered 10/10 correct translations, with no unavailable outcomes.
All five deliberately long requests became temporarily unavailable; all five
following short requests translated correctly, but waited progressively longer.
This stress sequence establishes an adverse native scenario, not its frequency
in normal video playback. Normal immediate restart succeeded 10/10 times.
With the owned OCR process suspended, Stop returned in 252.7 ms while
`is_running=true`; immediate Start failed with `Translation is already running`
(one sample). The suspended process was resumed in `finally`.

## Review dispositions and retained changes

**R1, confirmed.** The manager expires attempts before the old 30-second Core
request budget. The shared translation channel drains active requests after
caller cancellation. A controlled pipe test reproduces the budget mismatch;
the native long/short sequence shows the user-facing backlog risk. The candidate
passes one absolute deadline from manager through backend and client, subtracts
queue/handshake time, and bounds Core health checking plus HTTP completion.
Normal expiry has a distinct error and bypasses transport recovery. A two-second
response-drain grace preserves framing; it does not extend inference. Transport
and loopback HTTP tests cover channel reuse and HTTP disconnect. Actual llama
GPU cancellation, next task entry time and retained warm PIDs still need native
candidate evidence. Core source changes require a reviewed release and consumer
pin update; the existing pin is deliberately unchanged.

**R2, confirmed.** Native delayed OCR reproduces the Stop/Start mismatch.
The candidate races OCR against Stop, joins capture cleanup before acknowledging
Stop, serializes Start/Stop, and bounds teardown with cooperative then aborted
waits. It does not clear `is_running` before old capture cleanup. Regression
tests cover cancellation, cleanup completion and bounded abort. Native delayed
OCR, stale overlay suppression and the next session still require retesting.

**R3, confirmed.** Five native busy refresh groups sent 15 readiness RPCs.
The controlled AppController test independently reproduces three concurrent
requests. The candidate distinguishes busy from preparation, retains a known
ready snapshot during completion, and coalesces only the active preparation
promise. Success/failure clears that promise; a later refresh can retry. Tests
cover these transitions. The baseline does not show a measurable slowdown from
the duplicate RPCs; removing them is not evidence of faster model loading.

## Profiling interpretation and remaining gates

In the healthy pipeline, manager time accounts for nearly all accepted-frame
time (the difference is 13–19 ms). The source-to-render proxy adds hundreds of
milliseconds before/after that interval. This does not isolate GPU readback,
OCR stability admission, or compositor delay. No texture readback, startup,
candidate queueing, model concurrency, quality validation or safety policy was
changed without attributable measurement. No speculative speed change was
retained. The failed cold-start harness is excluded from benchmark results.

R1 passed the full local Windows `scripts/verify.ps1 -CoreSourceCandidate` gate:
95 Core unit tests (3 ignored), 1 executable and 15 protocol tests, 543 application
unit tests (4 ignored), 16 IPC and 2 command contracts, 466 frontend unit tests,
14 browser tests, formatting, lint, typecheck, web build and audit. A subsequently
added timeout-policy regression passed with all 15 attempt tests. R2's three
lifecycle tests and R3's six engine status tests pass; R3 also passes all 470
frontend tests. Exact-head CI and build results are recorded in the individual PRs.
Browser and controlled tests do not substitute for native candidate measurements.
All three PRs
remain draft until the changed native flows pass on their final heads. Remaining
highest-ROI work is R1's paired native retest with llama task/cancellation logs,
followed by R2 delayed-OCR restart and R3 real focus/return. Then repeat the
ten-cue pipeline and obtain actual presentation evidence. No user-perceptible
speedup is claimed from the present evidence.

Native candidate launching and continuous desktop pixel sampling were rejected
by automatic approval with `blocked by policy`; neither was bypassed. Existing
baseline measurements and controlled tests cannot fill those gaps.

## Reproduction

Use a development identity, preserve its configuration, and confirm sufficient
free memory and absence of another inference owner before loading one model.
Do not change power, GPU driver or device state between comparisons. Launch the
chosen native debug build with WebView2 CDP on a free localhost port 9241.

1. Run `scripts/subtitle-latency-fixture.ps1 -OutputDirectory <fixture-dir>` in
   a native PowerShell process; use its `capture-region.json` in the app.
2. Prepare the engine once. Run `node scripts/benchmark-native-deadline.mjs
   <results>/deadline.json`. The fixed long/short inputs are part of the script.
3. Run `scripts/benchmark-native-refresh.mjs` from the R3 branch for alternating
   quiet/busy refresh groups, without unloading the model.
4. Start capture and minimize the main window, leaving only the fixture in the
   selected region. Run `node scripts/benchmark-native-pipeline.mjs
   <results>/pipeline.json <fixture-dir>`; stop capture afterwards.
5. The R2 branch contains `scripts/benchmark-native-stop.ps1` for ten normal
   immediate restart cycles and `scripts/benchmark-delayed-ocr.ps1`. Supply the exact
   owned app and OCR PIDs; it refuses a Core with inference children and always
   resumes the suspended OCR process. Never supply a user-owned process.
6. Copy the same named result files into a separate baseline/candidate directory
   and run `node scripts/summarize-subtitle-latency.mjs <results>`.

Retain source SHA, executable hash, Core release/source-candidate identity,
configuration, process ownership, free memory, and raw samples for every run.
Do not compare source Core against a pinned Core without disclosing that change.
