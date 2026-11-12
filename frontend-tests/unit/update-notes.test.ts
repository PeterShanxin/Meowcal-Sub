import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { summarizeUpdateNotes } from "../../src/ui/update-notes";

describe("update highlights", () => {
  it("turns the shipped release into three readable highlights without download or test details", () => {
    const notes = readFileSync(new URL("../../docs/releases/v0.9.1.md", import.meta.url), "utf8");
    expect(summarizeUpdateNotes(notes)).toEqual([
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
      summarizeUpdateNotes(
        "## ✨ Highlights\n\n- ⚡ **Faster.** Read [short replies](https://example.com)\n  sooner (#59). More detail.\n- **Clearer.** Use `Settings`.\n- Third change.\n- Fourth change.\n\n## Fixes and improvements\n- Other fixes.",
      ),
    ).toEqual([
      { title: "Faster.", detail: "Read short replies sooner." },
      { title: "Clearer.", detail: "Use Settings." },
      { title: "", detail: "Third change." },
    ]);
  });

  it("uses fixes when a release has no highlights", () => {
    expect(
      summarizeUpdateNotes(
        "# Meowcal Sub v1.0.0\n## Fixes and improvements\n- Fixed a crash.\n## Download\n- Installer",
      ),
    ).toEqual([{ title: "", detail: "Fixed a crash." }]);
  });

  it("round trips the compact manifest and preserves literal HTML as text", () => {
    const notes = "- **Readable.** Text stays visible.\n- <img src=x onerror=alert(1)>";
    expect(summarizeUpdateNotes(notes)).toEqual([
      { title: "Readable.", detail: "Text stays visible." },
      { title: "", detail: "<img src=x onerror=alert(1)>" },
    ]);
  });

  it("keeps legacy plain notes short, and handles missing or unrelated sections", () => {
    expect(summarizeUpdateNotes(null)).toEqual([]);
    expect(summarizeUpdateNotes("  ")).toEqual([]);
    expect(summarizeUpdateNotes("## Download\n- installer.exe")).toEqual([]);
    expect(summarizeUpdateNotes("New fixes. More detail.")).toEqual([
      { title: "", detail: "New fixes." },
    ]);
    const [long] = summarizeUpdateNotes("- " + "Long text ".repeat(100));
    expect(long.detail.length).toBeLessThanOrEqual(220);
    expect(long.detail.endsWith("…")).toBe(true);
  });
});
