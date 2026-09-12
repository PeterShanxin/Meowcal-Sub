import { describe, expect, it } from "vitest";
import { describeError } from "../../src/ui/error-copy";

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
