import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const read = (relativePath) =>
  readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");

describe("main and setup entry graph", () => {
  it("loads the Lit main window entry without the deleted legacy controller", () => {
    const html = read("../../src/index.html");
    const entry = read("../../src/entries/main.ts");

    expect(html).toContain("./entries/main.ts");
    expect(html).not.toContain("main.js");
    expect(entry).toContain("../ui/meowcal-app");
    expect(entry).toContain("../scripts/tauri-bridge.js");
    expect(entry).not.toMatch(/BackendStatusPresentation|TranslationStart|main\.js/);
  });

  it("loads the Lit setup window without legacy wizard globals", () => {
    const html = read("../../src/wizard.html");
    const entry = read("../../src/entries/wizard.ts");

    expect(html).toContain("./entries/wizard.ts");
    expect(html).not.toContain("wizard.js");
    expect(entry).toContain("../ui/meowcal-setup");
    expect(entry).not.toMatch(/WizardState|wizard-state|wizard\.js/);
  });
});
