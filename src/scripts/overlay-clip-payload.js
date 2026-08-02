/* global module */

// Equality for the Win32 window-region payload.
//
// `set_overlay_window_clip` is a synchronous Tauri command: it runs GDI region
// work on the same main thread that serves capture, OCR, and click-through IPC.
// The clip settle loop ticks once per animation frame, so resending an identical
// payload ~60 times a second starves that thread - the overlay goes stale and the
// click-through monitor stops being able to re-enable interaction.
//
// Sending only on a real geometry change keeps the settle loop cheap.
(function exposeOverlayClipPayload(root) {
  function rectEquals(a, b) {
    if (a === b) return true;
    if (!a || !b) return false;
    return a.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height;
  }

  function rectListEquals(a, b) {
    if (a === b) return true;
    if (!a || !b || a.length !== b.length) return false;
    return a.every((rect, index) => rectEquals(rect, b[index]));
  }

  function numberListEquals(a, b) {
    if (a === b) return true;
    if (!a || !b || a.length !== b.length) return false;
    return a.every((value, index) => value === b[index]);
  }

  function clipPayloadEquals(a, b) {
    if (a === b) return true;
    if (!a || !b) return false;
    return (
      a.scaleFactor === b.scaleFactor &&
      rectEquals(a.frameRegion, b.frameRegion) &&
      rectEquals(a.subtitleBounds, b.subtitleBounds) &&
      rectListEquals(a.handleBounds, b.handleBounds) &&
      numberListEquals(a.controlRadii, b.controlRadii)
    );
  }

  const api = { clipPayloadEquals };
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.OverlayClipPayload = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this);
