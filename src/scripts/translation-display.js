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
    "engineSlow",
    "stopped",
  ]);

  // Why the pipeline threw away a line it did read. The backend sends the key;
  // the wording lives here so the viewer is told what to change, not which
  // filter fired.
  const UNREADABLE_HINTS = {
    tooShort: "Only stray marks found in the selected area",
    untranslatable: "Nothing translatable in the selected area",
    tooLong: "Too much text in the selected area — narrow it to the subtitle line",
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
      case "engineSlow":
        // The engine is working, just losing a race with the video. Saying it
        // is unavailable would send the viewer to repair a healthy engine; the
        // thing they can actually act on is the load on their machine.
        return {
          state: normalized,
          replaceText: false,
          clearText: false,
          // Not merely "do not clear": the line must be actively kept. A hint
          // state used to blank the text on its way to showing the banner, so
          // declining to clear was not enough to hold on to it.
          keepText: true,
          hint: "Translation engine is falling behind — close other heavy apps",
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
