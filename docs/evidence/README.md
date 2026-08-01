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

`2026-08-01-arm64-package.json` records the locally generated ARM64 MSI and
NSIS artifacts. Reproduce it with:

```powershell
.\scripts\build-package.ps1 -Architecture arm64
```

The package report proves artifact generation and hashes only. Installation,
upgrade, repair, uninstall, and first-run behavior remain manual Windows gates.

The `Windows Packages` workflow also produced the native x64 MSI and NSIS
artifact bundle on `f62de5a` (run `30691128927`, artifact
`meowcal-sub-x64-packages`). This proves x64 package generation only; it does
not prove x64 runtime behavior, performance, or installer lifecycle behavior.
