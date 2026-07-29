# CLAUDE.md

This repository's canonical working contract is
[`docs/AGENT_GUIDE.md`](docs/AGENT_GUIDE.md). Read it completely before making
changes, then read [`docs/CODING_STANDARDS.md`](docs/CODING_STANDARDS.md) and
the relevant accepted ADRs.

## Product

Meowcal Sub is a Windows Tauri 2/Rust application for private local subtitle
translation. The approved direction keeps Windows OCR and makes Tencent HY-MT
the only supported normal-mode translation engine. Generic endpoints belong
only in disabled-by-default developer mode.

Do not implement the archived Python/OpenSubtitles MeoCoSub2 plan. Do not merge
`feat/hymt-foundry` wholesale; selectively migrate reviewed pieces.

## Current commands

From the repository root:

```powershell
.\scripts\verify.ps1
```

Focused iteration may use `-Stage Lint`, `-Stage Test`, or `-Stage Frontend`.
The default gate includes the real browser-to-Rust health/settings smoke.

Use `.\dev-browser.cmd` for browser-only UI work and `.\dev-tauri.cmd` for the
current ARM64 Tauri development flow. Browser mode does not prove Windows OCR,
capture, selector, overlay, tray, installer, or DPI/window behavior.

## Non-negotiable rules

- Work in an isolated worktree and preserve unrelated changes.
- Keep visible behavior separate from structural refactors.
- Require fresh manual Windows evidence after visible behavior changes.
- Do not claim performance improvement without before/after measurements.
- Never present raw OCR as successful translation.
- Never infer merge or external-write permission.
