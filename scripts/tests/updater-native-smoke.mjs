import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect } from "@playwright/test";
import { formatUpdateFeedNotes } from "../../src/ui/update-notes.ts";

/** Intercept only update checks, before startup can start a real feed request. */
export async function installUpdaterFixture(page) {
  await page.addInitScript(() => {
    const fixture = { announcement: null, delivered: 0 };
    globalThis.__updaterSmokeFixture = fixture;
    Object.defineProperty(globalThis, "TauriBridge", {
      configurable: true,
      set(bridge) {
        Object.defineProperty(globalThis, "TauriBridge", {
          configurable: true,
          writable: true,
          value: bridge,
        });
        if (bridge.isBrowserMode() || !bridge.updates) return;
        bridge.updates.check = async () => {
          if (!fixture.announcement) return null;
          fixture.delivered += 1;
          return {
            ...fixture.announcement,
            close: async () => {},
            install: async () => {
              throw Error("The updater smoke must not install anything");
            },
          };
        };
      },
    });
  });
}

/** Verify the normal Settings check path in the existing disposable native app. */
export async function verifyNativeUpdateNotes(page, output, installedVersion) {
  const release = new URL("../../docs/releases/v0.11.0.md", import.meta.url);
  const notes = formatUpdateFeedNotes(readFileSync(release, "utf8"));
  const [major, minor, patch] = installedVersion.split(/[.+-]/, 3).map(Number);
  const version = `${major}.${minor}.${patch + 1}`;
  const autoCheck = page.getByRole("switch", { name: /自动检查更新/ });
  if (await autoCheck.isChecked()) await autoCheck.click();
  await expect(autoCheck).not.toBeChecked();
  const check = page.getByRole("button", { name: /^(检查更新|再次检查)$/ });
  await expect(check).toBeEnabled();
  await page.evaluate(
    (announcement) => {
      if (globalThis.TauriBridge.isBrowserMode() || !globalThis.TauriBridge.updates)
        throw Error("Native updater unavailable");
      globalThis.__updaterSmokeFixture.announcement = announcement;
    },
    { version, notes },
  );
  await check.click();
  await expect.poll(() => page.evaluate(() => globalThis.__updaterSmokeFixture.delivered)).toBe(1);
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
