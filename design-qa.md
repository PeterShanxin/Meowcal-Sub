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

## Product site: cinematic hero and open content layout

The [Raycast homepage](https://www.raycast.com/) captured on 2026-11-14 is the
visual reference: inset dark navigation, centered white display type, and red
optical lighting. This is a visual direction, not a clone of its content.

- Desktop implementation: [Chinese hero](docs/design/product-site/desktop-zh.png),
  compared with the reference at 1265 × 712 image pixels in the same comparison
  input. Both use the default browser viewport; no density normalization.
- Mobile implementation: [Chinese page](docs/design/product-site/mobile-zh.png),
  captured at 390 × 844 CSS pixels. All four routes were also checked at 320
  and 768 CSS pixels with no horizontal overflow.
- Typography: system fonts preserve Chinese coverage; large white propositions
  wrap without clipping. The smaller product name remains part of the H1.
- Spacing: compact paired hero actions, a full-width framed screenshot, and
  installer choices below it. Mobile navigation wraps into two rows.
- Color and assets: near-black surfaces and an original red-light WebP backdrop.
  The backdrop is darker than the reference for text contrast. Product images
  retain their proportions and are separate from the decorative background.
- Content: original capabilities, limits and release links remain. Footer prose is
  removed; licensing terms live in the repositories linked by the footer.
  Titles, canonical URLs, language alternates and structured data are retained.
- Focused checks: x64/ARM64 installer labels, three illustrated setup steps,
  lazy-image loading after scrolling, language/product navigation, download and
  setup anchors, and the keyboard skip link. Browser warning/error logs are empty.

The content layout uses typography, alignment and whitespace rather than cards.
Setup screenshots alternate with the copy on desktop and follow the copy on
mobile. Privacy and requirements use open text columns. The callout, related
products and footer have no enclosing panels or decorative heading rules.

- Compared the Chinese setup section at parent commit `9932f80` with the new
  [desktop setup layout](docs/design/product-site/steps-desktop.png) in one input,
  both captured at 1265 × 712 image pixels in the default browser viewport.
- Reviewed [privacy and requirements](docs/design/product-site/details-desktop.png),
  the English privacy layout, the unillustrated Sub 2 steps, and the link-only footer.
- Reviewed [mobile setup](docs/design/product-site/steps-mobile.png) at 390 × 844
  CSS pixels; all four routes also pass overflow checks at 320 and 768 CSS pixels.
- All four rendered footers contain four links and no prose paragraphs. Existing
  title, description, canonical, language alternates, H1 and JSON-LD checks pass.

No actionable P0/P1/P2 visual issues remain in the inspected views. No native app
behavior is covered by this website check.

### Motion

- The hero light settles once in 4.5 seconds; title movement ends in 900 ms.
  Buttons have a 650 ms light pass on pointer hover or keyboard focus. Nothing
  loops while the page is idle.
- Scroll-linked entrances use only opacity and transforms. Text starts at 80%
  opacity; screenshots rise by at most 24 px and scale from 0.985 to 1.
- In the Windows in-app browser, observed an entering screenshot at opacity
  0.751 and a 17.1 px offset, then opacity 1 and a zero offset once in view.
  The [settled desktop view](docs/design/product-site/motion-desktop.png) and
  390 px mobile view retain the open layout without clipping.
- Keyboard focus reaches the download action, keeps a solid outline and triggers
  the light pass. All four routes fit at 320 px; browser warnings/errors are empty.
- CSSOM inspection confirms every animation and transition rule is inside
  `prefers-reduced-motion: no-preference`. The existing reduced-motion rule
  disables smooth scrolling. This checks the stylesheet gates; it does not
  change the user's OS preference. Browsers without view timelines use the
  static layout through the feature query.
- Motion follows the platform's [view timeline](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-timeline/view)
  and [reduced-motion preference](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/prefers-reduced-motion).
  Page content, metadata, downloads and the no-JavaScript contract are unchanged.

final result: passed
