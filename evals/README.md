# Subtitle evaluation

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

The opt-in live mode reads the installed app configuration, starts the
app-managed runtime when needed, translates every non-filter case, shuts down
the exact runtime it started, and emits a privacy-safe JSON report containing
case IDs, output shape, validator result, rejection reason, and latency:

```powershell
.\scripts\run-subtitle-eval.ps1 -Live -Runs 3 `
  -ReportPath .\eval-results\arm64.json
```

Use `-ConfigPath` when the app configuration is not at the default
`%APPDATA%\com.meowcal.sub\config.json` path. Do not commit reports containing
machine-specific paths or private test material. Live wording is intentionally
not compared to one exact reference sentence; grading uses product invariants
and leaves semantic quality to the named manual episode gate.

The live release budgets are p50 at or below 1.5 seconds and p95 at or below
3 seconds. A report is successful only when every translated attempt passes
the production validator and both latency budgets pass.
