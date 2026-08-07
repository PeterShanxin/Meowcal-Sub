# Verification evidence

This directory contains privacy-safe, reproducible release evidence. Subtitle
evaluation reports include only authored case IDs, architecture, engine/model
identity, output shape, validator decisions, and latency. They intentionally
exclude OCR source text, model output text, configuration paths, and machine
identifiers.

`2026-07-31-arm64-subtitle-eval.json` was generated with:

```powershell
.\scripts\run-subtitle-eval.ps1 -Live -Runs 3 `
  -ReportPath .\docs\evidence\2026-07-31-arm64-subtitle-eval.json
```

`2026-08-01-arm64-subtitle-eval-after-extract.json` is a follow-up run after
the compatibility-command extraction. All 33 translated attempts passed the
quality grader, but this run measured p50 807 ms and p95 1,842 ms, just over
the approved warm-model budgets. It records a current budget exception, not a
performance improvement claim.

`2026-08-01-arm64-subtitle-eval-parallel1-warmup.json` is a comparable run after
the app-owned runtime was constrained to one server slot and the evaluator
added a fixed warm-up request. All 33 translated attempts passed the quality
grader; p50 was 660 ms and p95 was 3,558 ms. The matching pre-change run in
`2026-08-01-arm64-subtitle-eval-auto-warmup.json` measured p50 841 ms and p95
4,091 ms. This is a measured ARM64 improvement in median latency, but the p95
budget exception remains open and this is not x64 or capture-to-overlay
evidence.

`2026-08-01-arm64-package.json` records the locally generated ARM64 MSI and
NSIS artifacts. Reproduce it with:

```powershell
.\scripts\build-package.ps1 -Architecture arm64
```

The package report proves artifact generation and hashes only. Installation,
upgrade, repair, uninstall, and first-run behavior remain manual Windows gates.

`2026-08-01-arm64-tauri-window-smoke.json` records a fresh launch of the
current ARM64 Tauri debug binary after the managed-start UI change. The window
created once at a responding final rectangle; this is only a window smoke and
does not prove OCR, capture, translation, overlay, or episode behavior.

`2026-08-01-arm64-managed-start-smoke.json` records the stronger Tauri path:
with the app-owned runtime stopped, clicking Start Translation started and
warmed HY-MT, entered the running state without showing the warm-up modal, and
stopped the pipeline cleanly. The normal tray-close behavior keeps the app
alive; explicit process cleanup was verified separately. OCR, overlay, and
episode validation remain separate gates.

`2026-08-01-arm64-ocr-alias-smoke.json` records the live Windows OCR readiness
check: the app selected `zh-CN` while Windows reported the equivalent installed
tag `zh-Hans-CN`, with no false "not installed" warning. Recognition and overlay
rendering remain separate gates.

`2026-08-03-arm64-ocr-preprocessing-ab.json` answers the first question in
issue #53: whether hard binarization is what splits Chinese glyphs into their
radicals. It is not. Across three subtitle runs captured during live playback,
binarization lost 4.9 points of character accuracy on a low-contrast blue
background and gained 15.0 on a red one, and radical splitting appeared under
every variant including no preprocessing at all. The shipped default was left
unchanged. Reproduce it with:

```powershell
$env:MEOWCAL_OCR_AB_FRAME = "<directory of captured frames>"
$env:MEOWCAL_OCR_AB_TRUTH = "<what those frames say>"
cargo test --lib preprocessing_variants -- --ignored --nocapture
```

Ground truth is supplied at run time and not stored. The report keeps accuracy
figures, frame counts, and run shape only.

The `Windows Packages` workflow also produced the native x64 MSI and NSIS
artifact bundle on `2883966` (run `30699524713`, artifact
`meowcal-sub-x64-packages`). This proves x64 package generation only; it does
not prove x64 runtime behavior, performance, or installer lifecycle behavior.

`2026-08-05-arm64-engine-thread-sweep.json` picks the engine's `--threads`
value from measurement rather than intuition, and records a counter-finding.
llama.cpp's unset default was the slowest configuration tested on an idle host
(443ms median, 19.3 tok/s); eight threads held a loaded p90 of 377ms where six
reached 3814ms and the default 22695ms. Eight was chosen for that reason.

The counter-finding matters more than the choice: capping threads does **not**
fix the tail. Every setting tested, including the chosen one, still produced
single calls between 53 and 86 seconds under sustained load, which refutes the
thread-barrier explanation originally offered in issue #60. The translation
slot's own deadline is what bounds that outage. Reproduce with one server per
setting on a spare loopback port:

```powershell
llama-server.exe -m <model> --port 11440 -c 2048 -ngl 0 --jinja --no-webui `
  --parallel 1 --threads <N>
```

Twelve lines per setting on one 12-core machine, so the loaded maxima are
single observations and the cores-minus-four rule is extrapolated from a single
core count. Because of that last point the rule is applied only on `aarch64`,
where it was measured; elsewhere `available_parallelism` counts logical CPUs and
"cores minus four" could exceed the physical core count outright.

`2026-08-05-arm64-abandoned-request-frees-slot.json` checks the claim the
translation deadline rests on: that abandoning a request also stops the engine
generating. It does. Disconnecting mid-generation made llama-server log `cancel
task`, release the slot 9ms later having stopped at 307 tokens rather than
running to `max_tokens`, and start the next request 5ms after that.

This deserves its own file because the deadline's value depends entirely on it.
If the server finished abandoned work instead, the next line would queue behind
it and miss its deadline too, and the overlay's "engine is falling behind" hint
would become permanent rather than occasional — the deadline would buy staleness
protection and no throughput at all. Decided from the server's own log rather
than client timing, which cannot tell a cancelled generation from a fast one.
