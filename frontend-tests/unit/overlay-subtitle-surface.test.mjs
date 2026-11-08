import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const { resolveSubtitleSurface } = require("../../src/scripts/overlay-subtitle-surface.js");
const { getTranslationPresentation } = require("../../src/scripts/translation-display.js");

const surfaceFor = (state, text = "", previous = "") =>
  resolveSubtitleSurface(getTranslationPresentation(state, "local_engine"), text, previous);

describe("overlay subtitle surface", () => {
  it("shows only nonempty translated text", () => {
    expect(surfaceFor("translated", "Example")).toEqual({ mode: "text", showContainer: true });
    expect(surfaceFor("translated", "   ")).toEqual({ mode: "text", showContainer: false });
  });

  it.each(["warming", "temporarilyUnavailable", "sourceUnreadable", "engineSlow"])(
    "keeps the previous line quietly during %s",
    (state) => {
      expect(surfaceFor(state, "", "Readable line")).toEqual({ mode: "keep", showContainer: true });
      expect(surfaceFor(state)).toEqual({ mode: "keep", showContainer: false });
    },
  );

  it.each(["noSubtitleText", "sourceOnly", "stopped"])(
    "retires the previous line during %s",
    (state) => {
      expect(surfaceFor(state, "Raw source", "Previous")).toEqual({
        mode: "clear",
        showContainer: false,
      });
    },
  );

  it("clears unknown or absent presentation rules", () => {
    expect(resolveSubtitleSurface(undefined, "")).toEqual({ mode: "clear", showContainer: false });
    expect(resolveSubtitleSurface({ clearText: true, keepText: true }, "", "Old")).toEqual({
      mode: "clear",
      showContainer: false,
    });
  });
});
