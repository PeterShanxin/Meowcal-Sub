/* global module */

// Physical-pixel hit bounds for the smart click-through monitor.
//
// The monitor compares the OS cursor position (device pixels, screen space)
// against overlay surfaces measured in CSS pixels (window space), so every rect
// needs the window origin added before scaling.
(function exposeOverlayHitBounds(root) {
  function rectToPhysicalBounds(rect, origin, scaleFactor) {
    const offsetX = origin?.x || 0;
    const offsetY = origin?.y || 0;

    return {
      left: (rect.left + offsetX) * scaleFactor,
      top: (rect.top + offsetY) * scaleFactor,
      right: (rect.right + offsetX) * scaleFactor,
      bottom: (rect.bottom + offsetY) * scaleFactor,
    };
  }

  function regionToPhysicalBounds(region, padding, origin, scaleFactor) {
    return rectToPhysicalBounds(
      {
        left: region.x - padding,
        top: region.y - padding,
        right: region.x + region.width + padding,
        bottom: region.y + region.height + padding,
      },
      origin,
      scaleFactor,
    );
  }

  function pointInBounds(point, bounds) {
    return (
      point.x >= bounds.left &&
      point.x <= bounds.right &&
      point.y >= bounds.top &&
      point.y <= bounds.bottom
    );
  }

  const api = { pointInBounds, rectToPhysicalBounds, regionToPhysicalBounds };
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.OverlayHitBounds = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this);
