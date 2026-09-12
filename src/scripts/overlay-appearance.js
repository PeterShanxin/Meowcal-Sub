/* global module */

// Owner of the overlay's appearance state.
//
// Two paths set it: `get_settings` on startup, which must supply defaults for
// anything missing, and the `overlay-settings-updated` event from the main
// window, which is a partial patch and must ignore fields it does not carry.
// Both used to be open-coded chains of type guards inside `overlay.js`, which
// is how the two paths drifted - the loader coerced with `||`, the patch
// checked `typeof`, and neither was the stated rule.
//
// This module holds the rule and the defaults; the adapter keeps the DOM. The
// main window loads it too, so both windows offer the same text-size range.
(function exposeOverlayAppearance(root) {
  // Below 20px a subtitle is unreadable at viewing distance (#75), so a smaller
  // stored size is raised rather than honoured.
  const FONT_SIZE_MIN = 20;
  const FONT_SIZE_MAX = 48;

  const DEFAULT_APPEARANCE = Object.freeze({
    fontSize: 28,
    fontFamily: "Segoe UI",
    textColor: "#FFFFFF",
    lightBackground: false,
    showDiagnostics: false,
  });

  function clampFontSize(value) {
    if (!Number.isFinite(value)) return DEFAULT_APPEARANCE.fontSize;
    return Math.min(FONT_SIZE_MAX, Math.max(FONT_SIZE_MIN, Math.round(value)));
  }

  // Startup: a missing or falsy value takes the default, and the two toggles
  // are strictly boolean so an absent field can never read as "on".
  function hydrateAppearance(overlaySettings) {
    const settings = overlaySettings || {};

    return {
      fontSize: clampFontSize(settings.fontSize || DEFAULT_APPEARANCE.fontSize),
      fontFamily: settings.fontFamily || DEFAULT_APPEARANCE.fontFamily,
      textColor: settings.textColor || DEFAULT_APPEARANCE.textColor,
      lightBackground: settings.lightBackground === true,
      showDiagnostics: settings.showDiagnostics === true,
    };
  }

  // Live update: take only the fields the payload actually carries, with the
  // type each field is defined as. `applied` names what was taken, so the
  // adapter can sync exactly the controls that changed.
  function patchAppearance(current, payload) {
    const patch = payload || {};
    const next = {
      fontSize: current.fontSize,
      fontFamily: current.fontFamily,
      textColor: current.textColor,
      lightBackground: current.lightBackground,
      showDiagnostics: current.showDiagnostics,
    };
    const applied = [];

    const take = (key, type) => {
      if (typeof patch[key] !== type) return;
      next[key] = patch[key];
      applied.push(key);
    };

    take("fontSize", "number");
    take("fontFamily", "string");
    take("textColor", "string");
    take("lightBackground", "boolean");
    take("showDiagnostics", "boolean");
    next.fontSize = clampFontSize(next.fontSize);

    return { applied, next };
  }

  const api = {
    DEFAULT_APPEARANCE,
    FONT_SIZE_MAX,
    FONT_SIZE_MIN,
    clampFontSize,
    hydrateAppearance,
    patchAppearance,
  };
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.OverlayAppearance = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this);
