import { test, expect } from "@playwright/test";

test.use({
  viewport: { width: 560, height: 430 },
  deviceScaleFactor: 1.5,
  launchOptions: { args: ["--blink-settings=minimumFontSize=20"] },
});

test.beforeEach(async ({ page }) => {
  await page.route("**/api/settings", (r) => r.fulfill({ json: { uiLanguage: "en" } }));
  await page.route("**/api/engine/status", (r) => r.fulfill({ json: { phase: "ready" } }));
  await page.route("**/api/ocr/languages", (r) => r.fulfill({ json: ["zh-CN", "en-US"] }));
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Select subtitle area" })).toBeEnabled();
});

test("the preview badge leaves the feedback button visible", async ({ page }) => {
  const feedback = page.getByRole("button", { name: "Feedback", exact: true });
  const button = await feedback.boundingBox();
  const badge = await page.locator("#browser-mode-indicator").boundingBox();
  expect(button!.x + button!.width).toBeLessThanOrEqual(badge!.x);
  await feedback.press("Enter");
  await expect(page.getByRole("dialog", { name: "Feedback" })).toBeVisible();
});

test("the selected subtitle language fits beside its arrow", async ({ page }) => {
  const select = page.getByRole("combobox", { name: "Original subtitle language" });
  const widths = await select.evaluate((element: HTMLSelectElement) => {
    const style = getComputedStyle(element);
    const context = document.createElement("canvas").getContext("2d")!;
    context.font = style.font;
    return {
      label: context.measureText(element.selectedOptions[0].text.trim()).width,
      available:
        element.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight),
    };
  });
  expect(widths.label).toBeLessThanOrEqual(widths.available);
});
