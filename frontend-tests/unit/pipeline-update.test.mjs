import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const { position, shouldAccept } = require("../../src/scripts/pipeline-update.js");

describe("pipeline update ordering", () => {
  it("rejects stale captures from the current or an older session", () => {
    const previous = { sessionId: 4, captureId: 9 };
    expect(shouldAccept(previous, { sessionId: 4, captureId: 8 })).toBe(false);
    expect(shouldAccept(previous, { sessionId: 4, captureId: 9 })).toBe(false);
    expect(shouldAccept(previous, { sessionId: 3, captureId: 20 })).toBe(false);
  });

  it("accepts a newer capture or session", () => {
    const previous = { sessionId: 4, captureId: 9 };
    expect(shouldAccept(previous, { sessionId: 4, captureId: 10 })).toBe(true);
    expect(shouldAccept(previous, { sessionId: 5, captureId: 0 })).toBe(true);
  });

  it("keeps compatibility with payloads that predate pipeline IDs", () => {
    expect(shouldAccept({ sessionId: 1, captureId: 1 }, {})).toBe(true);
    expect(position({})).toBeNull();
    expect(position({ sessionId: 2, captureId: 3 })).toEqual({
      sessionId: 2,
      captureId: 3,
    });
  });
});
