import { describe, expect, it } from "vitest";
import { applyLanguageSelection, ensureDistinctLanguagePair } from "../../src/ui/languages";

describe("language pair selection", () => {
  it("repairs an existing same-language pair", () => {
    expect(
      ensureDistinctLanguagePair({ sourceLanguage: "ja-JP", targetLanguage: "ja-JP" }),
    ).toEqual({ sourceLanguage: "ja-JP", targetLanguage: "en-US" });
  });

  it("moves the counterpart when a selection would make the pair identical", () => {
    expect(
      applyLanguageSelection(
        { sourceLanguage: "zh-CN", targetLanguage: "en-US" },
        "source",
        "en-US",
      ),
    ).toEqual({ sourceLanguage: "en-US", targetLanguage: "zh-CN" });

    expect(
      applyLanguageSelection(
        { sourceLanguage: "zh-CN", targetLanguage: "en-US" },
        "target",
        "zh-CN",
      ),
    ).toEqual({ sourceLanguage: "en-US", targetLanguage: "zh-CN" });
  });
});
