import { readFile } from "node:fs/promises";
import { chromium, expect } from "@playwright/test";
import { createServer } from "vite";

const config = JSON.parse(await readFile("src-tauri/tauri.conf.json", "utf8"));
const { width, height } = config.app.windows.find((window) => window.label === "main");
const server = await createServer({ server: { port: 0, strictPort: false } });

// Render the actual frontend with an in-memory ready state. No native process,
// installed profile, screen capture, model download or translation is involved.
async function captureHome(browser, defaultSettings, language, path) {
  const page = await browser.newPage({
    viewport: { width, height },
    deviceScaleFactor: 3,
    locale: language,
    reducedMotion: "reduce",
  });
  const settings = {
    ...structuredClone(defaultSettings),
    uiLanguage: language,
    sourceLanguage: "en-US",
    targetLanguage: "zh-CN",
    autoCheckUpdates: false,
  };
  await page.addInitScript(
    ({ settings, identifier, version }) => {
      const region = { x: 100, y: 600, width: 800, height: 100 };
      const responses = {
        get_settings: settings,
        get_system_ui_language: settings.uiLanguage,
        get_ocr_languages: ["en-US", "zh-CN"],
        get_engine_status: { phase: "ready" },
        get_capture_region: region,
        get_display_context: { multipleDisplays: false, region },
        is_translation_running: false,
        show_main_after_setup: null,
      };
      localStorage.setItem("meowcal.setupAcknowledged", "true");
      globalThis.window.__TAURI__ = {
        core: {
          invoke: async (command) => {
            if (!(command in responses))
              throw new Error(`Unexpected screenshot command: ${command}`);
            return responses[command];
          },
        },
        app: { getIdentifier: async () => identifier, getVersion: async () => version },
        event: { listen: async () => () => {} },
        window: { getCurrentWindow: () => ({ isMaximized: async () => false }) },
      };
    },
    { settings, identifier: config.identifier, version: config.version },
  );
  await page.goto(server.resolvedUrls.local[0]);
  await expect(
    page.getByRole("heading", { name: language === "en" ? "Let’s watch" : "开始观看吧" }),
  ).toBeVisible();
  await expect(page.locator(".home-saved-action")).toBeEnabled();
  await expect(page.locator(".titlebar-identity")).not.toContainText("Dev");
  await expect(page.locator("#browser-mode-indicator")).toHaveCount(0);
  await page.evaluate(() => globalThis.document.fonts.ready);
  await page.screenshot({ path, animations: "disabled" });
  await page.close();
}

const image = async (file, type = "png") =>
  `data:image/${type};base64,${(await readFile(file)).toString("base64")}`;
const logo = await image("docs/assets/logo.svg", "svg+xml");
let browser;
try {
  await server.listen();
  const { defaultSettings } = await server.ssrLoadModule("/ui/settings-defaults.ts");
  browser = await chromium.launch({ channel: process.env.MEOWCAL_BROWSER_CHANNEL || "chrome" });
  for (const chinese of [false, true]) {
    const suffix = chinese ? ".zh-CN" : "";
    await captureHome(
      browser,
      defaultSettings,
      chinese ? "zh-CN" : "en",
      `docs/assets/screenshot-home${suffix}.png`,
    );
    const screenshot = await image(`docs/assets/screenshot-home${suffix}.png`);
    const page = await browser.newPage({
      viewport: { width: 1600, height: 720 },
      deviceScaleFactor: 2,
    });
    await page.setContent(`<!doctype html><html lang="${chinese ? "zh-CN" : "en"}">
      <meta charset="utf-8"><style>
        * { box-sizing: border-box; }
        body { margin: 0; color: #f7f6fa; font-family: "Segoe UI", "Microsoft YaHei", sans-serif; }
        main { width: 1600px; height: 720px; display: flex; align-items: center; gap: 64px;
          padding: 76px; background: radial-gradient(ellipse at 16% 0, #292635 0, #17181f 42%, #101115 100%); }
        .copy { width: 644px; flex: none; }
        .logo { width: 104px; height: 104px; margin-bottom: 38px; }
        h1 { margin: 0 0 24px; font-size: ${chinese ? 52 : 62}px; line-height: 1.15; letter-spacing: -2px; font-weight: 650; }
        p { margin: 0; font-size: 32px; line-height: 1.55; color: #dedce6; }
        .privacy { margin-top: 24px; color: #aaa6b8; font-size: 22px; }
        .app { width: 740px; max-height: 568px; object-fit: contain; border-radius: 12px;
          box-shadow: 0 30px 70px #0009; border: 1px solid #ffffff21; }
      </style><main>
        <div class="copy"><img class="logo" src="${logo}" alt="">
          <h1>${chinese ? "Meowcal Sub · 妙影" : "Meowcal Sub"}</h1>
          <p>${chinese ? "把屏幕上的字幕，<br>翻译成你想读的语言。" : "Translate the subtitles<br>already on your screen."}</p>
          <p class="privacy">${chinese ? "全本地运行 · 保护隐私 · 零成本" : "Fully local. Private. Zero cost."}</p>
        </div>
        <img class="app" src="${screenshot}" alt="${chinese ? "当前应用界面" : "Current app interface"}">
      </main></html>`);
    await page.evaluate("document.fonts.ready");
    await page.locator(".app").evaluate((element) => element.decode());
    await page.screenshot({ path: `docs/assets/hero${suffix}.png` });
    await page.close();
  }
} finally {
  try {
    await browser?.close();
  } finally {
    await server.close();
  }
}
