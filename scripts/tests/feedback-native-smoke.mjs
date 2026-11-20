import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { once } from "node:events";
import {
  access,
  chmod,
  mkdir,
  mkdtemp,
  open,
  readdir,
  readFile,
  writeFile,
} from "node:fs/promises";
import { arch, release } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { chromium, expect } from "@playwright/test";
import { preview } from "vite";
import { verifyNativeUpdateNotes } from "./updater-native-smoke.mjs";

// Tauri uses Windows known folders for app data. A process-local APPDATA override
// alone cannot isolate those paths, hence the separate compiled application ID.
const identifier = "com.meowcal.sub.feedback-smoke.dev";
const origin = "http://127.0.0.1:1420";
const output = resolve("test-results/feedback-native");
const executable = resolve(
  process.env.CARGO_TARGET_DIR || "src-tauri/target",
  "debug/meowcal-sub.exe",
);

async function installedRuntimes() {
  const installs = [];
  for (const base of new Set([
    process.env["ProgramFiles(x86)"],
    process.env.ProgramFiles,
    process.env.LOCALAPPDATA,
  ])) {
    if (!base) continue;
    const dir = join(base, "Microsoft/EdgeWebView/Application");
    for (const version of await readdir(dir).catch(() => [])) {
      if (!/^\d+(?:\.\d+){3}$/.test(version)) continue;
      const file = await open(join(dir, version, "msedgewebview2.exe")).catch(() => null);
      if (!file) continue;
      try {
        const dos = Buffer.alloc(64);
        await file.read(dos, 0, dos.length, 0);
        const pe = Buffer.alloc(6);
        await file.read(pe, 0, pe.length, dos.readUInt32LE(60));
        assert.equal(pe.readUInt32LE(0), 0x4550);
        const machine = pe.readUInt16LE(4);
        installs.push({
          version,
          architecture:
            { 0xaa64: "arm64", 0x8664: "x64", 0x14c: "x86" }[machine] ?? machine.toString(16),
        });
      } finally {
        await file.close();
      }
    }
  }
  return installs;
}

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

function observeWebView(page, events) {
  const record = (kind, detail) => {
    if (!page.url().startsWith(origin + "/") || events.length >= 80) return;
    events.push({ path: new URL(page.url()).pathname, kind, detail: String(detail).slice(0, 500) });
  };
  page.on("pageerror", (error) => record("pageerror", error.message));
  page.on("console", (message) => {
    if (["warning", "error"].includes(message.type())) record(message.type(), message.text());
  });
  page.on("requestfailed", (req) => {
    if (req.url().startsWith(origin + "/"))
      record("requestfailed", new URL(req.url()).pathname + " " + req.failure()?.errorText);
  });
  page.on("response", (res) => {
    if (res.status() >= 400 && res.url().startsWith(origin + "/"))
      record("response", new URL(res.url()).pathname + " " + res.status());
  });
}

async function describeWebView(page) {
  const path = new URL(page.url()).pathname;
  try {
    const state = await Promise.race([
      page.evaluate(() => {
        const doc = globalThis.document;
        const names = (selector) =>
          [...doc.querySelectorAll(selector)]
            .slice(0, 20)
            .map((el) =>
              (el.getAttribute("aria-label") || el.textContent || "").trim().slice(0, 80),
            );
        return {
          title: doc.title,
          lang: doc.documentElement.lang,
          ready: doc.readyState,
          body: [...doc.body.children].map((el) => el.localName),
          nativeWindow: globalThis.__TAURI__?.window?.getCurrentWindow().label,
          appRegistered: Boolean(globalThis.customElements.get("meowcal-app")),
          acknowledged: globalThis.localStorage.getItem("meowcal.setupAcknowledged"),
          headers: names("h1, h2, [role=heading]"),
          buttons: names("button"),
          dialogs: names("dialog[open] h2"),
        };
      }),
      delay(3000, { inspection: "timed out" }),
    ]);
    return { path, ...state };
  } catch (error) {
    return { path, inspection: String(error) };
  }
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
  await expect(page.locator("meowcal-titlebar .feedback-entry")).toBeAttached({ timeout: 30000 });
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN", { timeout: 30000 });
  await expect(entry).toBeVisible();
  await expect(page.locator("#browser-mode-indicator")).toHaveCount(0);
  await expect
    .poll(() => page.evaluate(() => globalThis.__TAURI__.window.getCurrentWindow().isVisible()), {
      timeout: 30000,
    })
    .toBe(true);
  await screenshot("01-home.png");
  await settings.click();
  await expect(page.getByRole("heading", { name: "设置", exact: true })).toBeVisible();
  await screenshot("02-settings.png");
  await entry.click();
  const dialog = page.getByRole("dialog", { name: "反馈" });
  await dialog.getByLabel("反馈类型").selectOption("suggestion");
  await dialog.getByRole("textbox").fill("希望可以调整字幕的停留时间。");
  await expect(dialog.getByRole("button", { name: "发送", exact: true })).toBeEnabled();
  const diagnostics = dialog.locator(".feedback-diagnostics");
  if ((await diagnostics.getAttribute("open")) === null)
    await diagnostics.locator("summary").click();
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
    await dialog.getByRole("button", { name: "关闭", exact: true }).click();
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
  const runtimes = await installedRuntimes();
  console.log(
    "Windows WebView2 runtime inventory",
    JSON.stringify({ os: release(), arch: arch(), runtimes }),
  );
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
  const port = Number(process.env.MEOWCAL_FEEDBACK_SMOKE_PORT);
  assert.ok(
    Number.isInteger(port) && port >= 1 && port <= 65535,
    "Missing debugger port from the frontend build step",
  );
  // Load the same build verified by the frontend gate. A one-document bootstrap
  // lets us seed the disposable webview profile and attach error listeners
  // before executing the actual app bundle. All screenshot pages are unmodified.
  await access(resolve("dist/index.html"));
  let loadApp = false;
  const server = await preview({
    clearScreen: false,
    preview: { host: "127.0.0.1", port: 1420, strictPort: true },
    plugins: [
      {
        name: "feedback-smoke-setup",
        configurePreviewServer({ middlewares }) {
          middlewares.use((request, response, next) => {
            if (loadApp || request.url !== "/") return next();
            response.writeHead(200, {
              "Content-Type": "text/html; charset=utf-8",
              "Cache-Control": "no-store",
            });
            response.end("<!doctype html><title>Feedback smoke setup</title>");
          });
        },
      },
    ],
  });
  let app;
  let browser;
  let page;
  const events = [];
  try {
    app = spawn(executable, [], {
      env: {
        ...process.env,
        APPDATA: roaming,
        LOCALAPPDATA: local,
        MEOWCAL_LOG_DIR: logs,
        MEOWCAL_CORE_EXECUTABLE: resolve("src-tauri/resources/core/meowcal-core.exe"),
        WEBVIEW2_USER_DATA_FOLDER: userData,
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
    for (const context of browser.contexts()) {
      for (const view of context.pages()) observeWebView(view, events);
      context.on("page", (view) => observeWebView(view, events));
    }
    page = await waitFor(
      () =>
        browser
          .contexts()
          .flatMap((ctx) => ctx.pages())
          .find((p) => p.url() === origin + "/"),
      "the main WebView",
      app,
    );
    await page.waitForFunction(() => Boolean(globalThis.__TAURI__?.app), null, { polling: 200 });
    assert.equal(await page.evaluate(() => globalThis.__TAURI__.app.getIdentifier()), identifier);
    assert.equal(
      await page.evaluate(() => globalThis.__TAURI__.window.getCurrentWindow().label),
      "main",
    );
    const appDirectory = resolve(await page.evaluate(() => globalThis.__TAURI__.path.appDataDir()));
    assert.ok(
      configFolders.some((path) => resolve(path).toLowerCase() === appDirectory.toLowerCase()),
      "Unexpected native config directory",
    );
    const effectiveLocale = await page.evaluate(async () => {
      const invoke = globalThis.__TAURI__.core.invoke;
      globalThis.localStorage.setItem("meowcal.setupAcknowledged", "true");
      if ((await invoke("get_settings")).uiLanguage !== "zh-CN")
        await invoke("set_ui_language", { language: "zh-CN" });
      return (await invoke("get_settings")).uiLanguage;
    });
    assert.equal(effectiveLocale, "zh-CN");
    loadApp = true;
    await page.reload({ waitUntil: "load" });
    await page
      .screenshot({ path: join(output, "00-startup.png"), timeout: 5000 })
      .catch((error) => events.push({ kind: "startup-screenshot", detail: String(error) }));
    const version = await captureViews(page, join(appDirectory, "config.json"));
    const updaterNotes = await verifyNativeUpdateNotes(page, output, version);
    const enginePhase = await page.evaluate(
      async () => (await globalThis.TauriBridge.invoke("get_engine_status")).phase,
    );
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
          webView2Version: browser.version(),
          surface: "visible native Tauri main window (WebView2)",
          language: "zh-CN",
          fault: "Windows rejected a settings save to the isolated read-only config",
          submissionAttempted: false,
          updaterNotes,
          enginePhase,
        },
        null,
        2,
      ) + "\n",
    );
  } catch (error) {
    // Upload only the disposable process and named startup milestones; a full
    // session log or process command line can contain private information.
    const markers = [];
    for (const file of await readdir(logs)) {
      if (!file.endsWith(".log")) continue;
      const content = await readFile(join(logs, file), "utf8");
      for (const marker of [
        "Meowcal Sub starting up",
        "Setting up system tray",
        "Using the Tauri selector and overlay",
      ]) {
        if (content.includes(marker)) markers.push(marker);
      }
    }
    const startup = {
      architecture: arch(),
      windows: release(),
      runtimes,
      port,
      error: String(error),
      markers,
      events,
      views: await Promise.all(
        (browser?.contexts().flatMap((context) => context.pages()) ?? [])
          .filter((view) => view.url().startsWith(origin + "/"))
          .map(describeWebView),
      ),
    };
    if (loadApp && page && !page.isClosed())
      await page
        .screenshot({ path: join(output, "native-failure.png"), timeout: 5000 })
        .catch(() => {});
    if (app?.pid) {
      const inspect = `
        $root = Get-Process -Id ${app.pid} -ErrorAction SilentlyContinue
        $children = Get-CimInstance Win32_Process -Filter "ParentProcessId=${app.pid}" | Select-Object ProcessId, Name, SessionId
        $listeners = Get-NetTCPConnection -LocalPort ${port} -State Listen -ErrorAction SilentlyContinue | Select-Object LocalAddress, OwningProcess
        @{ app = ($root | Select-Object ProcessName,SessionId,MainWindowTitle,Responding); directChildren = @($children); listeners = @($listeners) } | ConvertTo-Json -Compress -Depth 4
      `;
      try {
        startup.processes = JSON.parse(
          execFileSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", inspect], {
            encoding: "utf8",
            timeout: 12000,
          }),
        );
      } catch {
        startup.processes = "Process inspection unavailable";
      }
    }
    console.error("Native startup diagnostics:", JSON.stringify(startup));
    await writeFile(join(output, "startup.json"), JSON.stringify(startup, null, 2) + "\n");
    throw error;
  } finally {
    if (page && !page.isClosed()) {
      await page
        .evaluate(() => {
          void globalThis.__TAURI__.core.invoke("quit_initial_setup");
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
