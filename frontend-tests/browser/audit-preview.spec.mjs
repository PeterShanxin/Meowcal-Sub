import { expect, test } from "@playwright/test";
import { createServer } from "vite";

let preview;
let origin;

test.beforeAll(async () => {
  preview = await createServer({ configFile: "frontend-tests/ui-audit/vite.config.mjs" });
  await preview.listen(0);
  origin = preview.resolvedUrls.local[0];
});

test.afterAll(async () => {
  await preview?.close();
});

test("the audit preview keeps app language across a stale settings save and reload", async ({
  page,
  request,
}) => {
  const settingsUrl = new URL("/__ui-audit/api/settings", origin).href;
  const previous = await (await request.get(settingsUrl)).json();
  await page.goto(origin);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("combobox", { name: "App language" }).selectOption("zh-CN");
  await expect(page.getByRole("status")).toContainText("设置已保存");
  await request.post(settingsUrl, { data: { ...previous, sourceLanguage: "ja-JP" } });
  await page.reload();
  await expect(page.getByRole("heading", { name: "开始观看吧" })).toBeVisible();
  await expect(page.getByRole("combobox").first()).toHaveValue("ja-JP");
});

test("the preview explains where to select a subtitle area", async ({ page, request }) => {
  await request.post(new URL("/__ui-audit/api/ui-language", origin).href, {
    data: { language: "en" },
  });
  await page.goto(origin);
  await page.getByRole("button", { name: "Select subtitle area" }).click();
  await expect(page.getByRole("alert")).toContainText("require the Windows app");
});
