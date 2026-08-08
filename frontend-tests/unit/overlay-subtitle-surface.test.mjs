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

  it.each(["warming", "temporarilyUnavailable", "sourceOnly", "noSubtitleText"])(
    "keeps the box visible so the %s state is readable",
    (displayState) => {
      expect(surfaceFor(displayState, "foundry_local", "")).toEqual({
        mode: "hint",
        showContainer: true,
      });
    },
  );

  // A refused read means OCR saw text it could not use, not that the region
  // emptied. Blanking here handed the viewer a warning banner in place of the
  // translation they were reading, one quarter-second after it appeared - the
  // blank-screen symptom of issue #59 arriving through the notice meant to
  // explain it.
  it("keeps the previous line on screen while explaining a refused read", () => {
    expect(surfaceFor("sourceUnreadable", "foundry_local", "")).toEqual({
      mode: "keep",
      showContainer: true,
    });
  });

  // Issue #60. `hint` blanks the line to make room for the banner, which is
  // right for a pipeline that has nothing to show and wrong for one whose last
  // translation is still the best thing on screen. Under sustained load most
  // calls are slow, so getting this wrong replaces the subtitles with a warning
  // for most of a session.
  it("keeps the previous line on screen while the engine is behind", () => {
    expect(surfaceFor("engineSlow", "foundry_local", "")).toEqual({
      mode: "keep",
      showContainer: true,
    });
  });

  it("still hides the box for a keepText state with nothing to say", () => {
    expect(resolveSubtitleSurface({ keepText: true, hint: "" }, "")).toEqual({
      mode: "clear",
      showContainer: false,
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
