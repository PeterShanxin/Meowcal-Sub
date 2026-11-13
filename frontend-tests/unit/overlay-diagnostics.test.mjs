import { afterEach, describe, expect, it } from "vitest";
afterEach(() => setUiLanguage("en"));

import "../../src/scripts/overlay-diagnostics.js";
import { setUiLanguage } from "../../src/i18n/index";
const { buildDiagnosticsText } = globalThis.OverlayDiagnostics;

describe("overlay diagnostics text", () => {
  // Latency was the top complaint after a full episode, and there was nowhere
  // to see where the wait went without reading the dev console.
  it("reports total and model time separately", () => {
    const text = buildDiagnosticsText({
      state: "translated",
      backendUsed: "local_engine",
      modelMs: 1650,
      totalMs: 1820,
    });
    expect(text).toContain("Latency: 1820ms (model 1650ms)");
  });

  it("omits the latency row when nothing was timed", () => {
    const text = buildDiagnosticsText({ state: "noSubtitleText" });
    expect(text).not.toContain("Latency");
  });

  // The raw OCR line is the only way to tell a bad translation from a bad read.
  it("shows the OCR source line", () => {
    const text = buildDiagnosticsText({ state: "translated", source: "你好世界" });
    expect(text).toContain("Source: 你好世界");
  });

  it("leaves out the source row when OCR returned nothing", () => {
    expect(buildDiagnosticsText({ state: "stopped", source: "   " })).not.toContain("Source:");
  });

  // A lifecycle notice runs no backend; naming one made a healthy Foundry
  // session read as "mock (source only)".
  it("names no engine when none ran", () => {
    expect(buildDiagnosticsText({ state: "noSubtitleText", backendUsed: "" })).toBe(
      "State: noSubtitleText",
    );
  });

  it("still flags the mock backend as source only", () => {
    const text = buildDiagnosticsText({ state: "sourceOnly", backendUsed: "mock" });
    expect(text).toContain("mock (source only)");
  });

  it("includes rejection reasons so a filtered line explains itself", () => {
    const text = buildDiagnosticsText({
      state: "sourceUnreadable",
      warnings: ["tooShort"],
      now: "11:41:14 PM",
    });
    expect(text).toBe("State: sourceUnreadable · tooShort · 11:41:14 PM");
  });
});

it("localizes diagnostic labels without translating source text or protocol codes", () => {
  setUiLanguage("zh-CN");
  expect(
    buildDiagnosticsText({
      state: "translated",
      backendUsed: "mock",
      source: "A subtitle",
      totalMs: 80,
      modelMs: 60,
    }),
  ).toBe("状态：translated · mock（仅原文）\n延迟：80 毫秒（模型 60 毫秒）\n原文：A subtitle");
});
