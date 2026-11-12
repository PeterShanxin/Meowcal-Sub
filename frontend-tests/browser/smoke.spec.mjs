import { backendOrigin, expect, test } from "./fixtures.mjs";

test("title bar distinguishes development builds in both distribution channels", async ({
  page,
}) => {
  await page.goto("/");
  await page.waitForFunction(() => customElements.get("meowcal-titlebar"));
  for (const [identifier, expected] of [
    ["com.meowcal.sub", "Meowcal Sub"],
    ["com.meowcal.sub.dev", "Meowcal Sub - Dev"],
    ["com.meowcal.sub.store", "Meowcal Sub"],
    ["com.meowcal.sub.store.dev", "Meowcal Sub - Dev"],
  ]) {
    await page.evaluate((profile) => {
      window.TauriBridge.appIdentifier = async () => profile;
      document.querySelector('[data-testid="profile-titlebar"]')?.remove();
      const titlebar = document.createElement("meowcal-titlebar");
      titlebar.dataset.testid = "profile-titlebar";
      document.body.append(titlebar);
    }, identifier);
    await expect(page.getByTestId("profile-titlebar").locator(".titlebar-identity")).toHaveText(
      expected,
    );
  }
});

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
    .getByRole("button", { name: "Select subtitle area", exact: true })
    .click({ trial: true });
});

test("the multi-display hint is quiet, fits a compact Home and explains session pinning", async ({
  page,
}) => {
  await page.setViewportSize({ width: 544, height: 400 });
  await page.goto("/");
  await page.waitForFunction(() => document.querySelector("meowcal-app")?.snapshot.busy === "idle");
  await expect(page.locator(".display-notice")).toHaveCount(0);
  await page.evaluate(() => {
    const app = document.querySelector("meowcal-app");
    app.snapshot = {
      ...app.snapshot,
      multipleDisplays: true,
      engine: { phase: "ready" },
      region: { x: 100, y: 200, width: 600, height: 80 },
    };
  });
  await expect(page.locator(".display-notice")).toHaveText(
    "检测到多个显示器，将使用当前窗口所在的屏幕。",
  );
  await page
    .getByRole("button", { name: "Select subtitle area", exact: true })
    .click({ trial: true });
  await page.evaluate(() => {
    const app = document.querySelector("meowcal-app");
    app.snapshot = { ...app.snapshot, running: true };
  });
  await expect(page.locator(".display-notice")).toHaveText("翻译期间将保持使用已选屏幕。");
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

test("offline browser backend does not offer engine repair", async ({ page }) => {
  await page.route("**/api/engine/status", (route) => route.abort());
  await page.goto("/");
  await expect(
    page.getByRole("button", { name: "Backend unavailable", exact: true }),
  ).toBeDisabled();

  await page.getByRole("button", { name: "Settings" }).click();
  const engine = page.locator(".list-row", { hasText: "Translation engine" });
  await expect(engine).toContainText("Backend offline");
  await expect(engine.getByRole("button", { name: "Repair" })).toHaveCount(0);
});

test("browser mode reports Tauri-only capture as unavailable", async ({ request }) => {
  const response = await request.post(`${backendOrigin}/api/area-selector`);

  expect(response.status()).toBe(501);
  await expect(response.json()).resolves.toMatchObject({
    browserMode: true,
  });
  const display = await request.get(`${backendOrigin}/api/display-context`);
  expect(display.status()).toBe(501);
  await expect(display.json()).resolves.toMatchObject({ browserMode: true });
});

test("browser mode refuses native translation exit", async ({ page, request }) => {
  const response = await request.post(`${backendOrigin}/api/translation/exit`);
  expect(response.status()).toBe(501);
  await expect(response.json()).resolves.toMatchObject({ browserMode: true });
  await page.goto("/");
  const error = await page.evaluate(async () => {
    try {
      await window.TauriBridge.invoke("exit_translation");
    } catch (reason) {
      return reason.message;
    }
  });
  expect(error).toContain("Translation control requires Tauri runtime");
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
  await page.getByRole("button", { name: "About", exact: true }).click();
  await expect(page.getByRole("heading", { name: "About" })).toBeFocused();
  await expect(page.getByRole("link", { name: "Meowcal Sub on GitHub" })).toHaveAttribute(
    "href",
    "https://github.com/PeterShanxin/Meowcal-Sub",
  );
  await expect(page.getByRole("link", { name: "Shanxin Li (@PeterShanxin)" })).toBeVisible();
  await expect(page.getByText(/Not affiliated with/)).toBeVisible();
  const modelTerms = page.locator(".model-license summary");
  await expect(page.getByText(/The model license excludes use/)).toBeHidden();
  await modelTerms.click();
  await expect(page.getByLabel("Tencent HY Community License Agreement")).toBeVisible();
  await modelTerms.click();
  await page.getByRole("button", { name: "Settings", exact: true }).first().click();
  await expect(page.getByRole("heading", { name: "Settings" })).toBeFocused();
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
    app.controller.publish({
      update: { kind: "available", version: "0.6.10", notes: "New fixes" },
    });
  });
  await expect(page.locator(".nav-indicator")).toBeVisible();
  await page.evaluate(() => {
    const app = document.querySelector("meowcal-app");
    app.controller.publish({ update: { kind: "upToDate" } });
  });
  await expect(page.locator(".nav-indicator")).toHaveCount(0);

  await page.goto("/wizard.html");
  await expect(page.getByRole("heading", { name: "Welcome to Meowcal Sub" })).toBeVisible();
  await expect(page.locator("body")).not.toContainText("Foundry");
  await expect(page.locator("body")).not.toContainText("winget");
  await expect(page.locator("body")).not.toContainText("Model ID");
});

test("guided engine setup has one install action and no infrastructure choices", async ({
  page,
}) => {
  await page.setViewportSize({ width: 680, height: 500 });
  await page.goto("/wizard.html");

  await expect(page.getByRole("heading", { name: "Welcome to Meowcal Sub" })).toBeVisible();
  await expect(page.getByRole("button", { name: /Continue/ })).toBeVisible();
  await expect(page.locator("body")).not.toContainText(
    /\b(?:Foundry|endpoint|port|model ID|cache directory)\b|llama\.cpp/i,
  );
  await expect(page.locator(".step-dots i")).toHaveCount(4);
  await expect(page.getByRole("heading", { name: "Welcome to Meowcal Sub" })).toBeFocused();

  await page.getByRole("button", { name: /Continue/ }).click();
  await expect(page.getByRole("heading", { name: "Set up the translation model" })).toBeVisible();
  await expect(page.getByLabel("Original subtitles")).toHaveCount(0);
  await expect(page.getByLabel("Translate into")).toHaveCount(0);
  const consent = page.getByRole("checkbox", { name: /I agree to the model license/ });
  await expect(consent).not.toBeChecked();
  await expect(page.getByText(/Not affiliated with/)).toBeVisible();
  const contact = page.getByRole("link", { name: "Shanxin Li (@PeterShanxin)" });
  await expect(contact).toBeVisible();
  await expect(contact).toHaveAttribute("href", "https://github.com/PeterShanxin");
  await expect(contact).toHaveAttribute("target", "_blank");
  const terms = page.locator(".model-license summary");
  const restrictions = page.getByText(/The model license excludes use/);
  await expect(terms).toHaveText("Model license and usage terms");
  await expect(terms).toBeInViewport({ ratio: 1 });
  await expect(consent).toBeInViewport({ ratio: 1 });
  await expect(restrictions).toBeHidden();
  await terms.focus();
  await page.keyboard.press("Enter");
  await expect(restrictions).toBeVisible();
  await expect(page.locator('a[href^="mailto:"]')).toHaveCount(0);
  await expect(page.getByLabel("Tencent HY Community License Agreement")).toContainText(
    "ACCEPTABLE USE POLICY",
  );
  await page.keyboard.press("Enter");
  await expect(restrictions).toBeHidden();
  // Exercise the UI gate without downloading a model or accepting terms for a real user.
  const prepare = page.getByRole("button", { name: "Set up", exact: true });
  await expect(prepare).toBeDisabled();
  await consent.check();
  await expect(prepare).toBeEnabled();
  await consent.uncheck();
  await expect(prepare).toBeDisabled();
});
