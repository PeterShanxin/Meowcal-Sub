import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const {
  frameScaleTokens,
  resolveSubtitlePlacement,
  roundedRectBounds,
  subtitleHeightEstimate,
} = require("../../src/scripts/overlay-geometry.js");

describe("overlay frame scale tokens", () => {
  it.each([
    [1, 2, 8],
    [1.25, 2.4, 8],
    [1.5, 2, 8],
    [2, 2, 8],
  ])("snaps the frame ring to whole device pixels at %sx", (scaleFactor, borderCss, radiusCss) => {
    expect(frameScaleTokens(scaleFactor)).toEqual({ borderCss, radiusCss });
  });

  it.each([[undefined], [null], [0], [-1], ["1.5"], [Number.NaN]])(
    "falls back to an unscaled ring for %s",
    (scaleFactor) => {
      expect(frameScaleTokens(scaleFactor)).toEqual({ borderCss: 2, radiusCss: 8 });
    },
  );

  it("never produces a sub-pixel border that would disappear", () => {
    expect(frameScaleTokens(0.25).borderCss).toBeGreaterThan(0);
  });
});

describe("overlay subtitle placement", () => {
  const region = { x: 100, y: 200, width: 300, height: 100 };

  it("estimates the plate height from the font size only when it cannot be measured", () => {
    expect(subtitleHeightEstimate(0, 24)).toBe(88);
    expect(subtitleHeightEstimate(120, 24)).toBe(120);
  });

  it("places the plate below the capture region when it fits", () => {
    expect(resolveSubtitlePlacement(region, 1080, 0, 24, 10)).toEqual({
      left: 100,
      top: 310,
      width: 300,
      height: 88,
    });
  });

  it("places the plate above the capture region when only that fits", () => {
    expect(
      resolveSubtitlePlacement({ x: 100, y: 600, width: 300, height: 400 }, 700, 0, 24, 10),
    ).toEqual({ left: 100, top: 502, width: 300, height: 88 });
  });

  it("prefers below when neither side fits, rather than covering the captured text", () => {
    expect(
      resolveSubtitlePlacement({ x: 0, y: 40, width: 100, height: 100 }, 200, 0, 24, 10),
    ).toEqual({ left: 0, top: 150, width: 100, height: 88 });
  });

  it("treats an exactly-fitting gap below as fitting", () => {
    expect(
      resolveSubtitlePlacement({ x: 0, y: 0, width: 10, height: 10 }, 108, 0, 24, 10).top,
    ).toBe(20);
    expect(
      resolveSubtitlePlacement({ x: 0, y: 0, width: 10, height: 10 }, 107, 0, 24, 10).top,
    ).toBe(20);
  });

  it("uses the measured plate height so a hint row cannot overflow the gap", () => {
    const tall = { x: 100, y: 300, width: 300, height: 100 };

    // The estimate would fit below; the measured 220-tall plate (hint row shown)
    // does not, so the same region must flip above.
    expect(resolveSubtitlePlacement(tall, 600, 0, 24, 10).top).toBe(410);
    expect(resolveSubtitlePlacement(tall, 600, 220, 24, 10)).toEqual({
      left: 100,
      top: 70,
      width: 300,
      height: 220,
    });
  });
});

describe("overlay clip bounds rounding", () => {
  it("rounds a fractional DOM rect to the integral rectangle Win32 needs", () => {
    expect(roundedRectBounds({ left: 10.4, top: 20.6, width: 30.5, height: 40.49 })).toEqual({
      x: 10,
      y: 21,
      width: 31,
      height: 40,
    });
  });

  it("rounds negative offsets on a secondary monitor to the left of the primary", () => {
    expect(roundedRectBounds({ left: -10.6, top: -4.2, width: 100, height: 50 })).toEqual({
      x: -11,
      y: -4,
      width: 100,
      height: 50,
    });
  });
});
