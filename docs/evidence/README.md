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
