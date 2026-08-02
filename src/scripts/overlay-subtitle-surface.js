/* global module */

// Decides what the overlay subtitle box should show for a pipeline update.
//
// The overlay box is the only place a viewer can learn that the pipeline is
// alive but not producing translations yet. Hint-only states must therefore
// keep the box on screen; otherwise a warming, unavailable, or source-only
// pipeline is indistinguishable from a dead one.
(function exposeOverlaySubtitleSurface(root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.OverlaySubtitleSurface = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this, () => {
  function nonEmpty(value) {
    return typeof value === "string" && value.trim() !== "";
  }

  /**
   * @param {{ replaceText?: boolean, clearText?: boolean, hint?: string }} presentation
   * @param {string} translated
   * @returns {{ mode: "text" | "hint" | "clear", showContainer: boolean }}
   */
  function resolveSubtitleSurface(presentation, translated) {
    if (presentation?.replaceText) {
      return { mode: "text", showContainer: nonEmpty(translated) };
    }
    if (presentation?.clearText) {
      return { mode: "clear", showContainer: false };
    }
    const hasHint = nonEmpty(presentation?.hint);
    return { mode: hasHint ? "hint" : "clear", showContainer: hasHint };
  }

  return { resolveSubtitleSurface };
});
