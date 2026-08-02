import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const {
  pointInBounds,
  rectToPhysicalBounds,
  regionToPhysicalBounds,
} = require("../../src/scripts/overlay-hit-bounds.js");

describe("overlay hit bounds", () => {
  it("scales a window-relative rect into device pixels", () => {
    const rect = { left: 10, top: 20, right: 110, bottom: 70 };

    expect(rectToPhysicalBounds(rect, { x: 0, y: 0 }, 1.5)).toEqual({
      left: 15,
      top: 30,
      right: 165,
      bottom: 105,
    });
  });

  it("adds the window origin before scaling", () => {
    const rect = { left: 10, top: 20, right: 110, bottom: 70 };

    expect(rectToPhysicalBounds(rect, { x: -1920, y: -100 }, 1)).toEqual({
      left: -1910,
      top: -80,
      right: -1810,
      bottom: -30,
    });
  });

  it("treats a missing origin as the screen origin", () => {
    const rect = { left: 1, top: 2, right: 3, bottom: 4 };

    expect(rectToPhysicalBounds(rect, null, 1)).toEqual({
      left: 1,
      top: 2,
      right: 3,
      bottom: 4,
    });
  });

  it("pads the capture region so handles and the gear stay grabbable", () => {
    const region = { x: 100, y: 200, width: 300, height: 80 };

    expect(regionToPhysicalBounds(region, 40, { x: 0, y: 0 }, 1)).toEqual({
      left: 60,
      top: 160,
      right: 440,
      bottom: 320,
    });
  });

  it("includes the edges of the bounds", () => {
    const bounds = { left: 0, top: 0, right: 10, bottom: 10 };

    expect(pointInBounds({ x: 0, y: 10 }, bounds)).toBe(true);
    expect(pointInBounds({ x: 5, y: 5 }, bounds)).toBe(true);
    expect(pointInBounds({ x: 11, y: 5 }, bounds)).toBe(false);
    expect(pointInBounds({ x: 5, y: -1 }, bounds)).toBe(false);
  });
});
