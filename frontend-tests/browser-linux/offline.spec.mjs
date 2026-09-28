import { expect, test as base } from "@playwright/test";

const test = base.extend({
  page: async ({ page }, use) => {
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await use(page);
    expect(errors, "No uncaught exceptions while using the real frontend").toEqual([]);
  },
});

test("main navigation remains usable with an offline backend", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveTitle("Meowcal Sub");
  await expect(page.locator("#browser-mode-indicator")).toBeVisible();
  await expect(page.getByRole("status")).toHaveText("Backend offline");
  await expect(page.getByRole("status")).toHaveAttribute("aria-live", "polite");
  await expect(page.getByRole("button", { name: "Backend unavailable" })).toBeDisabled();
  await expect(page.getByRole("combobox", { name: "Original subtitle language" })).toBeVisible();
  await expect(page.getByRole("combobox", { name: "Translation language" })).toBeVisible();
  expect(await page.evaluate(() => globalThis.TauriBridge.API_BASE)).toBe(
    `http://127.0.0.1:${process.env.MEOWCAL_HTTP_PORT}/api`,
  );

  const navigation = page.getByRole("navigation", { name: "Main navigation" });
  await expect(navigation).toBeInViewport();
  const settings = navigation.getByRole("button", { name: "Settings", exact: true });
  await settings.focus();
  await expect(settings).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(settings).toHaveAttribute("aria-current", "page");
  await expect(page.getByRole("heading", { name: "Settings", exact: true })).toBeVisible();
  const engine = page.locator(".list-row", { hasText: "Translation engine" });
  await expect(engine).toContainText("Backend offline");
  await expect(engine.getByRole("button", { name: "Test", exact: true })).toBeDisabled();
  await expect(engine.getByRole("button", { name: /Repair|Set up/ })).toHaveCount(0);
  await expect(engine).not.toContainText("Installed and working");

  await navigation.getByRole("button", { name: "Subtitle style", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Subtitle style", exact: true })).toBeVisible();
  await expect(page.getByRole("region", { name: "Subtitle preview" })).toBeVisible();
  await expect(page.getByRole("slider", { name: /Text size/ })).toBeVisible();
  await navigation.getByRole("button", { name: "Home", exact: true }).click();
  await expect(page.getByRole("status")).toHaveText("Backend offline");
  await expect(page.getByRole("button", { name: "Start translation", exact: true })).toHaveCount(0);
});

test("a native capture request cannot report success without the backend", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("status")).toHaveText("Backend offline");
  await page.getByRole("button", { name: "No subtitle area yet Select" }).click();
  await expect(page.getByRole("alert")).toBeVisible();
  await expect(page.getByRole("alert")).not.toBeEmpty();
  await expect(page.getByRole("button", { name: "No subtitle area yet Select" })).toBeVisible();
  await expect(page.getByText("Subtitle area selected", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Backend unavailable" })).toBeDisabled();
  await page.getByRole("button", { name: "Dismiss", exact: true }).click();
  await expect(page.getByRole("alert")).toHaveCount(0);
});

test("setup navigation never claims OCR installation or translation readiness offline", async ({
  page,
}) => {
  await page.goto("/wizard.html");
  await expect(page).toHaveTitle("Set up Meowcal Sub");
  await expect(page.getByRole("heading", { name: "Welcome to Meowcal Sub" })).toBeVisible();
  await expect(page.getByRole("alert")).toBeVisible();
  await expect(page.getByRole("alert")).not.toBeEmpty();

  const next = page.getByRole("button", { name: "Continue", exact: true });
  await expect(next).toBeInViewport();
  await next.focus();
  await expect(next).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("button", { name: "Install recognition", exact: true }),
  ).toBeDisabled();
  await expect(page.getByRole("button", { name: "Prepare translation" })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Ready to watch" })).toHaveCount(0);
  await expect(page.locator(".status-chip.tone-success")).toHaveCount(0);
  await page.getByRole("button", { name: "Back", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Welcome to Meowcal Sub" })).toBeVisible();
  await expect(page.getByRole("alert")).toBeVisible();
});
