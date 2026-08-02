import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const { resolveSubtitleSurface } = require("../../src/scripts/overlay-subtitle-surface.js");
const { getTranslationPresentation } = require("../../src/scripts/translation-display.js");

function surfaceFor(displayState, backendUsed, translated) {
  return resolveSubtitleSurface(getTranslationPresentation(displayState, backendUsed), translated);
}

describe("overlay subtitle surface", () => {
  it("shows the box for translated text", () => {
    expect(surfaceFor("translated", "foundry_local", "先不提时钟塔")).toEqual({
      mode: "text",
      showContainer: true,
    });
  });

  it("hides the box when the translated text is blank", () => {
    expect(surfaceFor("translated", "foundry_local", "   ")).toEqual({
      mode: "text",
      showContainer: false,
    });
  });

  it.each([
    "warming",
    "temporarilyUnavailable",
    "sourceOnly",
    "noSubtitleText",
    "sourceUnreadable",
  ])("keeps the box visible so the %s state is readable", (displayState) => {
    expect(surfaceFor(displayState, "foundry_local", "")).toEqual({
      mode: "hint",
      showContainer: true,
    });
  });

  it("hides the box when translation stops", () => {
    expect(surfaceFor("stopped", "foundry_local", "")).toEqual({
      mode: "clear",
      showContainer: false,
    });
  });

  it("hides the box for a hint-less non-text state", () => {
    expect(resolveSubtitleSurface({ hint: "  " }, "")).toEqual({
      mode: "clear",
      showContainer: false,
    });
  });
});
