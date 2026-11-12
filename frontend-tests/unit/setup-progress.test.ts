import { describe, expect, it } from "vitest";
import { setupProgress } from "../../src/ui/setup-progress";

describe("setup progress", () => {
  it.each([
    ["Checking Windows, memory, and storage...", "Checking this PC…"],
    ["Downloading the translation runtime...", "Downloading engine…"],
    ["Runtime: 35%", "Downloading engine: 35%"],
    ["Installing or repairing the translation runtime...", "Installing engine…"],
    ["Downloading Tencent HY-MT (about 1.1 GB)...", "Downloading model…"],
    ["Model: 80%", "Downloading model: 80%"],
    ["Warming up and checking a sample translation...", "Checking the engine…"],
  ])("maps Core progress %s", (line, expected) => {
    expect(setupProgress(line)).toBe(expected);
  });

  it("does not replace progress with unrelated diagnostics", () => {
    expect(setupProgress("warning: retrying")).toBeNull();
  });
});
