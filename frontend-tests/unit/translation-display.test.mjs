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
    ["sourceUnreadable", false, false],
    ["engineSlow", false, false],
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

  // OCR read the region, so this state must never claim the area is empty -
  // that sends the viewer to move a region that was never the problem.
  it.each([
    ["tooShort", "Only stray marks found in the selected area"],
    ["untranslatable", "Nothing translatable in the selected area"],
    ["tooLong", "Too much text in the selected area — narrow it to the subtitle line"],
    // These two keys are produced by `OcrRejection::as_str` on the Rust side.
    // Nothing else pins them together: an unknown key silently falls back to the
    // generic hint below, so renaming one would downgrade the overlay without
    // failing anything.
    ["garbled", "The text in this area came back too garbled to translate"],
    ["bandHeld", "Waiting — the text here does not look like dialogue yet"],
  ])("explains %s without claiming the area is empty", (reason, hint) => {
    expect(getTranslationPresentation("sourceUnreadable", "", [reason])).toMatchObject({
      hint,
      persist: true,
    });
  });

  it("falls back to a generic reason when the backend sends an unknown one", () => {
    const presentation = getTranslationPresentation("sourceUnreadable", "", ["somethingNew"]);
    expect(presentation.hint).toBe("Subtitle text in this area could not be read");
  });

  // Issue #60: a busy machine used to look like a broken app, because a slow
  // call blanked the overlay and said nothing. The engine is answering here, so
  // this must not read as an outage, and it must not clear the line either -
  // the previous translation is the best thing still on screen.
  it("says the engine is behind rather than unavailable", () => {
    const presentation = getTranslationPresentation("engineSlow");
    expect(presentation).toMatchObject({
      hint: "Translation engine is falling behind — close other heavy apps",
      severity: "warn",
      persist: true,
      clearText: false,
      replaceText: false,
      // `clearText: false` alone did not achieve this: the overlay's hint path
      // blanked the text on its way to showing the banner, so the line was lost
      // anyway. `keepText` is the flag the surface resolver actually acts on.
      keepText: true,
    });
    expect(presentation.hint).not.toMatch(/unavailable|failed|error/i);
  });

  it("labels passthrough payloads from older backends as source only", () => {
    expect(normalizeTranslationDisplayState(undefined, "mock")).toBe("sourceOnly");
  });

  it("keeps translated as the compatibility default for real backends", () => {
    expect(normalizeTranslationDisplayState(undefined, "foundry_local")).toBe("translated");
  });
});
