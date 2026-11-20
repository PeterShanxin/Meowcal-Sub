import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect } from "@playwright/test";
import { formatUpdateFeedNotes } from "../../src/ui/update-notes.ts";

/** Verify the normal Settings check path in the existing disposable native app. */
export async function verifyNativeUpdateNotes(page, output, installedVersion) {
  const release = new URL("../../docs/releases/v0.11.0.md", import.meta.url);
  const notes = formatUpdateFeedNotes(readFileSync(release, "utf8"));
  const [major, minor, patch] = installedVersion.split(/[.+-]/, 3).map(Number);
  const version = `${major}.${minor}.${patch + 1}`;
  await page.evaluate(
    ({ version, notes }) => {
      const bridge = globalThis.TauriBridge;
      if (bridge.isBrowserMode() || !bridge.updates) throw Error("Native updater unavailable");
      const updates = bridge.updates;
      const original = updates.check;
      updates.check = async () => {
        updates.check = original;
        return {
          version,
          notes,
          close: async () => {},
          install: async () => {
            throw Error("The updater smoke must not install anything");
          },
        };
      };
    },
    { version, notes },
  );
  await page.getByRole("button", { name: "检查更新", exact: true }).click();
  const card = page.getByRole("region", { name: "更新内容" });
  await expect(card.locator("strong").filter({ hasText: /^Send$/ })).toBeVisible();
  await expect(card).not.toContainText("**Send**");
  const more = card.getByText("展开更新详情", { exact: true });
  const fix = card.getByText("If a send fails, your draft stays in the form so you can try again.");
  await expect(fix).not.toBeVisible();
  const screenshot = (name) =>
    card.screenshot({ path: join(output, name), animations: "disabled" });
  await screenshot("06-update-notes-preview.png");
  await more.focus();
  await expect(more).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(fix).toBeVisible();
  await expect(card.getByRole("heading", { name: "修复与改进" })).toBeVisible();
  await expect(card.getByRole("link", { name: /完整更新说明/ })).toHaveAttribute(
    "href",
    `https://github.com/PeterShanxin/Meowcal-Sub/releases/tag/v${version}`,
  );
  await screenshot("07-update-notes-expanded.png");
  return {
    announcedVersion: version,
    feedFixture: "v0.11.0 notes",
    metadataSimulated: true,
    openedWithKeyboard: true,
    installationAttempted: false,
  };
}
