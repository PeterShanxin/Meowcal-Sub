/* global module */

(function exposeTranslationDisplay(root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) module.exports = api;
  if (root) root.TranslationDisplay = api;
})(typeof globalThis !== "undefined" ? globalThis : this, () => {
  const DISPLAY_STATES = new Set([
    "translated",
    "warming",
    "temporarilyUnavailable",
    "sourceOnly",
    "noSubtitleText",
    "sourceUnreadable",
    "engineSlow",
    "stopped",
  ]);

  function normalizeTranslationDisplayState(state, backendUsed) {
    if (DISPLAY_STATES.has(state)) return state;
    return String(backendUsed || "").toLowerCase() === "mock" ? "sourceOnly" : "translated";
  }

  function getTranslationPresentation(state, backendUsed) {
    const normalized = normalizeTranslationDisplayState(state, backendUsed);
    const replaceText = normalized === "translated";
    const clearText = ["noSubtitleText", "sourceOnly", "stopped"].includes(normalized);
    return { state: normalized, replaceText, clearText, keepText: !replaceText && !clearText };
  }

  return { getTranslationPresentation, normalizeTranslationDisplayState };
});
