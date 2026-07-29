import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const { formatFoundryPhase, formatReadyState } = require("../../src/scripts/backend-status.js");

describe("formatReadyState", () => {
  it.each([
    ["ready", { label: "Ready", className: "ready" }],
    ["notReady", { label: "Not Ready", className: "not-ready" }],
    ["notSupported", { label: "Not Supported", className: "not-supported" }],
    ["error", { label: "Error", className: "error" }],
    ["futureValue", { label: "Unknown", className: "error" }],
  ])("maps %s to stable presentation", (state, expected) => {
    expect(formatReadyState(state)).toEqual(expected);
  });
});

describe("formatFoundryPhase", () => {
  it.each([
    ["ready", { label: "Ready", className: "ready" }],
    ["unchecked", { label: "Not checked", className: "unchecked" }],
    ["preparing", { label: "Preparing", className: "preparing" }],
    ["notRunning", { label: "Not Running", className: "not-ready" }],
    ["notrunning", { label: "Not Running", className: "not-ready" }],
    ["noModels", { label: "No Models", className: "not-ready" }],
    ["nomodels", { label: "No Models", className: "not-ready" }],
    ["notInstalled", { label: "Not Installed", className: "not-supported" }],
    ["notinstalled", { label: "Not Installed", className: "not-supported" }],
    ["error", { label: "Error", className: "error" }],
    ["futureValue", { label: "Unknown", className: "error" }],
  ])("maps %s to stable presentation", (phase, expected) => {
    expect(formatFoundryPhase(phase)).toEqual(expected);
  });
});
