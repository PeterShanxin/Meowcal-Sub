# Curated redesign UI QA

## Visual source

- Direction source: the seven 1448 x 1086 references in `D:\Downloads\ref`.
- Selected direction: the restrained dark cinematic shell, compact bottom navigation, guided
  four-step setup, and an app-managed private translation engine.
- Real app assets are used for the Meowcal icon and cinematic preview background. The interface
  uses the checked-in Phosphor subset rather than text symbols or improvised drawings.

## Native verification

- Runtime: Tauri 2 / WebView2 on Windows ARM64.
- Display: 200% DPI (`GetDpiForSystem = 192`).
- Reference viewport: 1448 x 1086 physical pixels.
- User-reported viewport: approximately 1364 x 1060 physical pixels.
- Capture method: Win32 `PrintWindow` against the live native windows while the desktop was
  locked; window resize/repaint was used only to force WebView2 to composite the current state.
- Home comparison: `artifacts/qa-home-ready-comparison-2.png`.
- Setup comparison: `artifacts/qa-wizard-comparison.png`.
- Final native states: `artifacts/tauri-home-startup-gate.png`,
  `artifacts/tauri-appearance-repaint.png`, `artifacts/tauri-settings.png`,
  `artifacts/tauri-wizard-fit.png`, and `artifacts/tauri-wizard-step2-fit.png`.

## Iterations completed

- Removed the duplicate in-page brand row because the native title bar already carries identity.
- Restored readable contrast on the primary action.
- Replaced raw capture dimensions with the user-facing `Area selected` state.
- Reduced default high-DPI window sizing and removed the Home scrollbar.
- Added a startup readiness gate so the UI cannot render Rust defaults before persisted settings
  load; verified the real Chinese-to-English pair after a native restart.
- Fixed the setup grid's missing error-slot row, which left a blank footer-sized strip and forced
  unnecessary scrolling.
- Compacted setup at short window heights and kept the language pair side by side so steps 1 and
  2 show all required content and actions without scrolling at the reported viewport.
- Removed the misleading focus ring from programmatically focused step headings while preserving
  focus announcements for assistive technology.
- Bound setup select options explicitly so the saved English target cannot visually fall back to
  the first option.
- Kept technical setup details out of the normal progress state; they appear only with an error.

## Coverage

- Home ready state: native capture, matched against reference 2.
- Overlay appearance: native navigation and capture.
- Settings: native navigation and capture.
- Setup steps 1 and 2: native navigation, persisted language values, no-scroll capture, and matched
  step 1 against reference 4.
- Setup progress: native action reached the real progress screen. The model/runtime path was also
  verified independently by the live subtitle eval.
- Browser smoke was intentionally not run because no browser preference has been selected; native
  Tauri verification is the primary acceptance path for this desktop UI.

final result: passed
