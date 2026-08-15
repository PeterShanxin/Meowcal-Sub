# #34 Wave 2 — capture-region and overlay geometry owners

Date: 2026-08-15
Issue: #34 (Extract overlay and capture-selection frontend boundaries)
Base: `c9a49dbdc378ddb0408245a620b129ab8ee7c9c9`

## Problem

After Wave 1 the selector had a geometry owner, but the overlay did not. Three
concrete consequences:

1. **Two owners for one rule.** `selector-geometry.js` and `overlay.js` each
   carried their own capture-region move/resize arithmetic. The clamping rule is
   identical apart from the minimum size, so a fix applied to one window would
   silently leave the other wrong.
2. **Untestable viewer-visible math.** Frame border rounding at fractional DPI,
   subtitle plate placement, and the rounding that feeds the Win32 clip payload
   were inline in DOM-writing functions. None of it could be characterized
   without a WebView, which is exactly the geometry the owner has to regress by
   hand on real hardware.
3. **A rebound handler.** `selector.js` declared `handleMouseMove`/`handleMouseUp`
   and then reassigned both at module scope, so the file had two apparent owners
   for each document mouse event and only the second one ever ran.

## Change

New `src/scripts/region-geometry.js` — the single owner of capture-region
`moveRegion`/`resizeRegion`, used by both windows. The minimum size is a required
argument because the surfaces genuinely differ (selector 30px, overlay 50px);
neither default belongs in the shared rule.

New `src/scripts/overlay-geometry.js` — the overlay's own pure geometry:

- `frameScaleTokens` — device-pixel snapping for the frame ring at 100/125/150/200%;
- `subtitleHeightEstimate` / `resolveSubtitlePlacement` — below/above/below-fallback
  placement of the subtitle plate;
- `roundedRectBounds` — the integral rectangle the Win32 clip needs.

`overlay.js` gains `applyRegionToOverlay`, one owner for "the capture region
moved": frame, subtitle plate, clip, and diagnostics readout are four views of
one rectangle and previously drifted apart across four copies. `initOverlay`
deliberately does **not** call it — it paints before the window is shown, and
issuing a clip against a hidden window is the stale-clip state #113 exists to
detect.

`selector.js` folds the rebound handlers back into single `handleMouseMove` and
`handleMouseUp` owners that branch on drag / resize / draw.

## Behavior

Preserved. Every extracted function is the previous expression, moved:

- resize clamping keeps the `handle.includes(...)` accumulation and the
  pin-the-opposite-edge minimum; the selector's explicit 8-branch chain produced
  identical output for all eight handles, and a test pins each one;
- subtitle placement keeps `below → above → below` order and the
  `round(fontSize * 1.4 + 54)` estimate;
- the mouse-mode fold preserves the rebound handlers' order (drag, then resize,
  then draw), which is what actually ran before.

No #113 signal changed: `overlay-liveness.js`, `overlay-ready`,
`overlay-heartbeat`, staleness detection, recovery, and clip reset are untouched.

## Ratchets

| File                      | Before | After |
| ------------------------- | -----: | ----: |
| `src/scripts/overlay.js`  |  1,129 | 1,073 |
| `src/scripts/selector.js` |    722 |   721 |

`docs/MAINTAINABILITY_BASELINE.md` also had three stale ceilings in its
narrative table (`foundry_local.rs` 1,700, `commands.rs` 1,209,
`manager.rs` 1,021) left over from earlier lanes. The machine-readable manifest
was already correct; the table now matches it rather than leaving known-false
lines standing.

## Tests

- `frontend-tests/unit/region-geometry.test.mjs` — move, all eight resize
  handles against both minimums, the pinned-opposite-edge invariant, and
  immutability of the source region.
- `frontend-tests/unit/overlay-geometry.test.mjs` — DPI token snapping and
  invalid scale-factor fallback, plate placement below/above/neither, the
  exactly-fitting boundary, measured-height override, and clip rounding
  including negative offsets from a monitor left of the primary.
- `frontend-tests/unit/selector-geometry.test.mjs` — the moved cases removed,
  the rest unchanged.

New modules are not added to the coverage include list: #35 owns risk-based
coverage expansion, and widening the include list from this lane would move the
measured floors under every other in-flight branch.

## Not in scope

Overlay timer and listener lifecycle ownership (click-through monitor, fade
timer, clip settle loop) is Wave 3. Product behavior, OCR/translation, #35,
#75, #107, and #114 are out of this lane entirely.
