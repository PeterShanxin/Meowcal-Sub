import { expect, test } from "@playwright/test";
import { fileURLToPath } from "node:url";

const bridgePath = fileURLToPath(new URL("../../src/scripts/tauri-bridge.js", import.meta.url));

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
