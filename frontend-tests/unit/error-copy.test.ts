import { describe, expect, it } from "vitest";
import { describeCaptureWarning, describeError } from "../../src/ui/error-copy";

describe("main window error copy", () => {
  it("offers area selection when start fails because no area is selected", () => {
    expect(describeError("No capture region set. Please select an area first.")).toEqual({
      text: "Select the subtitle area before starting.",
      action: { kind: "selectRegion", label: "Select area" },
    });
  });

  it("offers repair when the engine is not ready to start", () => {
    expect(describeError("The local translation engine is not ready yet.").action).toEqual({
      kind: "repair",
      label: "Repair",
    });
  });

  it("shows an unclassified failure as reported, with no action", () => {
    expect(describeError("Screen capture failed")).toEqual({
      text: "Screen capture failed",
      action: null,
    });
  });
});

describe("capture warning copy", () => {
  it.each([
    [
      "Using GDI fallback - video content may not capture correctly",
      "Compatibility capture · Protected video may not be read",
    ],
    [
      "OCR language 'ja-JP' is not installed. Using system default. Install it: Windows Settings > Time & Language > Language & Region.",
      "Recognition language missing · Using Windows default",
    ],
    ["Capture paused", "Capture paused"],
  ])("describes %s", (message, text) => {
    expect(describeCaptureWarning(message)).toBe(text);
  });
});
