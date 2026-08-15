/* global module */

// Pure geometry for the overlay window.
//
// The overlay adapter owns the DOM, Tauri IPC, and timers. This module owns the
// arithmetic behind what the viewer actually sees - how thick the capture frame
// border is at fractional DPI, where the subtitle plate lands relative to the
// capture region, and what rectangle the Win32 clip is told about - so those
// decisions can be characterized without a WebView, a monitor, or a scale
// factor supplied by Windows.
//
// Shared capture-region move/resize rules live in `region-geometry.js`, which
// the selector uses too.
(function exposeOverlayGeometry(root) {
  // The frame border is drawn in CSS pixels but should land on whole device
  // pixels, or a 125% display renders a blurred, uneven ring. Rounding to
  // physical pixels first and dividing back is what keeps the edge crisp.
  function frameScaleTokens(scaleFactor) {
    const sf = typeof scaleFactor === "number" && scaleFactor > 0 ? scaleFactor : 1;
    const borderPx = Math.max(1, Math.round(2 * sf));
    const radiusPx = Math.max(0, Math.round(8 * sf));

    return { borderCss: borderPx / sf, radiusCss: radiusPx / sf };
  }

  // A hidden plate cannot be measured, so fall back to the height its font size
  // implies. The constants match the plate's padding and hint row.
  function subtitleHeightEstimate(measuredHeight, fontSize) {
    return measuredHeight > 0 ? measuredHeight : Math.round(fontSize * 1.4 + 54);
  }

  // Below the capture region if it fits, above if only that fits, and below
  // again when neither does - a plate pushed off the bottom edge is still
  // better than one covering the subtitles being captured.
  function resolveSubtitlePlacement(
    region,
    viewportHeight,
    measuredHeight,
    fontSize,
    padding = 10,
  ) {
    const height = subtitleHeightEstimate(measuredHeight, fontSize);
    const below = region.y + region.height + padding;
    const spaceBelow = viewportHeight - below;
    const spaceAbove = region.y - padding;

    let top = below;
    if (spaceBelow < height && spaceAbove >= height) {
      top = region.y - padding - height;
    }

    return { left: region.x, top, width: region.width, height };
  }

  // Win32 window regions are integral, and a DOMRect is not. Round once, here,
  // so the clip payload comparison sees a stable value instead of sub-pixel
  // jitter that would resend an identical clip every animation frame.
  function roundedRectBounds(rect) {
    return {
      x: Math.round(rect.left),
      y: Math.round(rect.top),
      width: Math.round(rect.width),
      height: Math.round(rect.height),
    };
  }

  const api = {
    frameScaleTokens,
    resolveSubtitlePlacement,
    roundedRectBounds,
    subtitleHeightEstimate,
  };
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.OverlayGeometry = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this);
