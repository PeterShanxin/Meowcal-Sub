import { expect, test } from "@playwright/test";

test("browser bridge reads backend health, settings, and readiness", async ({ page, request }) => {
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));

  await page.goto("/");
  await expect(page.locator("#browser-mode-indicator")).toBeVisible();

  const healthResponse = await request.get("http://127.0.0.1:3001/api/health");
  expect(healthResponse.ok()).toBe(true);
  await expect(healthResponse.json()).resolves.toMatchObject({
    status: "ok",
    browserMode: true,
  });

  const flow = await page.evaluate(async () => {
    const settings = await window.TauriBridge.invoke("get_settings");
    const diagnostics = await window.TauriBridge.invoke("get_translation_diagnostics");

    return {
      browserMode: window.TauriBridge.isBrowserMode(),
      settings,
      diagnostics,
    };
  });

  expect(flow.browserMode).toBe(true);
  expect(flow.settings).toEqual(
    expect.objectContaining({
      sourceLanguage: expect.any(String),
      targetLanguage: expect.any(String),
    }),
  );
  expect(flow.diagnostics).toEqual(
    expect.objectContaining({
      backends: expect.any(Array),
    }),
  );
  expect(pageErrors).toEqual([]);
});

test("browser mode reports Tauri-only capture as unavailable", async ({ request }) => {
  const response = await request.post("http://127.0.0.1:3001/api/area-selector");

  expect(response.status()).toBe(501);
  await expect(response.json()).resolves.toMatchObject({
    browserMode: true,
  });
});

test("normal setup presents one private HY-MT engine without infrastructure choices", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: /Local Translation Engine/ })).toBeVisible();
  await expect(page.getByText(/Tencent HY-MT runs privately/)).toBeVisible();
  await expect(page.locator("body")).not.toContainText("Foundry Local");
  await expect(page.locator("body")).not.toContainText("Passthrough");
  await expect(page.locator("body")).not.toContainText("translateLocally");
  await expect(page.locator("body")).not.toContainText("Model ID");
  await expect(page.locator("body")).not.toContainText("endpoint");

  await page.goto("/wizard.html");
  await expect(page.getByText("Private subtitle translation")).toBeVisible();
  await expect(page.locator("body")).toContainText("Tencent HY-MT");
  await expect(page.locator("body")).not.toContainText("Foundry");
  await expect(page.locator("body")).not.toContainText("winget");
  await expect(page.locator("body")).not.toContainText("Model ID");
});

test("guided engine setup has one install action and no infrastructure choices", async ({
  page,
}) => {
  await page.goto("/wizard.html");

  await expect(page.getByRole("heading", { name: "Private subtitle translation" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Install translation engine" })).toBeVisible();
  await expect(page.locator("body")).not.toContainText(
    /\b(?:Foundry|endpoint|port|model ID|cache directory)\b|llama\.cpp/i,
  );
  await expect(page.locator(".wizard-step-item")).toHaveCount(3);
  await expect(page.getByRole("heading", { name: "Private subtitle translation" })).toBeFocused();
});
