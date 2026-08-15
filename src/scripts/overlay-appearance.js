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
// This module holds the rule and the defaults; the adapter keeps the DOM.
(function exposeOverlayAppearance(root) {
  const DEFAULT_APPEARANCE = Object.freeze({
    fontSize: 24,
    fontFamily: "Segoe UI",
    textColor: "#FFFFFF",
    lightBackground: false,
    showDiagnostics: false,
  });

  // Startup: a missing or falsy value takes the default, and the two toggles
  // are strictly boolean so an absent field can never read as "on".
  function hydrateAppearance(overlaySettings) {
    const settings = overlaySettings || {};

    return {
      fontSize: settings.fontSize || DEFAULT_APPEARANCE.fontSize,
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

    return { applied, next };
  }

  const api = { DEFAULT_APPEARANCE, hydrateAppearance, patchAppearance };
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.OverlayAppearance = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this);
