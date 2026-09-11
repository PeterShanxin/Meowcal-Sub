import { backendOrigin, expect, test } from "./fixtures.mjs";

test("status notices do not intercept the primary action in a compact window", async ({ page }) => {
  await page.setViewportSize({ width: 544, height: 400 });
  await page.goto("/");
  await page.waitForFunction(() => document.querySelector("meowcal-app")?.snapshot.busy === "idle");
  await page.evaluate(() => {
    const app = document.querySelector("meowcal-app");
    app.snapshot = {
      ...app.snapshot,
      engine: { phase: "ready" },
      ocrLanguages: new Set([app.snapshot.settings.sourceLanguage]),
      region: { x: 10, y: 10, width: 200, height: 50 },
      notice: "Subtitle area selected",
    };
  });
  await expect(page.locator(".toast.notice")).toContainText("Subtitle area selected");
  await page
    .getByRole("button", { name: "Start translation", exact: true })
    .click({ trial: true, timeout: 2000 });
});

test("browser bridge reads backend health, settings, and readiness", async ({ page, request }) => {
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));

  await page.goto("/");
  await expect(page.locator("#browser-mode-indicator")).toBeVisible();

  const healthResponse = await request.get(`${backendOrigin}/api/health`);
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
  const response = await request.post(`${backendOrigin}/api/area-selector`);

  expect(response.status()).toBe(501);
  await expect(response.json()).resolves.toMatchObject({
    browserMode: true,
  });
});

test("normal setup presents one private HY-MT engine without infrastructure choices", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page).toHaveTitle("Meowcal Sub");
  await expect(page.getByRole("navigation", { name: "Main navigation" })).toBeVisible();
  await expect(page.getByLabel("Original subtitle language")).toBeVisible();
  await expect(page.getByLabel("Translation language")).toBeVisible();
  await expect(page.locator("body")).not.toContainText("Foundry Local");
  await expect(page.locator("body")).not.toContainText("Passthrough");
  await expect(page.locator("body")).not.toContainText("translateLocally");
  await expect(page.locator("body")).not.toContainText("Model ID");
  await expect(page.locator("body")).not.toContainText("endpoint");

  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByText("Keep running in the tray")).toHaveCount(0);
  await expect(page.getByText("Start with Windows")).toHaveCount(0);
  const autoCheckInput = page.getByLabel(/Automatically check for updates/i);
  await expect(autoCheckInput).toBeVisible();
  await expect(autoCheckInput).toBeChecked();
  await autoCheckInput.click();
  await expect(autoCheckInput).not.toBeChecked();
  await autoCheckInput.click();
  await expect(autoCheckInput).toBeChecked();

  // Settings nav indicator appears when update is available and clears when not
  await expect(page.locator(".nav-indicator")).toHaveCount(0);
  await page.evaluate(() => {
    const app = document.querySelector("meowcal-app");
    if (app && app.snapshot) {
      app.snapshot = {
        ...app.snapshot,
        update: { kind: "available", version: "0.6.10", notes: "New fixes" },
      };
    }
  });
  await expect(page.locator(".nav-indicator")).toBeVisible();
  await page.evaluate(() => {
    const app = document.querySelector("meowcal-app");
    if (app && app.snapshot) {
      app.snapshot = { ...app.snapshot, update: { kind: "upToDate" } };
    }
  });
  await expect(page.locator(".nav-indicator")).toHaveCount(0);

  await page.goto("/wizard.html");
  await expect(page.getByRole("heading", { name: "Welcome to Meowcal Sub" })).toBeVisible();
  await expect(page.getByText("Everything stays on this PC")).toBeVisible();
  await expect(page.locator("body")).not.toContainText("Foundry");
  await expect(page.locator("body")).not.toContainText("winget");
  await expect(page.locator("body")).not.toContainText("Model ID");
});

test("guided engine setup has one install action and no infrastructure choices", async ({
  page,
}) => {
  await page.goto("/wizard.html");

  await expect(page.getByRole("heading", { name: "Welcome to Meowcal Sub" })).toBeVisible();
  await expect(page.getByRole("button", { name: /Continue/ })).toBeVisible();
  await expect(page.locator("body")).not.toContainText(
    /\b(?:Foundry|endpoint|port|model ID|cache directory)\b|llama\.cpp/i,
  );
  await expect(page.locator(".step-dots i")).toHaveCount(4);
  await expect(page.getByRole("heading", { name: "Welcome to Meowcal Sub" })).toBeFocused();

  await page.getByRole("button", { name: /Continue/ }).click();
  await expect(page.getByRole("heading", { name: "Choose your languages" })).toBeVisible();
  await expect(page.getByLabel("Original subtitles")).toBeVisible();
  await expect(page.getByLabel("Translate into")).toBeVisible();
});
