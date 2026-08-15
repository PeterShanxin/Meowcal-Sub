# Issue #34 Wave 1: Selector pure geometry

## Decision

Extract the selector's pure geometry decisions into one small, DOM-free module.
The audit selected this boundary from current `main` commit
`d469b3355b12fb5c18716ec78714e3025f4253f2`.

The module owns calculations only. `selector.js` remains the DOM, event,
Tauri, persistence, and lifecycle adapter.

## Included responsibilities

- normalize two screen points into a selection rectangle;
- convert a screen rectangle to the selector window's client coordinates;
- clamp a selection hole to the viewport and describe the four dim segments;
- compute the padded, window-clamped capture payload;
- preserve the capture payload's scale-factor value without applying a second
  DPI conversion;
- calculate drag and resize results, including the existing 30-pixel resize
  minimum.

The public API must remain pure: no DOM, Tauri bridge, timers, listeners, or
mutable module state.

## Preserved behavior

- Selection uses `MouseEvent.screenX`/`screenY`.
- Client presentation subtracts `window.screenX`/`screenY`.
- Initial selection validation remains 30 pixels wide and 15 pixels high.
- Resize validation remains 30 pixels in both dimensions.
- Confirmation keeps 8-pixel horizontal and 10-pixel vertical padding.
- Confirmation clamps to selector window screen bounds, then applies the
  existing non-negative `x`/`y` clamp and minimum 1-pixel payload extents.
- Fractional `scaleFactor` values pass through unchanged in the payload.
- Existing-region preload, drag/resize presentation, snapshot behavior,
  confirm timing, close/hide fallback, and event names remain unchanged.

## Test characterization

Add deterministic unit coverage for:

- forward and reverse selection drags, including zero extents;
- non-zero and negative window origins;
- viewport-edge and out-of-viewport dim-hole clamping;
- padding clamping at window bounds and the non-negative capture clamp;
- fractional scale-factor payload preservation;
- drag and all resize-handle directions at and below the minimum size.

Tests assert geometry and invariants, not DOM implementation details.

## Explicit non-goals

- no overlay extraction;
- no click-through, clipping, or overlay liveness/recovery changes;
- no listener or timer lifecycle refactor in this wave;
- no Rust, IPC, capture, translation, #33, #35, or #114 changes;
- no visible interaction or copy changes.

Listener/timer cleanup remains a separately mapped #34 wave because the audit
found ambiguity but no repeated-registration failure that should be bundled
with this pure geometry cut.

## Ratchet and native gate

Lower only `src/scripts/selector.js` to its measured post-extraction line
count. Keep `src/scripts/overlay.js` at 1129. Prove the selector ceiling fails
one line below the measured result, then restore and pass the maintainability
check.

The named native regression remains deferred to the #34 closeout owner gate:
mixed DPI, drag, resize, click-through, clipping, and region persistence.
Browser/unit tests do not replace that evidence.
