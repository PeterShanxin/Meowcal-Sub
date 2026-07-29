import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const {
  isOcrLanguageAvailable,
  normalizeOcrLanguageTag,
} = require("../../src/scripts/ocr-language-tags.js");

describe("normalizeOcrLanguageTag", () => {
  it.each([
    ["zh_Hans_CN", "zh-hans-cn"],
    ["ZH-cn", "zh-cn"],
    [" ja-JP ", "ja-jp"],
  ])("normalizes %s", (input, expected) => {
    expect(normalizeOcrLanguageTag(input)).toBe(expected);
  });
});

describe("isOcrLanguageAvailable", () => {
  it.each([
    [["zh-Hans-CN"], "zh-CN"],
    [["zh-Hans"], "zh-CN"],
    [["zh-CN"], "zh-Hans-CN"],
    [["zh-Hant-TW"], "zh-TW"],
    [["zh-Hant"], "zh-TW"],
    [["zh-TW"], "zh-Hant-TW"],
  ])("accepts equivalent Windows Chinese OCR tags: %j provides %s", (installed, requested) => {
    expect(isOcrLanguageAvailable(installed, requested)).toBe(true);
  });

  it.each([
    [["zh-Hant-TW"], "zh-CN"],
    [["zh-Hans-CN"], "zh-TW"],
    [["en-GB"], "en-US"],
    [[], "zh-CN"],
  ])("does not alias distinct OCR languages: %j does not provide %s", (installed, requested) => {
    expect(isOcrLanguageAvailable(installed, requested)).toBe(false);
  });

  it("matches ordinary tags case-insensitively", () => {
    expect(isOcrLanguageAvailable(new Set(["JA-jp"]), "ja-JP")).toBe(true);
  });
});
