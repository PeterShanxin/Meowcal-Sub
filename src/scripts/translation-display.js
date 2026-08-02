/* global module */

(function exposeTranslationDisplay(root, factory) {
  const api = factory();

  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }

  if (root) {
    root.TranslationDisplay = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this, () => {
  const DISPLAY_STATES = new Set([
    "translated",
    "warming",
    "temporarilyUnavailable",
    "sourceOnly",
    "noSubtitleText",
    "stopped",
  ]);

  function normalizeTranslationDisplayState(state, backendUsed) {
    if (DISPLAY_STATES.has(state)) {
      return state;
    }

    return String(backendUsed || "").toLowerCase() === "mock" ? "sourceOnly" : "translated";
  }

  function getTranslationPresentation(state, backendUsed) {
    const normalized = normalizeTranslationDisplayState(state, backendUsed);

    switch (normalized) {
      case "warming":
        return {
          state: normalized,
          replaceText: false,
          clearText: false,
          hint: "Translation engine is warming up",
          severity: "warn",
          persist: true,
        };
      case "temporarilyUnavailable":
        return {
          state: normalized,
          replaceText: false,
          clearText: false,
          hint: "Translation temporarily unavailable",
          severity: "error",
          persist: true,
        };
      case "sourceOnly":
        return {
          state: normalized,
          replaceText: false,
          clearText: false,
          hint: "OCR captured · no translation shown",
          severity: "warn",
          persist: true,
        };
      case "noSubtitleText":
        // The pipeline is healthy and the region simply has no text in it. The
        // previous translation must go, or a stale line sits over the video.
        return {
          state: normalized,
          replaceText: false,
          clearText: false,
          hint: "No subtitle text in the selected area",
          severity: "warn",
          persist: true,
        };
      case "stopped":
        return {
          state: normalized,
          replaceText: false,
          clearText: true,
          hint: "",
          severity: "warn",
          persist: false,
        };
      default:
        return {
          state: "translated",
          replaceText: true,
          clearText: false,
          hint: "",
          severity: "ok",
          persist: false,
        };
    }
  }

  return {
    getTranslationPresentation,
    normalizeTranslationDisplayState,
  };
});
