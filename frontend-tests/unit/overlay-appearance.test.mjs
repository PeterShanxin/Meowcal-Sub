import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const {
  DEFAULT_APPEARANCE,
  hydrateAppearance,
  patchAppearance,
} = require("../../src/scripts/overlay-appearance.js");

describe("overlay appearance hydration", () => {
  it("supplies every default when settings carry nothing", () => {
    expect(hydrateAppearance({})).toEqual(DEFAULT_APPEARANCE);
    expect(hydrateAppearance(undefined)).toEqual(DEFAULT_APPEARANCE);
    expect(hydrateAppearance(null)).toEqual(DEFAULT_APPEARANCE);
  });

  it("takes stored values over defaults", () => {
    expect(
      hydrateAppearance({
        fontSize: 32,
        fontFamily: "Cascadia Code",
        textColor: "#00FF00",
        lightBackground: true,
        showDiagnostics: true,
      }),
    ).toEqual({
      fontSize: 32,
      fontFamily: "Cascadia Code",
      textColor: "#00FF00",
      lightBackground: true,
      showDiagnostics: true,
    });
  });

  it("treats a falsy stored size or colour as absent", () => {
    // A zero font size or empty colour is a corrupt setting, not a request for
    // an invisible subtitle.
    expect(hydrateAppearance({ fontSize: 0, textColor: "" })).toEqual(DEFAULT_APPEARANCE);
  });

  it("keeps the toggles strictly boolean so a stray truthy value cannot turn them on", () => {
    expect(hydrateAppearance({ lightBackground: "yes", showDiagnostics: 1 })).toMatchObject({
      lightBackground: false,
      showDiagnostics: false,
    });
  });

  it("does not expose the shared default object for mutation", () => {
    const hydrated = hydrateAppearance({});
    hydrated.fontSize = 99;

    expect(DEFAULT_APPEARANCE.fontSize).toBe(24);
  });
});

describe("overlay appearance patching", () => {
  const current = Object.freeze({
    fontSize: 24,
    fontFamily: "Segoe UI",
    textColor: "#FFFFFF",
    lightBackground: false,
    showDiagnostics: false,
  });

  it("keeps every field and applies nothing for an empty payload", () => {
    expect(patchAppearance(current, {})).toEqual({ applied: [], next: { ...current } });
    expect(patchAppearance(current, undefined)).toEqual({ applied: [], next: { ...current } });
  });

  it("takes only the fields the payload carries", () => {
    expect(patchAppearance(current, { fontSize: 30 })).toEqual({
      applied: ["fontSize"],
      next: { ...current, fontSize: 30 },
    });
  });

  it("ignores fields carrying the wrong type", () => {
    expect(
      patchAppearance(current, {
        fontSize: "30",
        fontFamily: 12,
        lightBackground: "true",
        showDiagnostics: null,
      }),
    ).toEqual({ applied: [], next: { ...current } });
  });

  it("accepts a boolean turning a toggle off", () => {
    const on = { ...current, lightBackground: true, showDiagnostics: true };

    expect(patchAppearance(on, { lightBackground: false, showDiagnostics: false })).toEqual({
      applied: ["lightBackground", "showDiagnostics"],
      next: { ...current },
    });
  });

  it("does not mutate the current appearance", () => {
    patchAppearance(current, { fontSize: 40, showDiagnostics: true });

    expect(current.fontSize).toBe(24);
    expect(current.showDiagnostics).toBe(false);
  });
});
