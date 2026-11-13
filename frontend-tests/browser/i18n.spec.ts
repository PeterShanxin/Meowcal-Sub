import { test, expect, type Page } from "@playwright/test";
import { defaultSettings } from "../../src/ui/settings-defaults";

async function settingsBackend(page: Page, language = "system", failLanguageSave = false) {
  let settings = { ...structuredClone(defaultSettings), uiLanguage: language };
  let settingsWrites = 0;
  await page.route("**/api/settings", (route) => {
    if (route.request().method() === "POST") {
      settingsWrites++;
      settings = { ...route.request().postDataJSON(), uiLanguage: settings.uiLanguage };
    }
    return route.fulfill({ json: settings });
  });
  await page.route("**/api/ui-language", (route) => {
    if (failLanguageSave) return route.fulfill({ status: 503, json: { error: "disk locked" } });
    settings.uiLanguage = route.request().postDataJSON().language;
    return route.fulfill({ json: { success: true } });
  });
  await page.route("**/api/engine/status", (route) => route.fulfill({ json: { phase: "ready" } }));
  await page.route("**/api/ocr/languages", (route) => route.fulfill({ json: ["zh-CN", "en-US"] }));
  return {
    stored: () => settings,
    settingsWrites: () => settingsWrites,
    changeSubtitlePair: () => {
      settings = { ...settings, sourceLanguage: "ja-JP", targetLanguage: "en-US" };
    },
    allowSave: () => {
      failLanguageSave = false;
    },
  };
}

test("language changes immediately, saves, survives reload, and leaves the subtitle pair unchanged", async ({
  page,
}) => {
  const backend = await settingsBackend(page, "en");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  backend.changeSubtitlePair(); // Simulate Setup saving a newer pair than this window's snapshot.
  await page.getByRole("combobox", { name: "App language", exact: true }).selectOption("zh-CN");
  await expect(page.getByRole("heading", { name: "设置", exact: true })).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");
  await expect(page.locator(".titlebar-identity")).toContainText("Meowcal Sub · 妙影");
  await expect.poll(() => backend.stored().uiLanguage).toBe("zh-CN");
  expect(backend.stored()).toMatchObject({ sourceLanguage: "ja-JP", targetLanguage: "en-US" });
  expect(backend.settingsWrites()).toBe(0);
  await page.reload();
  await expect(page.getByRole("heading", { name: "开始观看吧" })).toBeVisible();
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.getByRole("combobox", { name: "界面语言", exact: true }).selectOption("en");
  await expect(page.getByRole("heading", { name: "Settings", exact: true })).toBeVisible();
  await expect(page.locator(".titlebar-identity")).not.toContainText("妙影");
});

test("an unsaved language remains explicit and can be retried", async ({ page }) => {
  const backend = await settingsBackend(page, "en", true);
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("combobox", { name: "App language", exact: true }).selectOption("zh-CN");
  await expect(page.getByRole("alert")).toContainText("更改尚未保存");
  expect(backend.stored().uiLanguage).toBe("en");
  backend.allowSave();
  await page.getByRole("button", { name: "重新保存" }).click();
  await expect.poll(() => backend.stored().uiLanguage).toBe("zh-CN");
  await expect(page.getByRole("status")).toContainText("设置已保存");
  expect(backend.settingsWrites()).toBe(0);
});

test.describe("Chinese system language", () => {
  test.use({ locale: "zh-CN" });
  test("detects system language and honors an English override", async ({ page }) => {
    await settingsBackend(page);
    await page.goto("/");
    await expect(page.getByRole("heading", { name: "开始观看吧" })).toBeVisible();
    await page.getByRole("button", { name: "设置", exact: true }).click();
    await page.getByRole("combobox", { name: "界面语言", exact: true }).selectOption("en");
    await expect(page.getByRole("heading", { name: "Settings", exact: true })).toBeVisible();
  });
});

test("setup failures and in-flight progress relocalize, while sample text stays untouched", async ({
  page,
}) => {
  await settingsBackend(page, "zh-CN");
  await page.goto("/wizard.html");
  await expect(page.getByRole("heading", { name: "欢迎使用 Meowcal Sub · 妙影" })).toBeVisible();
  await page.evaluate(() => {
    const setup = document.querySelector("meowcal-setup") as unknown as {
      step: number;
      fail(error: string): void;
    };
    setup.step = 3;
    setup.fail("ENGINE_DOWNLOAD_FAILED");
  });
  await expect(page.getByRole("alert")).toContainText("设置未能完成");
  await expect(page.getByRole("alert")).toContainText("ENGINE_DOWNLOAD_FAILED");
  await page.evaluate(() => window.TauriBridge.event.emit("ui-language-changed", "en"));
  await expect(page.getByRole("alert")).toContainText("Setup couldn’t finish");
  await page.evaluate(() => {
    const setup = document.querySelector("meowcal-setup") as unknown as {
      working: boolean;
      error: null;
      progress: string;
    };
    setup.working = true;
    setup.error = null;
    setup.progress = "Model: 65%";
  });
  await expect(page.getByRole("status")).toContainText("Downloading model: 65%");
  await page.evaluate(() => window.TauriBridge.event.emit("ui-language-changed", "zh-CN"));
  await expect(page.getByRole("status")).toContainText("正在下载模型：65%");
  await page.evaluate(() => {
    const setup = document.querySelector("meowcal-setup") as unknown as {
      step: number;
      working: boolean;
      existingEngine: boolean;
      sampleSource: string;
      sample: { translatedText: string };
    };
    setup.step = 4;
    setup.working = false;
    setup.existingEngine = false;
    setup.sampleSource = "Keep this original subtitle.";
    setup.sample = { translatedText: "保留这条翻译字幕。" };
  });
  await expect(page.locator(".setup-sample small")).toHaveText("Keep this original subtitle.");
  await expect(page.locator(".setup-sample span")).toHaveText("保留这条翻译字幕。");
  await page.evaluate(() => window.TauriBridge.event.emit("ui-language-changed", "en"));
  await expect(page.getByRole("heading", { name: "Ready to watch", exact: true })).toBeVisible();
  await expect(page.locator(".setup-sample small")).toHaveText("Keep this original subtitle.");
  await expect(page.locator(".setup-sample span")).toHaveText("保留这条翻译字幕。");
});

for (const locale of ["en", "zh-CN"]) {
  test(`${locale} screens fit the minimum window without horizontal clipping`, async ({ page }) => {
    await settingsBackend(page, locale);
    await page.setViewportSize({ width: 560, height: 430 });
    await page.goto("/");
    for (const name of locale === "en"
      ? ["Home", "Subtitle style", "Settings"]
      : ["首页", "字幕样式", "设置"]) {
      await page.getByRole("button", { name, exact: true }).click();
      expect(await page.locator("main").evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(
        true,
      );
    }
    await page
      .getByRole("button", { name: locale === "en" ? "About" : "关于", exact: true })
      .click();
    expect(await page.locator("main").evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(
      true,
    );
  });
}
