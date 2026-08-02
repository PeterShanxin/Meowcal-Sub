import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const read = (relativePath) =>
  readFileSync(fileURLToPath(new URL(`../../src/${relativePath}`, import.meta.url)), "utf8");

const SHELL_PAGES = ["index.html", "wizard.html"];

function linkedStylesheets(page) {
  return [...read(page).matchAll(/href="\.\/styles\/([\w-]+\.css)"/g)].map((match) => match[1]);
}

function definesKeyframes(stylesheet, name) {
  return new RegExp(`@keyframes\\s+${name}\\b`).test(read(`styles/${stylesheet}`));
}

describe("shell stylesheets", () => {
  // The setup wizard animates its stage spinner with `meowcal-spin`. When the
  // keyframes lived only in home-next.css the wizard resolved the animation to
  // nothing, so a slow install showed a frozen icon and looked like a hang.
  it.each(SHELL_PAGES)("%s can resolve the meowcal-spin animation", (page) => {
    const sheets = linkedStylesheets(page);
    expect(sheets.some((sheet) => definesKeyframes(sheet, "meowcal-spin"))).toBe(true);
  });

  it("keeps a single definition of the shared spin keyframes", () => {
    const definitions = new Set(
      SHELL_PAGES.flatMap(linkedStylesheets).filter((sheet) =>
        definesKeyframes(sheet, "meowcal-spin"),
      ),
    );
    expect([...definitions]).toEqual(["app-shell.css"]);
  });
});
