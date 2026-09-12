# Curated redesign UI QA

## Visual source

- Structure and flow: the seven 1448 x 1086 references in
  `docs/design/curated-redesign/reference/`.
- Visual language: Obsidian Ceramic, locked in #72. A near-black ground, glass
  panels with hairline borders, one white ceramic primary action per screen, and
  an ice-white accent. Colour is reserved for success, warning, and danger.
- One token set, `src/styles/tokens.css`, is shared by the main window, setup,
  area selector, and subtitle overlay. Icons are the inline SVG set in
  `src/ui/icons.ts`.

## Native verification

- Tested commit: `821b614`, run through `dev-tauri.cmd` in the isolated
  `com.meowcal.sub.dev` profile. The selector checks ran earlier in the same
  session on `e5fac60`; the selector files are unchanged since.
- Hardware: Snapdragon X Elite X1E80100, Windows 11 25H2 build 26200.9445,
  ARM64, 2560 x 1440 at 125% (120 DPI).
- Captures: Win32 `PrintWindow` of the app's own windows, in
  `docs/design/curated-redesign/qa/`. The selector and overlay were checked on
  screen but not captured, because both windows cover the rest of the desktop.
- Escape was checked with a raw key event. Desktop automation that reserves Esc
  as its own stop key cannot show whether the selector handles it.

| Scenario | Result | Capture |
| --- | --- | --- |
| First launch opens setup; Continue, Back, and Cancel work; Cancel closes it | Pass | `setup-welcome.png`, `setup-languages.png` |
| Returning launch after closing setup once: setup stays closed, Home is ready, text size and plate persist | Pass | `home-ready.png`, `subtitle-style-light.png` |
| Home: Ready, Start, Running, Stop, Ready again; the overlay hides on Stop | Pass | `home-ready.png`, `home-running.png` |
| Subtitle style: text size by keyboard, Dark and Light plates, preview at the overlay's opacity | Pass | `subtitle-style-light.png`, `subtitle-style-dark.png` |
| Overlay: hovering the frame edge shows the handles and settings button; the quick menu shows text size and background; switching to Dark changes the live plate; the main window picks the value up on focus | Pass | `subtitle-style-dark.png` |
| Area selector: opens on the saved area; arrow keys move, Shift+arrows resize; Esc closes it without saving | Pass | none |
| Settings: plain-language rows and pill switches; Repair opens setup | Pass | `settings.png` |
| Minimum window, 560 x 430: Home and Subtitle style fit without scrolling; Settings scrolls | Pass | `min-home.png`, `min-subtitle-style.png`, `min-settings.png` |

## Known limits

- The overlay window is drawn at a fixed alpha of 200/255
  (`src-tauri/src/overlay/window_alpha.rs`), so the quick menu and both plates
  show some of what is behind them. The Subtitle style preview renders at the
  same alpha.
- Not covered by this pass: x64, a real engine download and its failure path
  (stage-specific failure copy is covered by unit tests), and a 30-minute episode
  run.
