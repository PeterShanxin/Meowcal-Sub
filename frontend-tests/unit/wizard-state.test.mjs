import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const { isReady, progressMessage, supportCode } = require("../../src/scripts/wizard-state.js");

describe("guided engine setup state", () => {
  it("shows only a fully running engine as ready", () => {
    expect(isReady({ serviceRunning: true, phase: "ready" })).toBe(true);
    expect(isReady({ serviceRunning: false, phase: "ready" })).toBe(false);
    expect(isReady({ serviceRunning: true, phase: "warming" })).toBe(false);
    expect(isReady(null)).toBe(false);
  });

  it("extracts stable support codes without exposing raw infrastructure", () => {
    expect(supportCode(new Error("ENGINE_DISK_SPACE: free storage"))).toBe("ENGINE_DISK_SPACE");
    expect(supportCode("request failed")).toBe("ENGINE_SETUP_FAILED");
  });

  it.each([
    ["", "Working…"],
    ["Checking Windows, memory, and storage...", "Checking this PC…"],
    ["Downloading Tencent HY-MT", "Downloading translation engine…"],
    ["Downloading the translation runtime", "Downloading engine support files…"],
    ["Installing or repairing the translation runtime", "Installing translation engine…"],
    ["Warming up and checking a sample", "Starting and testing translation…"],
    ["Install failed; restored the last known-good engine", "Restoring the working engine…"],
    ["Local Translation Engine verified", "Verifying translation engine…"],
    ["Preparing files", "Preparing files"],
  ])("maps progress %s to one user-facing state", (line, expected) => {
    expect(progressMessage(line)).toBe(expected);
  });
});
