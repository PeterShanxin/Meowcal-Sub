# UI audit captures, 2026-09-26

Before and after screenshots for the shell-window fixes on
`claude/ui-ux-audit-20260926`. `before-*` is `main` at `e63fe10`; an `after-*`
without a `before-*` twin rendered identically on `main`.

## Method

- `capture.mjs` drives the main window and the setup window through the
  journeys below in Chromium, served by `npm run dev:browser`.
- `tauri-shim.js` stands in for the Tauri runtime with scripted command answers,
  so the pages run their Tauri code paths. This is presentation and flow
  evidence only. It does not exercise Windows capture, OCR, the selector or
  overlay windows, the tray, DPI, or the real engine, and it does not satisfy
  the manual Windows gate in `docs/AGENT_GUIDE.md`.
- Sizes: 680 x 500 (default window), 560 x 430 (minimum window), 1440 x 900
  (maximised). 390 x 844 is recorded only to show that phone widths are below
  the window minimum and clip.

```text
npm run dev:browser
node docs/audit-20260926/ui/capture.mjs http://127.0.0.1:3000 after
```

Set `MEOWCAL_AUDIT_CHROMIUM` to a Chromium executable when Playwright's own
browser is not installed.

## Journeys and results

| Journey | Check | `main` | Branch |
| --- | --- | --- | --- |
| J1 setup | Welcome to "Ready to watch" at 680 and 560 | Pass | Pass |
| J1 setup | Failed download fits 560 x 430 without scrolling | Fail | Pass |
| J2 Home | Select area, start, running, stop, reload at three sizes | Pass | Pass |
| J2 Home | Language and area locked while starting | Fail | Pass |
| J3 Home | Change area, then cancel: no "selected" message | Fail | Pass |
| J4 Subtitle style | Text size by keyboard, Light plate | Pass | Pass |
| J5 Settings | Progress message still shown 4.8 s into a 6 s test | Fail | Pass |
| J5 Settings | Last row clear of a message at the end of the page | Fail | Pass |
| Browser mode | No dead window controls | Fail | Pass |
