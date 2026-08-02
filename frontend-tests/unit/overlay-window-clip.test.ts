import { afterEach, describe, expect, it, vi } from "vitest";

interface ClipElement {
  getBoundingClientRect(): Pick<DOMRect, "left" | "top" | "width" | "height">;
}

describe("overlay clip surface geometry", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("collects bounds with each surface's computed radius", async () => {
    vi.stubGlobal("window", {});
    vi.stubGlobal(
      "getComputedStyle",
      vi.fn(() => ({ borderTopLeftRadius: "4px" })),
    );
    // @ts-expect-error The legacy helper is a JavaScript module without declarations.
    await import("../../src/scripts/overlay-window-clip.js");
    const api = (
      window as unknown as {
        OverlayWindowClip: {
          appendClipSurface(bounds: unknown[], radii: number[], element: ClipElement): void;
        };
      }
    ).OverlayWindowClip;
    const bounds: unknown[] = [];
    const radii: number[] = [];

    api.appendClipSurface(bounds, radii, {
      getBoundingClientRect: () => ({ left: 10.4, top: 20.6, width: 100, height: 40 }),
    });

    expect(bounds).toEqual([{ x: 10, y: 21, width: 100, height: 40 }]);
    expect(radii).toEqual([4]);
  });
});
