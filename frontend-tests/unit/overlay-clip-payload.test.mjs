import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const { clipPayloadEquals } = require("../../src/scripts/overlay-clip-payload.js");

function payload(overrides = {}) {
  return {
    frameRegion: { x: 10, y: 20, width: 300, height: 80 },
    subtitleBounds: { x: 10, y: 110, width: 300, height: 60 },
    handleBounds: [{ x: 3, y: 13, width: 14, height: 14 }],
    controlRadii: [7],
    scaleFactor: 1.5,
    ...overrides,
  };
}

describe("overlay clip payload equality", () => {
  it("treats an unchanged payload as equal so the settle loop stays silent", () => {
    expect(clipPayloadEquals(payload(), payload())).toBe(true);
  });

  it("detects a moved frame", () => {
    expect(
      clipPayloadEquals(
        payload(),
        payload({ frameRegion: { x: 11, y: 20, width: 300, height: 80 } }),
      ),
    ).toBe(false);
  });

  it("detects the subtitle box appearing", () => {
    expect(clipPayloadEquals(payload({ subtitleBounds: null }), payload())).toBe(false);
  });

  it("detects a scale factor change", () => {
    expect(clipPayloadEquals(payload(), payload({ scaleFactor: 2 }))).toBe(false);
  });

  it("detects handles appearing and disappearing", () => {
    expect(clipPayloadEquals(payload(), payload({ handleBounds: null }))).toBe(false);
    expect(
      clipPayloadEquals(
        payload(),
        payload({
          handleBounds: [
            { x: 3, y: 13, width: 14, height: 14 },
            { x: 40, y: 13, width: 14, height: 14 },
          ],
          controlRadii: [7, 7],
        }),
      ),
    ).toBe(false);
  });

  it("detects a radius change with identical rectangles", () => {
    expect(clipPayloadEquals(payload(), payload({ controlRadii: [4] }))).toBe(false);
  });

  it("never reports equality against a missing payload", () => {
    expect(clipPayloadEquals(payload(), null)).toBe(false);
    expect(clipPayloadEquals(null, null)).toBe(true);
  });
});
