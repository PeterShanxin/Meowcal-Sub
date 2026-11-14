import { describe, expect, it } from "vitest";
import { compareBilingualDocs } from "../../scripts/bilingual-docs.mjs";

describe("bilingual user documentation", () => {
  const english = "## Install\n[Guide](docs/USAGE.md#installation)\n```powershell\nverify.ps1\n```";
  const chinese = "## 安装\n[指南](docs/USAGE.zh-CN.md#安装)\n```powershell\nverify.ps1\n```";

  it("accepts translated labels, anchors, and paired filenames", () => {
    expect(compareBilingualDocs(english, chinese)).toEqual([]);
  });
  it("accepts the paired language pages of the product site", () => {
    expect(
      compareBilingualDocs(
        '<a href="https://example.com/app/">Website</a>',
        '<a href="https://example.com/app/zh-CN/">官网</a>',
      ),
    ).toEqual([]);
  });
  it("rejects a missing section", () => {
    expect(compareBilingualDocs(english, chinese.replace("## 安装", "安装"))).toContain(
      "section structure differ between English and Simplified Chinese",
    );
  });
  it("rejects a stale installer version", () => {
    expect(
      compareBilingualDocs(
        "[Download](https://example.com/v1/app.exe)",
        "[下载](https://example.com/v0/app.exe)",
      ),
    ).toContain("links and release downloads differ between English and Simplified Chinese");
  });
  it("rejects mismatched commands and the discarded Chinese name", () => {
    expect(
      compareBilingualDocs(english, chinese.replace("verify.ps1", "build.ps1") + "妙幕"),
    ).toEqual([
      "example commands differ between English and Simplified Chinese",
      "Chinese brand must be 妙影",
    ]);
  });
});
