import { expect, test } from "@playwright/test";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";
import { formatUpdateFeedNotes } from "../../src/ui/update-notes.ts";

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
  await expect(notes.locator(":scope > ul strong.update-note-title")).toHaveText([
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

test("Settings formats inline emphasis in the shipped update and expands actual follow-on release notes", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  const raw = readFileSync(new URL("../../docs/releases/v0.11.0.md", import.meta.url), "utf8");
  const legacy =
    "- **Send a report from the app.** Choose a feedback type, describe what happened and select **Send**.\n" +
    "- **Choose what to share.** For a translation that looks wrong, you can preview the original line and its translation before attaching them.";
  async function announce(version, notes) {
    await page.evaluate(
      ({ version, notes }) => {
        document
          .querySelector("meowcal-app")
          .controller.publish({ update: { kind: "available", version, notes } });
      },
      { version, notes },
    );
  }
  const card = page.getByRole("region", { name: "What's new" });
  await announce("0.11.0", legacy);
  await expect(card.locator("strong").filter({ hasText: /^Send$/ })).toBeVisible();
  await expect(card).not.toContainText("**Send**");
  await expect(card.locator("summary")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Download and install" })).toBeVisible();

  await announce(
    "0.11.1",
    formatUpdateFeedNotes(raw.replace(/^# Meowcal Sub v0.11.0/m, "# Meowcal Sub v0.11.1")),
  );
  const more = card.getByText("More about this update");
  const fix = card.getByText("If a send fails, your draft stays in the form so you can try again.");
  await expect(more).toBeVisible();
  await expect(fix).not.toBeVisible();
  await more.focus();
  await expect(more).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(fix).toBeVisible();
  await expect(card.getByRole("heading", { name: "Fixes and improvements" })).toBeVisible();
  await expect(card.getByRole("link", { name: "Full release notes" })).toHaveAttribute(
    "href",
    /v0\.11\.1$/,
  );
  await expect(card).not.toContainText("SHA256SUMS");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test("the Store update row checks the Store and opens its listing instead of installing", async ({
  page,
}, testInfo) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.evaluate(async () => {
    const bridge = window.TauriBridge;
    const originalInvoke = bridge.invoke;
    window.storeUpdateCalls = [];
    bridge.appIdentifier = async () => "com.meowcal.sub.store";
    bridge.invoke = async (command, args) => {
      if (command === "check_store_update" || command === "open_store_listing") {
        window.storeUpdateCalls.push(command);
        return true;
      }
      return originalInvoke(command, args);
    };
    const controller = document.querySelector("meowcal-app").controller;
    controller.publish({ ...(await controller.updates.initialState()), settingsRead: "ready" });
  });

  await expect(page.getByLabel("Automatically check for updates")).toBeChecked();
  await page.getByRole("button", { name: "Check for updates", exact: true }).click();
  await expect(page.getByText("An update is available")).toBeVisible();
  await expect(page.locator(".nav-indicator")).toBeVisible();
  await expect(page.getByRole("button", { name: "Download and install" })).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath("store-update-available.png") });
  await page.getByRole("button", { name: "Open Microsoft Store", exact: true }).click();
  expect(await page.evaluate(() => window.storeUpdateCalls)).toEqual([
    "check_store_update",
    "open_store_listing",
  ]);
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
