import { readFile } from "node:fs/promises";
import { chromium } from "@playwright/test";

const image = async (file, type = "png") =>
  `data:image/${type};base64,${(await readFile(file)).toString("base64")}`;
const logo = await image("docs/assets/logo.svg", "svg+xml");
const browser = await chromium.launch({ channel: process.env.MEOWCAL_BROWSER_CHANNEL || "chrome" });
try {
  for (const chinese of [false, true]) {
    const suffix = chinese ? ".zh-CN" : "";
    const screenshot = await image(`docs/assets/screenshot-home${suffix}.png`);
    const page = await browser.newPage({
      viewport: { width: 1600, height: 720 },
      deviceScaleFactor: 1,
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
        .platform { margin-top: 52px; color: #aaa6b8; font-size: 20px; letter-spacing: 0.5px; }
        .app { width: 740px; max-height: 568px; object-fit: contain; border-radius: 12px;
          box-shadow: 0 30px 70px #0009; border: 1px solid #ffffff21; }
      </style><main>
        <div class="copy"><img class="logo" src="${logo}" alt="">
          <h1>${chinese ? "Meowcal Sub · 妙影" : "Meowcal Sub"}</h1>
          <p>${chinese ? "把屏幕上的字幕，<br>翻译成你想读的语言。" : "Translate the subtitles<br>already on your screen."}</p>
          <p class="privacy">${chinese ? "本地识别 · 本地翻译 · 字幕留在你的电脑上" : "Local OCR. Local translation. Your words stay on your PC."}</p>
          <p class="platform">Windows 11 &nbsp;·&nbsp; x64 / ARM64</p>
        </div>
        <img class="app" src="${screenshot}" alt="${chinese ? "真实应用截图" : "Actual app screenshot"}">
      </main></html>`);
    await page.evaluate("document.fonts.ready");
    await page.locator(".app").evaluate((element) => element.decode());
    await page.screenshot({ path: `docs/assets/hero${suffix}.png` });
    await page.close();
  }
} finally {
  await browser.close();
}
