/* global module */

// Pure geometry for the legacy capture selector.
//
// The selector adapter owns DOM state, event handlers, Tauri calls, and
// persistence. This module owns only the coordinate and rectangle decisions so
// they can be characterized without a WebView or a native window.
//
// Move and resize are not here: the overlay applies the same rules to the same
// capture region, so `region-geometry.js` owns them for both windows.
(function exposeSelectorGeometry(root, factory) {
  const api = factory();

  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }

  if (root) {
    root.SelectorGeometry = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this, () => {
  function selectionRectFromPoints(startX, startY, currentX, currentY) {
    return {
      x: Math.min(startX, currentX),
      y: Math.min(startY, currentY),
      width: Math.abs(currentX - startX),
      height: Math.abs(currentY - startY),
    };
  }

  function screenRectToClientRect(rect, origin) {
    const originX = origin?.x || 0;
    const originY = origin?.y || 0;

    return {
      left: rect.x - originX,
      top: rect.y - originY,
      width: rect.width,
      height: rect.height,
    };
  }

  function clampSelectionHole(rect, viewport) {
    const clampedLeft = Math.max(0, Math.min(rect.x, viewport.width));
    const clampedTop = Math.max(0, Math.min(rect.y, viewport.height));
    const clampedRight = Math.max(clampedLeft, Math.min(rect.x + rect.width, viewport.width));
    const clampedBottom = Math.max(clampedTop, Math.min(rect.y + rect.height, viewport.height));

    return {
      left: clampedLeft,
      top: clampedTop,
      width: Math.max(0, clampedRight - clampedLeft),
      height: Math.max(0, clampedBottom - clampedTop),
    };
  }

  function buildDimOverlaySegments(rect, viewport) {
    const hole = clampSelectionHole(rect, viewport);
    const bottomTop = hole.top + hole.height;
    const rightLeft = hole.left + hole.width;

    return {
      top: { top: 0, left: 0, width: viewport.width, height: hole.top },
      bottom: {
        top: bottomTop,
        left: 0,
        width: viewport.width,
        height: Math.max(0, viewport.height - bottomTop),
      },
      left: { top: hole.top, left: 0, width: hole.left, height: hole.height },
      right: {
        top: hole.top,
        left: rightLeft,
        width: Math.max(0, viewport.width - rightLeft),
        height: hole.height,
      },
    };
  }

  function buildCaptureRegionPayload(region, windowBounds, scaleFactor) {
    const rawX = Math.round(region.x);
    const rawY = Math.round(region.y);
    const rawWidth = Math.round(region.width);
    const rawHeight = Math.round(region.height);

    let x1 = rawX;
    let y1 = rawY;
    let x2 = rawX + Math.max(1, rawWidth);
    let y2 = rawY + Math.max(1, rawHeight);

    x1 -= 8;
    y1 -= 10;
    x2 += 8;
    y2 += 10;

    // Keep the existing selector-window clamp and the non-negative capture
    // clamp in this order. The backend expects this logical screen payload.
    x1 = Math.max(windowBounds.left, x1);
    y1 = Math.max(windowBounds.top, y1);
    x2 = Math.min(windowBounds.right, x2);
    y2 = Math.min(windowBounds.bottom, y2);

    x1 = Math.max(0, x1);
    y1 = Math.max(0, y1);

    return {
      x: x1,
      y: y1,
      width: Math.max(1, x2 - x1),
      height: Math.max(1, y2 - y1),
      scaleFactor,
    };
  }

  function meetsMinimumSelection(region, minWidth = 30, minHeight = 15) {
    return region.width >= minWidth && region.height >= minHeight;
  }

  const ARROW_DIRECTIONS = {
    ArrowLeft: [-1, 0],
    ArrowRight: [1, 0],
    ArrowUp: [0, -1],
    ArrowDown: [0, 1],
  };

  // One pixel per press, or ten with Ctrl, so the keyboard can both line a box
  // up exactly and cross a screen without hundreds of presses.
  function arrowDelta(key, fast) {
    const direction = ARROW_DIRECTIONS[key];
    if (!direction) return null;
    const step = fast ? 10 : 1;
    return { dx: direction[0] * step, dy: direction[1] * step };
  }

  // The box a keyboard user starts from: centred across the lower part of the
  // screen, where subtitles usually sit. `viewport` is in screen coordinates.
  function defaultSelectionRect(viewport) {
    const width = Math.round(viewport.width * 0.6);
    const height = Math.max(40, Math.round(viewport.height * 0.1));
    return {
      x: viewport.x + Math.round((viewport.width - width) / 2),
      y: viewport.y + Math.round(viewport.height * 0.8 - height / 2),
      width,
      height,
    };
  }

  // Below the box when the buttons fit under it, otherwise above it, and never
  // past the top edge. Subtitle boxes sit low on screen, so "above" is common.
  function actionBarTop(box, viewportHeight, barHeight, gap) {
    const below = box.top + box.height + gap;
    if (below + barHeight <= viewportHeight) return below;
    return Math.max(gap, box.top - gap - barHeight);
  }

  // Keyboard adjustments stay on the selector's screen: a box carried past an
  // edge can no longer be seen or adjusted, and saving it collapses the capture
  // area to a sliver. `viewport` is in screen coordinates.
  function containRegion(region, viewport) {
    const width = Math.min(region.width, viewport.width);
    const height = Math.min(region.height, viewport.height);
    return {
      x: Math.min(Math.max(region.x, viewport.x), viewport.x + viewport.width - width),
      y: Math.min(Math.max(region.y, viewport.y), viewport.y + viewport.height - height),
      width,
      height,
    };
  }

  function regionFits(region, viewport) {
    return (
      region.x >= viewport.x &&
      region.y >= viewport.y &&
      region.x + region.width <= viewport.x + viewport.width &&
      region.y + region.height <= viewport.y + viewport.height
    );
  }

  return {
    actionBarTop,
    arrowDelta,
    buildCaptureRegionPayload,
    buildDimOverlaySegments,
    clampSelectionHole,
    containRegion,
    defaultSelectionRect,
    meetsMinimumSelection,
    regionFits,
    screenRectToClientRect,
    selectionRectFromPoints,
  };
});
