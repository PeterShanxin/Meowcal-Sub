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
    "sourceUnreadable",
    "stopped",
  ]);

  // Why the pipeline threw away a line it did read. The backend sends the key;
  // the wording lives here so the viewer is told what to change, not which
  // filter fired.
  const UNREADABLE_HINTS = {
    lowConfidence: "Subtitle text here is too unclear to read",
    tooShort: "Only stray marks found in the selected area",
    untranslatable: "Nothing translatable in the selected area",
  };

  function unreadableHint(reason) {
    return UNREADABLE_HINTS[reason] || "Subtitle text in this area could not be read";
  }

  function normalizeTranslationDisplayState(state, backendUsed) {
    if (DISPLAY_STATES.has(state)) {
      return state;
    }

    return String(backendUsed || "").toLowerCase() === "mock" ? "sourceOnly" : "translated";
  }

  function getTranslationPresentation(state, backendUsed, warnings) {
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
      case "sourceUnreadable":
        // OCR did read the region, so this must never say the area is empty:
        // that sends the viewer to move a region that was never the problem.
        return {
          state: normalized,
          replaceText: false,
          clearText: false,
          hint: unreadableHint(Array.isArray(warnings) ? warnings[0] : warnings),
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
