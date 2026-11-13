import { mkdir, readFile, writeFile } from "node:fs/promises";
import { chromium, expect } from "@playwright/test";
import { createServer } from "vite";
import sharp from "sharp";

const output = "docs/assets/store";
await mkdir(output, { recursive: true });
const config = JSON.parse(await readFile("src-tauri/tauri.conf.json", "utf8"));
const main = config.app.windows.find((window) => window.label === "main");
const server = await createServer({ server: { port: 0, strictPort: false } });
const scene = `data:image/webp;base64,${(await readFile(`${output}/coast.webp`)).toString("base64")}`;
const samples = {
  en: {
    source: "海面很平静，我们还有时间。",
    translation: "The sea is quiet. We still have time.",
  },
  "zh-CN": {
    source: "The sea is quiet. We still have time.",
    translation: "海面很平静，我们还有时间。",
  },
};
const region = { x: 240, y: 400, width: 800, height: 70 };
let browser;

// The real frontend receives deterministic sample state, as in the README
// capture. Only the demonstration film and desktop background are composed.
async function openApp(settings, url = "/", desktop = false, snapshot = null) {
  const page = await browser.newPage({
    viewport: desktop ? { width: 1280, height: 720 } : { width: main.width, height: main.height },
    deviceScaleFactor: 2,
    locale: settings.uiLanguage,
    reducedMotion: "reduce",
  });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.addInitScript(
    ({ settings, config, region, snapshot }) => {
      const listeners = new Map();
      const responses = {
        get_settings: settings,
        get_system_ui_language: settings.uiLanguage,
        get_ocr_languages: ["en-US", "zh-CN"],
        get_engine_status: { phase: "ready" },
        get_capture_region: region,
        get_display_context: { multipleDisplays: false, region },
        get_area_selection_request: { startTranslation: true },
        get_selector_snapshot: snapshot ? { dataUrl: snapshot, selectionId: 1 } : null,
        is_translation_running: false,
        show_main_after_setup: null,
        set_overlay_click_through: null,
        set_overlay_window_clip: null,
      };
      const windowApi = {
        isMaximized: async () => false,
        scaleFactor: async () => 1,
        innerPosition: async () => ({ x: 0, y: 0 }),
        setBackgroundColor: async () => {},
        setFocus: async () => {},
      };
      localStorage.setItem("meowcal.setupAcknowledged", "true");
      globalThis.window.__TAURI__ = {
        core: {
          invoke: async (command) => {
            if (!(command in responses)) throw new Error(`Unexpected capture command: ${command}`);
            return responses[command];
          },
        },
        app: {
          getIdentifier: async () => "com.meowcal.sub.store",
          getVersion: async () => config.version,
        },
        event: {
          listen: async (name, callback) => {
            if (!listeners.has(name)) listeners.set(name, []);
            listeners.get(name).push(callback);
            return () => {};
          },
          emit: async (name, payload) => {
            for (const callback of listeners.get(name) ?? []) await callback({ payload });
          },
        },
        window: { getCurrentWindow: () => windowApi, cursorPosition: async () => ({ x: 0, y: 0 }) },
        webview: { getCurrentWebview: () => windowApi },
      };
      globalThis.window.captureListeners = listeners;
    },
    { settings, config, region, snapshot },
  );
  await page.goto(new URL(url, server.resolvedUrls.local[0]).href);
  await page.evaluate(() => globalThis.document.fonts.ready);
  return { page, errors };
}

async function save(page, path, errors = []) {
  await page.evaluate(() => globalThis.document.fonts.ready);
  if (errors.length) throw new Error(errors.join("\n"));
  const png = await page.screenshot({ animations: "disabled", omitBackground: true });
  await sharp(png).png({ compressionLevel: 9 }).toFile(path);
}

async function desktopCard(capture, path) {
  const page = await browser.newPage({
    viewport: { width: 1280, height: 720 },
    deviceScaleFactor: 2,
  });
  const image = `data:image/png;base64,${capture.toString("base64")}`;
  await page.setContent(
    `<style>body{margin:0;width:1280px;height:720px;display:grid;place-items:center;background:radial-gradient(ellipse at 18% 8%,#353044,#20232d 53%,#181c23)}img{width:${main.width}px;height:${main.height}px;border-radius:10px;box-shadow:0 24px 80px #0008;outline:1px solid #ffffff20}</style><img src="${image}" alt="">`,
  );
  await page.locator("img").evaluate((image) => image.decode());
  await save(page, path);
  await page.close();
}

async function film(source) {
  const page = await browser.newPage({
    viewport: { width: 1280, height: 720 },
    deviceScaleFactor: 2,
  });
  await page.setContent(
    `<style>body{margin:0;background:#090d15;width:1280px;height:720px;font-family:'Segoe UI',sans-serif}img{width:1280px;height:720px;object-fit:cover}p{position:absolute;top:409px;left:0;width:100%;text-align:center;margin:0;color:white;font-size:28px;font-weight:500;text-shadow:0 2px 4px #000,0 0 12px #000}</style><img src="${scene}" alt=""><p>${source}</p>`,
  );
  await page.locator("img").evaluate((image) => image.decode());
  await page.evaluate(() => globalThis.document.fonts.ready);
  const png = await page.screenshot();
  await page.close();
  return png;
}

try {
  await server.listen();
  const { defaultSettings } = await server.ssrLoadModule("/ui/settings-defaults.ts");
  browser = await chromium.launch({ channel: process.env.MEOWCAL_BROWSER_CHANNEL || "msedge" });
  for (const language of ["en", "zh-CN"]) {
    const zh = language === "zh-CN";
    const { source, translation } = samples[language];
    const filmPng = await film(source);
    const snapshot = `data:image/png;base64,${filmPng.toString("base64")}`;
    const settings = {
      ...structuredClone(defaultSettings),
      uiLanguage: language,
      sourceLanguage: zh ? "en-US" : "zh-CN",
      targetLanguage: zh ? "zh-CN" : "en-US",
      autoCheckUpdates: false,
    };
    const { page, errors } = await openApp(settings);
    await expect(
      page.getByRole("heading", { name: zh ? "开始观看吧" : "Let’s watch" }),
    ).toBeVisible();
    await expect(page.locator(".titlebar-identity")).not.toContainText("Dev");
    await expect(page.locator("#browser-mode-indicator")).toHaveCount(0);
    await desktopCard(await page.screenshot(), `${output}/${language}-02-home.png`);
    await page
      .getByRole("button", { name: zh ? "字幕样式" : "Subtitle style", exact: true })
      .click();
    await expect(
      page.getByRole("heading", { name: zh ? "字幕样式" : "Subtitle style", exact: true }),
    ).toBeVisible();
    await desktopCard(await page.screenshot(), `${output}/${language}-04-style.png`);
    await page.getByRole("button", { name: zh ? "设置" : "Settings", exact: true }).click();
    await expect(
      page.getByRole("combobox", { name: zh ? "界面语言" : "App language", exact: true }),
    ).toHaveValue(language);
    await desktopCard(await page.screenshot(), `${output}/${language}-05-settings.png`);
    if (errors.length) throw new Error(errors.join("\n"));
    await page.close();

    const selector = await openApp(settings, "/selector.html", true, snapshot);
    await expect(selector.page.locator("#save-exit-btn")).toBeVisible();
    await expect(selector.page.locator("#desktop-snapshot")).toHaveJSProperty("complete", true);
    await save(selector.page, `${output}/${language}-03-selection.png`, selector.errors);
    await selector.page.close();

    const overlay = await openApp(settings, "/overlay.html", true);
    await overlay.page.waitForFunction(() =>
      globalThis.window.captureListeners.has("overlay-settings-updated"),
    );
    await overlay.page.evaluate(
      async ({ source, translation }) => {
        await globalThis.window.__TAURI__.event.emit("overlay-visibility", true);
        await globalThis.window.__TAURI__.event.emit("translation-update", {
          original: source,
          translated: translation,
          timestamp: Date.now(),
          backendUsed: "local",
          displayState: "translated",
          warnings: [],
        });
      },
      { source, translation },
    );
    await expect(overlay.page.locator("#subtitle-text")).toHaveText(translation);
    await expect(overlay.page.locator("#subtitle-container")).toBeVisible();
    const overlayPng = await overlay.page.screenshot({
      omitBackground: true,
      animations: "disabled",
    });
    if (overlay.errors.length) throw new Error(overlay.errors.join("\n"));
    await sharp(filmPng)
      .composite([{ input: overlayPng }])
      .png({ compressionLevel: 9 })
      .toFile(`${output}/${language}-01-overlay.png`);
    await overlay.page.close();
  }
  await writeFile(
    `${output}/capture.json`,
    `${JSON.stringify({ appVersion: config.version, viewport: [1280, 720], scale: 2, appViewport: [main.width, main.height], samples, method: "Actual frontend with deterministic sample state; original demonstration film background." }, null, 2)}\n`,
  );
  console.log("Captured 10 localized Store screenshots at 2560 × 1440.");
} finally {
  await browser?.close();
  await server.close();
}
