import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { formatUpdateFeedNotes, getUpdateNotes } from "../../src/ui/update-notes";

const preview = (notes: string | null) => getUpdateNotes(notes).preview;

describe("update highlights", () => {
  it("turns the shipped release into three readable highlights without download or test details", () => {
    const notes = readFileSync(new URL("../../docs/releases/v0.9.1.md", import.meta.url), "utf8");
    expect(preview(notes)).toEqual([
      {
        title: "Keep short replies and small changes.",
        detail: "Single Chinese characters, negations and changed numbers stay distinct.",
      },
      {
        title: "Steadier subtitles.",
        detail:
          "Translations appear in order with a fixed bottom position and a reading interval based on source duration, text length and delay.",
      },
      {
        title: "Confirm the right area.",
        detail: "Subtitle selection follows the main window's monitor.",
      },
    ]);
  });

  it("reads wrapped bullets, removes authoring markup, and limits the list to three", () => {
    expect(
      preview(
        "## ✨ Highlights\n\n- ⚡ **Faster.** Read [short replies](https://example.com)\n  sooner (#59). More detail.\n- **Clearer.** Use `Settings`.\n- Third change.\n- Fourth change.\n\n## Fixes and improvements\n- Other fixes.",
      ),
    ).toEqual([
      { title: "Faster.", detail: "Read short replies sooner." },
      { title: "Clearer.", detail: "Use `Settings`." },
      { title: "", detail: "Third change." },
    ]);
  });

  it("uses fixes when a release has no highlights", () => {
    expect(
      preview(
        "# Meowcal Sub v1.0.0\n## Fixes and improvements\n- Fixed a crash.\n## Download\n- Installer",
      ),
    ).toEqual([{ title: "", detail: "Fixed a crash." }]);
  });

  it("round trips the compact manifest and preserves literal HTML as text", () => {
    const notes = "- **Readable.** Text stays visible.\n- <img src=x onerror=alert(1)>";
    expect(preview(notes)).toEqual([
      { title: "Readable.", detail: "Text stays visible." },
      { title: "", detail: "<img src=x onerror=alert(1)>" },
    ]);
  });

  it("keeps legacy plain notes short, and handles missing or unrelated sections", () => {
    expect(preview(null)).toEqual([]);
    expect(preview("  ")).toEqual([]);
    expect(preview("## Download\n- installer.exe")).toEqual([]);
    expect(preview("New fixes. More detail.")).toEqual([{ title: "", detail: "New fixes." }]);
    const [long] = preview("- " + "Long text ".repeat(100));
    expect(long.detail.length).toBeLessThanOrEqual(220);
    expect(long.detail.endsWith("…")).toBe(true);
  });
  it("keeps the actual v0.11.0 wording and fixes without exposing notes unrelated to the update", () => {
    const raw = readFileSync(new URL("../../docs/releases/v0.11.0.md", import.meta.url), "utf8");
    const feed = formatUpdateFeedNotes(raw);
    const notes = getUpdateNotes(feed);
    expect(notes.preview).toHaveLength(2);
    expect(notes.preview[0].detail).toContain("select **Send**.");
    expect(notes.more[0].detail).toBe(
      "When you open the form from an error, it includes the available diagnostics for that error.",
    );
    expect(notes.fixes.map(({ detail }) => detail)).toEqual([
      "If a send fails, your draft stays in the form so you can try again.",
      "After delivery, you get a link to the public issue. It shows the feedback type, app version and reference number. The report and attachments are automatically deleted 30 days after the team closes the issue.",
    ]);
    expect(feed).not.toMatch(/setup.exe|SHA256SUMS|#340|#342/);
    expect(formatUpdateFeedNotes(feed)).toBe(feed);
  });

  it("uses Chinese sentence boundaries even without a space and keeps overflow for expansion", () => {
    const raw =
      "## 主要更新\n- **看见变化。** 请点 **发送**。还有更多。\n- 第二项。\n- 第三项。\n- 第四项。\n## 修复与改进\n- 修复问题。";
    const notes = getUpdateNotes(formatUpdateFeedNotes(raw));
    expect(notes.preview[0]).toEqual({ title: "看见变化。", detail: "请点 **发送**。" });
    expect(notes.more).toEqual([
      { title: "看见变化。", detail: "还有更多。" },
      { title: "", detail: "第四项。" },
    ]);
    expect(notes.fixes[0].detail).toBe("修复问题。");
  });

  it("does not offer more details for an already compact legacy feed", () => {
    expect(
      getUpdateNotes(
        "- **Send a report from the app.** Choose a feedback type and select **Send**.",
      ),
    ).toEqual({
      preview: [
        {
          title: "Send a report from the app.",
          detail: "Choose a feedback type and select **Send**.",
        },
      ],
      more: [],
      fixes: [],
    });
  });
});
