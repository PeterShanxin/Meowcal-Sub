import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const {
  getTranslationPresentation,
  normalizeTranslationDisplayState,
} = require("../../src/scripts/translation-display.js");

describe("translation display states", () => {
  it.each([
    ["translated", true, false],
    ["warming", false, false],
    ["temporarilyUnavailable", false, false],
    ["sourceOnly", false, false],
    ["noSubtitleText", false, false],
    ["stopped", false, true],
  ])("presents %s without mislabeling source text", (state, replaceText, clearText) => {
    expect(getTranslationPresentation(state)).toMatchObject({
      state,
      replaceText,
      clearText,
    });
  });

  // The pipeline is still healthy here, so the box has to stay up with an
  // explanation instead of leaving the previous translation on screen.
  it("retires the previous line when the region has no text", () => {
    expect(getTranslationPresentation("noSubtitleText")).toMatchObject({
      hint: "No subtitle text in the selected area",
      persist: true,
    });
  });

  it("labels passthrough payloads from older backends as source only", () => {
    expect(normalizeTranslationDisplayState(undefined, "mock")).toBe("sourceOnly");
  });

  it("keeps translated as the compatibility default for real backends", () => {
    expect(normalizeTranslationDisplayState(undefined, "foundry_local")).toBe("translated");
  });
});
