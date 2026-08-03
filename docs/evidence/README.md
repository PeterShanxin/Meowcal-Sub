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
