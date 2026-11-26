import { test, expect } from "@playwright/test";

// The native 560x430 minimum window leaves about 546x422 for the page after its resize borders.
test.use({ viewport: { width: 546, height: 422 }, deviceScaleFactor: 1.25 });

test("the minimum native window shows the region button without scrolling", async ({ page }) => {
  await page.route("**/api/settings", (r) => r.fulfill({ json: { uiLanguage: "en" } }));
  await page.route("**/api/engine/status", (r) => r.fulfill({ json: { phase: "ready" } }));
  await page.route("**/api/ocr/languages", (r) => r.fulfill({ json: ["zh-CN", "en-US"] }));
  await page.goto("/");

  const button = page.getByRole("button", { name: "Select subtitle area" });
  await expect(button).toBeEnabled();
  const box = (await button.boundingBox())!;
  const nav = (await page.locator(".app-nav").boundingBox())!;
  expect(box.y + box.height).toBeLessThanOrEqual(nav.y);
});
