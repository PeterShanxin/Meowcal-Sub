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
    ["stopped", false, true],
  ])("presents %s without mislabeling source text", (state, replaceText, clearText) => {
    expect(getTranslationPresentation(state)).toMatchObject({
      state,
      replaceText,
      clearText,
    });
  });

  it("labels passthrough payloads from older backends as source only", () => {
    expect(normalizeTranslationDisplayState(undefined, "mock")).toBe("sourceOnly");
  });

  it("keeps translated as the compatibility default for real backends", () => {
    expect(normalizeTranslationDisplayState(undefined, "foundry_local")).toBe("translated");
  });
});
