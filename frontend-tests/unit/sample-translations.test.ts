import { describe, expect, it } from "vitest";
import { languages } from "../../src/ui/languages";
import { pickSampleTranslation, sampleTranslations } from "../../src/ui/sample-translations";

describe("curated sample translations", () => {
  it("provides at least three subtitle-style samples for every source language", () => {
    for (const language of languages) {
      expect(sampleTranslations[language.value]).toHaveLength(3);
      expect(sampleTranslations[language.value].every((sample) => sample.length > 8)).toBe(true);
    }
  });

  it("selects varied samples deterministically from an injectable random value", () => {
    const samples = sampleTranslations["zh-CN"];

    expect(pickSampleTranslation("zh-CN", 0)).toBe(samples[0]);
    expect(pickSampleTranslation("zh-CN", 0.5)).toBe(samples[1]);
    expect(pickSampleTranslation("zh-CN", 1)).toBe(samples[2]);
    expect(new Set([0, 0.5, 1].map((value) => pickSampleTranslation("zh-CN", value))).size).toBe(3);
  });

  it("clamps invalid random edges instead of producing an invalid sample", () => {
    const samples = sampleTranslations["en-US"];

    expect(pickSampleTranslation("en-US", -1)).toBe(samples[0]);
    expect(pickSampleTranslation("en-US", Number.NaN)).toBe(samples[0]);
    expect(pickSampleTranslation("en-US", Number.POSITIVE_INFINITY)).toBe(samples[0]);
    expect(pickSampleTranslation("en-US", 2)).toBe(samples[2]);
  });
});
