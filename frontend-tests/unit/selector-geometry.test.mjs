import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const {
  buildCaptureRegionPayload,
  buildDimOverlaySegments,
  clampSelectionHole,
  meetsMinimumSelection,
  screenRectToClientRect,
  selectionRectFromPoints,
} = require("../../src/scripts/selector-geometry.js");

describe("selector geometry", () => {
  it("normalizes forward and reverse screen drags", () => {
    expect(selectionRectFromPoints(100, 200, 340, 280)).toEqual({
      x: 100,
      y: 200,
      width: 240,
      height: 80,
    });
    expect(selectionRectFromPoints(340, 280, 100, 200)).toEqual({
      x: 100,
      y: 200,
      width: 240,
      height: 80,
    });
  });

  it("preserves zero-width and zero-height selection boundaries", () => {
    expect(selectionRectFromPoints(120, 240, 120, 300)).toEqual({
      x: 120,
      y: 240,
      width: 0,
      height: 60,
    });
    expect(meetsMinimumSelection({ width: 30, height: 15 })).toBe(true);
    expect(meetsMinimumSelection({ width: 29, height: 15 })).toBe(false);
    expect(meetsMinimumSelection({ width: 30, height: 14 })).toBe(false);
  });

  it("maps a screen rectangle into client coordinates at a non-zero origin", () => {
    expect(
      screenRectToClientRect({ x: 150, y: 260, width: 320, height: 80 }, { x: 100, y: 200 }),
    ).toEqual({ left: 50, top: 60, width: 320, height: 80 });
  });

  it("keeps negative virtual-screen origins in presentation coordinates", () => {
    expect(
      screenRectToClientRect({ x: -1820, y: -80, width: 320, height: 80 }, { x: -1920, y: -100 }),
    ).toEqual({ left: 100, top: 20, width: 320, height: 80 });
  });

  it("clamps a dim hole to viewport edges", () => {
    expect(
      clampSelectionHole({ x: -20, y: -10, width: 100, height: 80 }, { width: 640, height: 480 }),
    ).toEqual({ left: 0, top: 0, width: 80, height: 70 });

    const segments = buildDimOverlaySegments(
      { x: 600, y: 450, width: 100, height: 100 },
      { width: 640, height: 480 },
    );
    expect(segments).toEqual({
      top: { top: 0, left: 0, width: 640, height: 450 },
      bottom: { top: 480, left: 0, width: 640, height: 0 },
      left: { top: 450, left: 0, width: 600, height: 30 },
      right: { top: 450, left: 640, width: 0, height: 30 },
    });
  });

  it("pads and clamps the persisted capture region while preserving fractional DPI", () => {
    expect(
      buildCaptureRegionPayload(
        { x: 105, y: 55, width: 100, height: 40 },
        { left: 100, top: 50, right: 500, bottom: 300 },
        1.25,
      ),
    ).toEqual({ x: 100, y: 50, width: 113, height: 55, scaleFactor: 1.25 });
  });

  it("applies the existing non-negative clamp for a negative screen origin", () => {
    expect(
      buildCaptureRegionPayload(
        { x: -100, y: -50, width: 20, height: 20 },
        { left: -1920, top: -1080, right: 0, bottom: 1080 },
        1.5,
      ),
    ).toEqual({ x: 0, y: 0, width: 1, height: 1, scaleFactor: 1.5 });
  });
});
