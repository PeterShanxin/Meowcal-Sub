import { describe, expect, it } from "vitest";
import { describeCaptureWarning, describeError } from "../../src/ui/error-copy";
import { setUiLanguage } from "../../src/i18n/index";

describe("main window error copy", () => {
  it("localizes recognition recovery instructions while preserving technical details", () => {
    const message =
      "Text recognition could not start: OCR_PIPE_FAILED: C:/Users/小明. Try the Compatibility profile in Settings.";
    setUiLanguage("zh-CN");
    expect(describeError(message).text).toBe(
      "文字识别无法启动，请在设置中尝试兼容模式。技术详情：OCR_PIPE_FAILED: C:/Users/小明",
    );
    const stopped = "Text recognition is not responding. Return to Home and restart translation.";
    expect(describeError(stopped).text).toBe("文字识别没有响应，请返回首页并重新开始翻译。");
    setUiLanguage("en");
    expect(describeError(stopped).text).toBe(stopped);
    expect(describeError(message).text).toBe(
      "Text recognition could not start. Try the Compatibility profile in Settings. Technical details: OCR_PIPE_FAILED: C:/Users/小明",
    );
  });
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
