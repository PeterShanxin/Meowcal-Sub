import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const {
  getTranslationPresentation,
  normalizeTranslationDisplayState,
} = require("../../src/scripts/translation-display.js");

describe("quiet subtitle presentation", () => {
  it.each([
    ["translated", true, false, false],
    ["warming", false, false, true],
    ["temporarilyUnavailable", false, false, true],
    ["sourceOnly", false, true, false],
    ["noSubtitleText", false, true, false],
    ["sourceUnreadable", false, false, true],
    ["engineSlow", false, false, true],
    ["stopped", false, true, false],
  ])("presents %s without an inline notice", (state, replaceText, clearText, keepText) => {
    expect(getTranslationPresentation(state)).toEqual({ state, replaceText, clearText, keepText });
  });

  it.each(["tooShort", "untranslatable", "tooLong", "garbled", "bandHeld", "somethingNew"])(
    "keeps the readable line during %s without warning text",
    (reason) => {
      expect(getTranslationPresentation("sourceUnreadable", "", [reason])).toMatchObject({
        keepText: true,
        clearText: false,
      });
      expect(getTranslationPresentation("sourceUnreadable", "", [reason]).hint).toBeUndefined();
    },
  );

  it("never labels a passthrough result as translated", () => {
    expect(normalizeTranslationDisplayState(undefined, "mock")).toBe("sourceOnly");
    expect(getTranslationPresentation(undefined, "mock").replaceText).toBe(false);
  });

  it("preserves the existing real-backend compatibility default", () => {
    expect(normalizeTranslationDisplayState(undefined, "local_engine")).toBe("translated");
    expect(normalizeTranslationDisplayState(undefined)).toBe("translated");
  });
});
