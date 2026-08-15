# #34 Wave 3 — overlay timer and appearance-state owners

Date: 2026-08-15
Issue: #34 (Extract overlay and capture-selection frontend boundaries)
Base: `5cb4e31df5a51cfd18c3656a4176cf78c2525c83`

## Problem

Waves 1 and 2 closed the geometry half of #34. Two ownership gaps remained, both
in `overlay.js`.

**Timers.** The overlay runs three timers whose lifetime is overlay *state*, not
page lifetime: the click-through poll, the capture-frame fade, and the post-fade
hide cleanup. Two were loose module variables cleared at their call sites. The
third — the hide cleanup armed on `overlay-visibility: false` — was not tracked
at all:

```js
setTimeout(() => {
    captureFrame.classList.add('hidden');
    captureFrame.classList.remove('visible', 'exiting', 'faded');
    ...
}, OVERLAY_VISIBILITY_FADE_MS);   // 220ms, no handle kept
```

Stop then Start inside that 220ms window leaves the timer armed. The show path
re-adds `visible`; the stale cleanup then fires and re-adds `hidden`. Nothing
else adds `visible` back, so the capture frame stays invisible for the rest of
the session. Subtitles recover on the next translation update; the frame does
not.

**Appearance state.** Two paths write the overlay's font, colour, and toggles —
`get_settings` on startup and the `overlay-settings-updated` event — and each
open-coded its own guard chain in `overlay.js`. They had already drifted: the
loader coerced with `||`, the patch checked `typeof`, and neither was stated
anywhere as the rule.

## Change

New `src/scripts/overlay-timers.js` — a named-slot timer owner. One slot per
name; scheduling into an occupied slot replaces what was there, cancelling is
idempotent, and a fired timeout clears its own slot so `isPending` cannot report
a dead timer. The clock is injectable so the rules are testable without waiting.

`overlay.js` now names its three slots (`clickThrough`, `frameFade`,
`hideCleanup`), and the show path cancels `hideCleanup`.

New `src/scripts/overlay-appearance.js` — the owner of appearance state and its
defaults:

- `hydrateAppearance(settings)` — startup: falsy values take the default, the two
  toggles are strictly boolean so an absent field cannot read as "on";
- `patchAppearance(current, payload)` — live update: takes only fields the
  payload carries with the type each is defined as, and reports which it took so
  the adapter syncs exactly the controls that changed.

`overlayState` now seeds its appearance fields from `DEFAULT_APPEARANCE` instead
of repeating the literals.

## Behavior

One deliberate, named change: **a Stop→Start inside the 220ms fade window no
longer leaves the capture frame hidden.** This is the direct consequence of
giving the hide-cleanup timer an owner — a timer owner that never cancels is not
an owner — and it is called out for the owner's `Start/Stop and second
Start/Stop` manual matrix item rather than folded in silently.

Everything else is preserved:

- `startClickThroughMonitor` keeps its "do not restart if already running" guard
  via `isPending`, so the poll phase is not reset;
- `scheduleFadeOut` still cancels first and still returns early when there is no
  capture frame;
- the settings patch keeps the original apply order (state, then the font-size
  controls, then styles, then reposition, then diagnostics);
- cadences are unchanged (150ms poll, 2000ms stall guard, 4000ms fade, 220ms
  hide) — they are now named constants rather than literals.

**#113 is untouched.** `overlay-liveness.js`, `overlay-ready`,
`overlay-heartbeat`, staleness detection, recovery, and clip reset are not
modified. The liveness heartbeat is deliberately *not* moved into the timer
owner: it is page-scoped by design (it must beat for as long as the renderer
lives, and a wedged renderer stopping it is the signal), so putting it in a
cancellable slot would create a way to silence the very detector #113 added.

## Ratchets

| File | Before | After |
| --- | ---: | ---: |
| `src/scripts/overlay.js` | 1,073 | 1,062 |

`docs/MAINTAINABILITY_BASELINE.md` still said "10 existing ESLint warnings"
after Wave 2 lowered the manifest to 8. Corrected here.

## Tests

- `frontend-tests/unit/overlay-timers.test.mjs` — pending/cancel reporting,
  idempotent cancel, cancelled callbacks never running, slot replacement not
  leaking the previous timer, a fired timeout clearing its own slot, intervals
  surviving ticks, slot independence, `cancelAll`, and the Stop→Start sequence
  that motivated the module.
- `frontend-tests/unit/overlay-appearance.test.mjs` — defaults for empty/absent
  settings, stored values winning, falsy size/colour treated as absent, strictly
  boolean toggles, no mutation of the shared defaults or of the current state,
  empty and wrong-typed patches applying nothing, and `applied` reporting.

New modules are not added to the coverage include list; #35 owns that.

## Not in scope

Page-lifetime listeners (the five Tauri `event.listen` registrations and the two
document mouse listeners) keep no disposer. The overlay page is destroyed as a
whole on recovery or quit, so a disposer nobody calls would be a fake owner, not
an explicit one. This is recorded as the deliberate answer to #34's cleanup
criterion rather than left implicit.
