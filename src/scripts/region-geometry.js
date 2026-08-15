/* global module */

// Pure capture-region rectangle math shared by the selector and the overlay.
//
// Both windows let the user move and resize the same logical capture region,
// and both used to carry their own copy of the drag/resize arithmetic. Two
// copies of a clamping rule is one copy too many: a fix applied to the selector
// silently left the overlay wrong. This module is the single owner of that rule.
//
// Coordinates are logical screen pixels in the caller's own space; nothing here
// reads the DOM, a window origin, or a scale factor. The minimum size differs
// per surface (the selector accepts a smaller rectangle than the overlay frame),
// so it is a required argument rather than a default that happens to suit one
// caller.
(function exposeRegionGeometry(root) {
  function moveRegion(region, deltaX, deltaY) {
    return {
      x: region.x + deltaX,
      y: region.y + deltaY,
      width: region.width,
      height: region.height,
    };
  }

  function resizeRegion(region, handle, deltaX, deltaY, minSize) {
    let x = region.x;
    let y = region.y;
    let width = region.width;
    let height = region.height;

    if (handle.includes("w")) {
      x = region.x + deltaX;
      width = region.width - deltaX;
    }
    if (handle.includes("e")) {
      width = region.width + deltaX;
    }
    if (handle.includes("n")) {
      y = region.y + deltaY;
      height = region.height - deltaY;
    }
    if (handle.includes("s")) {
      height = region.height + deltaY;
    }

    // Clamping pins the edge the user is not dragging, so a west or north drag
    // past the minimum keeps the opposite edge where it was.
    if (width < minSize) {
      if (handle.includes("w")) x = region.x + region.width - minSize;
      width = minSize;
    }
    if (height < minSize) {
      if (handle.includes("n")) y = region.y + region.height - minSize;
      height = minSize;
    }

    return { x, y, width, height };
  }

  const api = { moveRegion, resizeRegion };
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.RegionGeometry = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this);
