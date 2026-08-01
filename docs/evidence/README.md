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

`2026-08-01-arm64-package.json` records the locally generated ARM64 MSI and
NSIS artifacts. Reproduce it with:

```powershell
.\scripts\build-package.ps1 -Architecture arm64
```

The package report proves artifact generation and hashes only. Installation,
upgrade, repair, uninstall, and first-run behavior remain manual Windows gates.
