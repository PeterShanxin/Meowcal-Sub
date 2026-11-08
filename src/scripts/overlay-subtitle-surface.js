/* global module */

(function exposeOverlaySubtitleSurface(root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) module.exports = api;
  if (root) root.OverlaySubtitleSurface = api;
})(typeof globalThis !== "undefined" ? globalThis : this, () => {
  function nonEmpty(value) {
    return typeof value === "string" && value.trim() !== "";
  }

  /** Preserve a readable line during a retry, without creating an empty plate. */
  function resolveSubtitleSurface(presentation, translated, previousText = "") {
    if (presentation?.replaceText) {
      return { mode: "text", showContainer: nonEmpty(translated) };
    }
    if (presentation?.keepText && !presentation.clearText) {
      return { mode: "keep", showContainer: nonEmpty(previousText) };
    }
    return { mode: "clear", showContainer: false };
  }

  return { resolveSubtitleSurface };
});
