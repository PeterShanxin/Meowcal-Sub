import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const { moveRegion, resizeRegion } = require("../../src/scripts/region-geometry.js");

describe("region geometry", () => {
  it("moves a region without clamping its screen coordinates", () => {
    expect(moveRegion({ x: 100, y: 200, width: 80, height: 40 }, -120, 15)).toEqual({
      x: -20,
      y: 215,
      width: 80,
      height: 40,
    });
  });

  it.each([
    ["se", { x: 100, y: 200, width: 90, height: 45 }, 10, 5],
    ["nw", { x: 120, y: 210, width: 60, height: 30 }, 20, 10],
    ["e", { x: 100, y: 200, width: 30, height: 40 }, -100, 0],
    ["w", { x: 150, y: 200, width: 30, height: 40 }, 100, 0],
    ["n", { x: 100, y: 210, width: 80, height: 30 }, 0, 20],
    ["s", { x: 100, y: 200, width: 80, height: 30 }, 0, -20],
    ["ne", { x: 100, y: 210, width: 90, height: 30 }, 10, 20],
    ["sw", { x: 120, y: 200, width: 60, height: 30 }, 20, -20],
  ])(
    "resizes from %s against the selector's 30-pixel minimum",
    (handle, expected, deltaX, deltaY) => {
      expect(
        resizeRegion({ x: 100, y: 200, width: 80, height: 40 }, handle, deltaX, deltaY, 30),
      ).toEqual(expected);
    },
  );

  it.each([
    // The base region is only 40 tall, so the overlay's 50px floor also lifts
    // every result's height - which is the point: the frame has to stay large
    // enough to hold its own resize handles.
    ["e", { x: 100, y: 200, width: 50, height: 50 }, -40, 0],
    ["w", { x: 130, y: 200, width: 50, height: 50 }, 40, 0],
    ["n", { x: 100, y: 190, width: 80, height: 50 }, 0, 0],
    ["se", { x: 100, y: 200, width: 100, height: 60 }, 20, 20],
  ])(
    "resizes from %s against the overlay's larger 50-pixel minimum",
    (handle, expected, deltaX, deltaY) => {
      expect(
        resizeRegion({ x: 100, y: 200, width: 80, height: 40 }, handle, deltaX, deltaY, 50),
      ).toEqual(expected);
    },
  );

  it("pins the edge the user is not dragging when the minimum applies", () => {
    // Dragging the west edge past the minimum must leave the east edge where it
    // was, so the frame collapses toward the cursor rather than jumping.
    const region = { x: 100, y: 200, width: 80, height: 40 };
    const collapsed = resizeRegion(region, "w", 500, 0, 30);

    expect(collapsed.x + collapsed.width).toBe(region.x + region.width);
  });

  it("leaves the source region untouched", () => {
    const region = { x: 10, y: 20, width: 30, height: 40 };

    moveRegion(region, 5, 5);
    resizeRegion(region, "se", 5, 5, 30);

    expect(region).toEqual({ x: 10, y: 20, width: 30, height: 40 });
  });
});
