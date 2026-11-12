import { expect, test } from "@playwright/test";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";

const bridgePath = fileURLToPath(new URL("../../src/scripts/tauri-bridge.js", import.meta.url));
const releaseNotes = readFileSync(
  new URL("../../docs/releases/v0.9.1.md", import.meta.url),
  "utf8",
);

test("Settings shows three formatted highlights and a version-specific full notes link", async ({
  page,
}, testInfo) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.evaluate((notes) => {
    document.querySelector("meowcal-app").controller.publish({
      update: { kind: "available", version: "0.9.1", notes },
    });
  }, releaseNotes);
  const notes = page.getByRole("region", { name: "What's new" });
  await expect(notes.getByRole("listitem")).toHaveCount(3);
  await expect(notes.locator("strong")).toHaveText([
    "Keep short replies and small changes.",
    "Steadier subtitles.",
    "Confirm the right area.",
  ]);
  await expect(notes).not.toContainText("**");
  await expect(notes).not.toContainText("Download");
  await expect(notes).not.toContainText("How this release was tested");
  const link = notes.getByRole("link", { name: "Full release notes" });
  await expect(link).toHaveAttribute(
    "href",
    "https://github.com/PeterShanxin/Meowcal-Sub/releases/tag/v0.9.1",
  );
  await expect(link).toHaveAttribute("target", "_blank");
  await link.focus();
  await expect(link).toBeFocused();
  for (const width of [1280, 560]) {
    await page.setViewportSize({ width, height: 800 });
    await notes.scrollIntoViewIfNeeded();
    expect(await notes.evaluate((element) => element.scrollHeight <= element.clientHeight)).toBe(
      true,
    );
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
  }
  await page.setViewportSize({ width: 1280, height: 900 });
  await notes.scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath("update-highlights.png") });

  await page.evaluate(() => {
    document.querySelector("meowcal-app").controller.publish({
      update: {
        kind: "available",
        version: "0.9.2",
        notes: "- **Safe.** <img src=x onerror=alert(1)>",
      },
    });
  });
  await expect(notes.locator("img")).toHaveCount(0);
  await expect(notes).toContainText("<img src=x onerror=alert(1)>");
  await expect(link).toHaveAttribute("href", /\/v0\.9\.2$/);

  await page.evaluate(() => {
    document.querySelector("meowcal-app").controller.publish({
      update: { kind: "available", version: "0.9.3", notes: null },
    });
  });
  await expect(notes.getByRole("list")).toHaveCount(0);
  await expect(link).toHaveAttribute("href", /\/v0\.9\.3$/);
  await page.evaluate(() => {
    document.querySelector("meowcal-app").controller.publish({ update: { kind: "upToDate" } });
  });
  await expect(notes).toHaveCount(0);
});

test("the updater limits metadata checks while downloads keep their own lifetime", async ({
  page,
}) => {
  await page.setContent("<!doctype html><title>Updater bridge</title><body></body>");
  await page.evaluate(() => {
    window.updateCalls = [];
    window.__TAURI__ = {
      core: {
        async invoke(command, args) {
          window.updateCalls.push({ command, args });
        },
      },
      updater: {
        async check(options) {
          window.updateCalls.push({ command: "check", args: options });
          return {
            version: "0.9.1",
            body: "Fixes",
            async downloadAndInstall(...args) {
              window.updateCalls.push({
                command: "downloadAndInstall",
                argumentCount: args.length,
              });
              args[0]({ event: "Finished" });
            },
            async close() {
              window.updateCalls.push({ command: "close" });
            },
          };
        },
      },
    };
  });
  await page.addScriptTag({ path: bridgePath });
  const result = await page.evaluate(async () => {
    const update = await window.TauriBridge.updates.check({ timeout: 10_000 });
    const progress = [];
    await update.install((event) => progress.push(event));
    await update.close();
    await window.TauriBridge.invoke("record_update_check", { event: { phase: "started" } });
    return { version: update.version, progress, calls: window.updateCalls };
  });
  expect(result).toEqual({
    version: "0.9.1",
    progress: [{ event: "Finished" }],
    calls: [
      { command: "check", args: { timeout: 10_000 } },
      { command: "downloadAndInstall", argumentCount: 1 },
      { command: "close" },
      { command: "record_update_check", args: { event: { phase: "started" } } },
    ],
  });
});

test("browser preview has no updater and identifies update diagnostics as native-only", async ({
  page,
}) => {
  await page.setContent("<!doctype html><title>Updater bridge</title><body></body>");
  await page.addScriptTag({ path: bridgePath });
  expect(
    await page.evaluate(() => ({
      browserMode: window.TauriBridge.isBrowserMode(),
      hasUpdater: Boolean(window.TauriBridge.updates),
      nativeOnly: window.TauriBridge.TAURI_ONLY_COMMANDS.includes("record_update_check"),
    })),
  ).toEqual({ browserMode: true, hasUpdater: false, nativeOnly: true });
});
