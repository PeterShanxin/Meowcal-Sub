import { expect, test } from "@playwright/test";
import { defaultSettings } from "../../src/ui/settings-defaults";

async function mockSettings(page: import("@playwright/test").Page) {
  await page.route("**/api/ocr/languages", (route) =>
    route.fulfill({ json: ["zh-Hans-CN", "en-US"] }),
  );
  await page.route("**/api/engine/status", (route) => route.fulfill({ json: { phase: "ready" } }));
  await page.route("**/api/settings", (route) => route.fulfill({ json: defaultSettings }));
}

test("setup stays recoverable when initialization fails", async ({ page }) => {
  await mockSettings(page);
  let offline = true;
  await page.route("**/api/settings", (route) =>
    offline ? route.abort() : route.fulfill({ json: defaultSettings }),
  );
  await page.goto("/wizard.html");
  await expect(page.getByRole("alert")).toContainText("Couldn’t load settings");
  await expect(page.getByRole("button", { name: "Continue", exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Try again", exact: true }).click();
  await expect(page.getByRole("alert")).toBeVisible();
  offline = false;
  await page.getByRole("button", { name: "Try again", exact: true }).click();
  await page.getByRole("button", { name: "Continue", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Choose your languages" })).toBeFocused();
  await expect(page.getByRole("button", { name: "Prepare translation" })).toBeDisabled();
});

test("failed settings edits can be retried and survive reload", async ({ page }) => {
  await page.setViewportSize({ width: 560, height: 430 });
  await mockSettings(page);
  let failSave = true;
  let stored = structuredClone(defaultSettings);
  await page.route("**/api/settings", (route) => {
    if (route.request().method() === "POST") {
      if (failSave) return route.fulfill({ status: 503, json: { error: "Read-only volume" } });
      stored = route.request().postDataJSON();
    }
    return route.fulfill({ json: stored });
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  const continuity = page.getByRole("switch", { name: /^Keep names consistent/ });
  await continuity.check();
  await expect(page.getByRole("alert")).toContainText("Changes aren’t saved");
  const message = await page.getByRole("alert").boundingBox();
  const navigation = await page.getByRole("navigation").boundingBox();
  expect(message!.y + message!.height).toBeLessThan(navigation!.y);
  failSave = false;
  await page.getByRole("button", { name: "Retry save" }).click();
  await expect(page.getByRole("status")).toContainText("Settings saved");
  await expect(page.getByRole("alert")).toHaveCount(0);
  await page.reload();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(continuity).toBeChecked();
});

test("keyboard navigation enters the newly selected screen", async ({ page }) => {
  await mockSettings(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Subtitle style", exact: true }).press("Enter");
  await expect(page.getByRole("heading", { name: "Subtitle style" })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("slider", { name: "Text size" })).toBeFocused();
});

for (const width of [320, 390, 560, 1000]) {
  test(`controls fit at ${width}px and the preview badge stays clear of navigation`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: width === 560 ? 430 : 844 });
    await mockSettings(page);
    await page.goto("/");
    for (const screen of ["Home", "Subtitle style", "Settings"]) {
      await page.getByRole("button", { name: screen, exact: true }).click();
      const overflow = await page.locator("main").evaluate((el) => el.scrollWidth > el.clientWidth);
      expect(overflow).toBe(false);
      const badge = await page.locator("#browser-mode-indicator").boundingBox();
      const nav = await page.getByRole("navigation").boundingBox();
      expect(badge!.y + badge!.height).toBeLessThan(nav!.y);
    }
    await page.goto("/wizard.html");
    await page.getByRole("button", { name: "Continue", exact: true }).click();
    const footer = await page
      .locator(".wizard-footer")
      .evaluate((el) => ({ width: el.clientWidth, scroll: el.scrollWidth }));
    expect(footer.scroll).toBeLessThanOrEqual(footer.width);
  });
}

for (const phase of ["busy", "preparing"]) {
  test(`Settings presents ${phase} neutrally and disables engine actions`, async ({ page }) => {
    await mockSettings(page);
    await page.route("**/api/engine/status", (route) => route.fulfill({ json: { phase } }));
    await page.goto("/");
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    const engine = page.locator(".list-row", { hasText: "Translation engine" });
    await expect(engine.locator(".status-chip")).toHaveClass(/tone-neutral/);
    await expect(engine).toContainText(phase === "busy" ? "Busy" : "Preparing");
    await expect(engine).not.toContainText("Needs repair");
    await expect(engine.getByRole("button", { name: "Repair" })).toBeDisabled();
    await expect(engine.getByRole("button", { name: "Test", exact: true })).toBeDisabled();
    if (phase === "busy") {
      await page.getByRole("button", { name: "Home", exact: true }).click();
      await expect(page.getByRole("button", { name: "Start", exact: true })).toHaveCount(0);
    }
  });
}
