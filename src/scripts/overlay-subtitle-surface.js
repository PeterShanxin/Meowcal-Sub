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
   * @param {{ replaceText?: boolean, clearText?: boolean, keepText?: boolean, hint?: string }} presentation
   * @param {string} translated
   * @returns {{ mode: "text" | "hint" | "keep" | "clear", showContainer: boolean }}
   */
  function resolveSubtitleSurface(presentation, translated) {
    if (presentation?.replaceText) {
      return { mode: "text", showContainer: nonEmpty(translated) };
    }
    if (presentation?.clearText) {
      return { mode: "clear", showContainer: false };
    }
    const hasHint = nonEmpty(presentation?.hint);
    // `keep` exists because "does not clear" and "shows a hint" used to be
    // contradictory: the hint path blanked the line on its way to showing the
    // banner, so a state that declined to clear the text lost it anyway. That
    // is the wrong trade for a line the engine merely failed to *replace* in
    // time - the previous translation is still the best thing on screen, and
    // under the sustained load of issue #60 the viewer would otherwise get a
    // warning banner where a readable line used to be, for most of a session.
    if (presentation?.keepText && hasHint) {
      return { mode: "keep", showContainer: true };
    }
    return { mode: hasHint ? "hint" : "clear", showContainer: hasHint };
  }

  return { resolveSubtitleSurface };
});
