import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { access, chmod, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { arch, release } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { chromium, expect } from "@playwright/test";
import { createServer } from "vite";
import { allocatePort } from "../allocate-port.mjs";

// Tauri uses Windows known folders for app data. A process-local APPDATA override
// alone cannot isolate those paths, hence the separate compiled application ID.
const identifier = "com.meowcal.sub.feedback-smoke.dev";
const origin = "http://127.0.0.1:1420";
const output = resolve("test-results/feedback-native");
const executable = resolve(
  process.env.CARGO_TARGET_DIR || "src-tauri/target",
  "debug/meowcal-sub.exe",
);

async function waitFor(get, description, app) {
  const deadline = Date.now() + 60000;
  while (Date.now() < deadline) {
    if (app.exitCode !== null || app.signalCode)
      throw Error("Native app exited before " + description);
    const value = await get();
    if (value) return value;
    await delay(200);
  }
  throw Error("Timed out waiting for " + description);
}

async function captureViews(page, configFile) {
  const version = await page.evaluate(async () => {
    const bridge = globalThis.TauriBridge;
    if (!bridge || bridge.isBrowserMode() || !globalThis.__TAURI__.core)
      throw Error("Native IPC unavailable");
    return bridge.appVersion();
  });
  assert.match(version, /^\d+\.\d+\.\d+/);
  const entry = page.getByRole("button", { name: "反馈", exact: true });
  const settings = page.getByRole("button", { name: "设置", exact: true });
  const screenshot = (name) =>
    page.screenshot({ path: join(output, name), animations: "disabled" });
  await expect(entry).toBeVisible();
  await expect(page.locator("#browser-mode-indicator")).toHaveCount(0);
  await page.waitForFunction(() => globalThis.__TAURI__.window.getCurrentWindow().isVisible());
  await screenshot("01-home.png");
  await settings.click();
  await expect(page.getByRole("heading", { name: "设置", exact: true })).toBeVisible();
  await screenshot("02-settings.png");
  await entry.click();
  const dialog = page.getByRole("dialog", { name: "反馈" });
  await dialog.getByLabel("反馈类型").selectOption("suggestion");
  await dialog.getByRole("textbox").fill("希望可以调整字幕的停留时间。");
  await expect(dialog.getByRole("button", { name: "发送", exact: true })).toBeEnabled();
  await expect(dialog.getByText(version, { exact: true })).toBeVisible();
  await screenshot("03-feedback.png");
  await dialog.getByRole("button", { name: "关闭", exact: true }).click();

  // A read-only destination makes Windows reject the app's atomic rename.
  // This produces a real save error without installing a model or sending data.
  await chmod(configFile, 0o444);
  try {
    await page.getByRole("switch", { name: /保持名称一致/ }).click();
    const reportError = page.getByRole("alert").getByRole("button", { name: "报告错误" });
    await expect(reportError).toBeVisible();
    await screenshot("04-error-entry.png");
    await reportError.click();
    await expect(dialog.getByLabel("反馈类型")).toHaveValue("error");
    await expect(dialog.getByText("APP_ERROR", { exact: true })).toBeVisible();
    await expect(dialog.getByRole("button", { name: "发送", exact: true })).toBeEnabled();
    await screenshot("05-error-report.png");
  } finally {
    await chmod(configFile, 0o600);
  }
  return version;
}

async function main() {
  if (
    process.platform !== "win32" ||
    process.env.GITHUB_ACTIONS !== "true" ||
    process.env.RUNNER_ENVIRONMENT !== "github-hosted" ||
    !process.env.RUNNER_TEMP
  ) {
    throw Error("Run in the isolated GitHub-hosted Windows frontend job.");
  }
  await access(executable);
  await mkdir(output, { recursive: true });
  const profile = await mkdtemp(join(process.env.RUNNER_TEMP, "meowcal-feedback-"));
  const roaming = join(profile, "Roaming");
  const local = join(profile, "Local");
  const userData = join(profile, "WebView");
  const logs = join(profile, "Logs");
  for (const dir of [roaming, local, userData, logs]) await mkdir(dir);
  const configFolders = [join(process.env.APPDATA, identifier), join(roaming, identifier)];
  const seed = { uiLanguage: "zh-CN", autoCheckUpdates: false };
  for (const dir of configFolders) {
    // A pre-existing namespace is never ours to seed or change.
    await mkdir(dir);
    await writeFile(join(dir, "config.json"), JSON.stringify(seed), { flag: "wx" });
  }
  const port = await allocatePort();
  const server = await createServer({
    clearScreen: false,
    server: { host: "127.0.0.1", port: 1420, strictPort: true, hmr: false },
    plugins: [
      {
        name: "feedback-smoke-setup",
        transformIndexHtml: {
          order: "pre",
          handler(_html, ctx) {
            if (ctx.path !== "/" && ctx.path !== "/index.html") return [];
            return [
              {
                tag: "script",
                children: 'localStorage.setItem("meowcal.setupAcknowledged","true")',
                injectTo: "head-prepend",
              },
            ];
          },
        },
      },
    ],
  });
  let app;
  let browser;
  let page;
  try {
    await server.listen();
    app = spawn(executable, [], {
      env: {
        ...process.env,
        APPDATA: roaming,
        LOCALAPPDATA: local,
        MEOWCAL_LOG_DIR: logs,
        MEOWCAL_CORE_EXECUTABLE: resolve("src-tauri/resources/core/meowcal-core.exe"),
        WEBVIEW2_USER_DATA_FOLDER: userData,
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:
          "--remote-debugging-address=127.0.0.1 --remote-debugging-port=" + port,
      },
      stdio: "inherit",
    });
    await once(app, "spawn");
    const endpoint = "http://127.0.0.1:" + port;
    await waitFor(
      async () =>
        (
          await fetch(endpoint + "/json/version", { signal: AbortSignal.timeout(1500) }).catch(
            () => null,
          )
        )?.ok,
      "WebView2 debugger",
      app,
    );
    browser = await chromium.connectOverCDP(endpoint, { timeout: 20000 });
    page = await waitFor(
      () =>
        browser
          .contexts()
          .flatMap((ctx) => ctx.pages())
          .find((p) => p.url() === origin + "/"),
      "the main WebView",
      app,
    );
    await page.waitForFunction(() => typeof globalThis.TauriBridge?.appIdentifier === "function");
    assert.equal(await page.evaluate(() => globalThis.TauriBridge.appIdentifier()), identifier);
    const appDirectory = resolve(await page.evaluate(() => globalThis.__TAURI__.path.appDataDir()));
    assert.ok(
      configFolders.some((path) => resolve(path).toLowerCase() === appDirectory.toLowerCase()),
      "Unexpected native config directory",
    );
    const version = await captureViews(page, join(appDirectory, "config.json"));
    const config = JSON.parse(await readFile(join(appDirectory, "config.json"), "utf8"));
    assert.equal(config.uiLanguage, "zh-CN");
    await writeFile(
      join(output, "evidence.json"),
      JSON.stringify(
        {
          version,
          commit: process.env.GITHUB_SHA,
          architecture: arch(),
          windows: release(),
          surface: "visible native Tauri main window (WebView2)",
          language: "zh-CN",
          fault: "Windows rejected a settings save to the isolated read-only config",
          submissionAttempted: false,
          modelInstalled: false,
        },
        null,
        2,
      ) + "\n",
    );
  } finally {
    if (page && !page.isClosed()) {
      await page
        .evaluate(() => {
          void globalThis.TauriBridge.invoke("quit_initial_setup");
        })
        .catch(() => {});
    }
    await browser?.close().catch(() => {});
    if (app && app.exitCode === null && !app.signalCode) {
      await Promise.race([once(app, "exit"), delay(3000)]);
      if (app.exitCode === null && !app.signalCode) app.kill();
    }
    await server.close();
  }
}

await main();
