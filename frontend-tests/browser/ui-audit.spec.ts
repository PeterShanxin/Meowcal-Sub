import { expect, test } from "@playwright/test";
import { defaultSettings } from "../../src/ui/settings-defaults";

test("setup advances after the install command completes", async ({ page }) => {
  await mockSettings(page);
  await page.goto("/wizard.html");
  await page.getByRole("button", { name: "Continue", exact: true }).click();
  await page.evaluate(() => {
    const original = window.TauriBridge.invoke;
    window.TauriBridge.invoke = (async (command: string, args?: Record<string, unknown>) => {
      if (["save_settings", "wizard_install_engine", "wizard_start_service"].includes(command))
        return undefined;
      if (command === "refresh_engine_status") return { phase: "ready", serviceRunning: true };
      if (command === "wizard_test_translation") return { translatedText: "Hello" };
      return original(command, args);
    }) as typeof original;
  });
  await page.getByRole("checkbox", { name: /I agree/ }).check();
  await page.getByRole("button", { name: "Set up", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Ready to watch" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Open app" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Select subtitle area" })).toHaveCount(0);
});

test("existing engine shows a completion screen and opens the app", async ({ page }) => {
  await mockSettings(page);
  await page.goto("/wizard.html");
  await page.evaluate(() => {
    const setup = document.querySelector("meowcal-setup") as unknown as {
      reset(existing: boolean): void;
    };
    const original = window.TauriBridge.invoke;
    window.TauriBridge.invoke = (async (command: string, args?: Record<string, unknown>) => {
      if (command === "close_engine_wizard") {
        (window as unknown as { closeArgs: unknown }).closeArgs = args;
        return undefined;
      }
      return original(command, args);
    }) as typeof original;
    setup.reset(true);
  });
  await expect(page.getByRole("heading", { name: "Existing setup detected" })).toBeVisible();
  await expect(page.locator(".setup-sample")).toHaveCount(0);
  const footer = await page.locator(".wizard-footer").boundingBox();
  const open = await page.getByRole("button", { name: "Open app" }).boundingBox();
  expect(open!.x + open!.width).toBeGreaterThan(footer!.x + footer!.width - 30);
  await page.getByRole("button", { name: "Open app" }).click();
  await expect
    .poll(() => page.evaluate(() => (window as unknown as { closeArgs: unknown }).closeArgs))
    .toMatchObject({ modelDownloaded: true });
});

async function mockSettings(page: import("@playwright/test").Page) {
  await page.route("**/api/ocr/languages", (route) =>
    route.fulfill({ json: ["zh-Hans-CN", "en-US"] }),
  );
  await page.route("**/api/engine/status", (route) => route.fulfill({ json: { phase: "ready" } }));
  await page.route("**/api/settings", (route) => route.fulfill({ json: defaultSettings }));
}

test("setup ignores a repeated install request while installation is pending", async ({ page }) => {
  await mockSettings(page);
  await page.goto("/wizard.html");
  await page.getByRole("button", { name: "Continue", exact: true }).click();
  await page.getByRole("checkbox", { name: /I agree/ }).check();
  const requests = await page.evaluate(async () => {
    const setup = document.querySelector("meowcal-setup") as unknown as {
      beginEngineSetup(): Promise<void>;
    };
    let installs = 0;
    const original = window.TauriBridge.invoke;
    window.TauriBridge.invoke = ((command: string, args?: Record<string, unknown>) => {
      if (command === "save_settings") return Promise.resolve();
      if (command === "wizard_install_engine") {
        installs++;
        return new Promise(() => {});
      }
      return original(command, args);
    }) as typeof original;
    void setup.beginEngineSetup();
    void setup.beginEngineSetup();
    await Promise.resolve();
    return installs;
  });
  expect(requests).toBe(1);
});

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
  await expect(page.getByRole("heading", { name: "Set up the translation model" })).toBeFocused();
  await expect(page.getByRole("button", { name: "Set up", exact: true })).toBeDisabled();
});

test("setup displays Core download progress without inventing completed steps", async ({
  page,
}) => {
  await mockSettings(page);
  await page.goto("/wizard.html");
  await page.getByRole("button", { name: "Continue", exact: true }).click();
  await page.evaluate(async () => {
    const listeners = new Map<string, (event: { payload: unknown }) => void>();
    window.TauriBridge.event.listen = async (name, callback) => {
      listeners.set(name, callback);
      return () => listeners.delete(name);
    };
    const original = window.TauriBridge.invoke;
    window.TauriBridge.invoke = ((command: string, args?: Record<string, unknown>) => {
      if (command === "wizard_install_engine") return new Promise(() => {});
      return original(command, args);
    }) as typeof original;
    const setup = document.querySelector("meowcal-setup") as unknown as {
      listenForSetup(): Promise<void>;
    };
    await setup.listenForSetup();
    (window as unknown as { emitSetupProgress: (line: string) => void }).emitSetupProgress = (
      line,
    ) => listeners.get("wizard-output")?.({ payload: { line } });
  });
  await page.getByRole("checkbox", { name: /I agree/ }).check();
  await page.getByRole("button", { name: "Set up", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Setting up the app" })).toBeVisible();
  await page.evaluate(() =>
    (window as unknown as { emitSetupProgress: (line: string) => void }).emitSetupProgress(
      "Model: 65%",
    ),
  );
  await expect(page.getByRole("status")).toHaveText("Downloading model: 65%");
  await expect(page.getByText("1 of 5")).toHaveCount(0);
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
  await page.getByRole("button", { name: "About", exact: true }).click({ trial: true });
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
    await expect(engine.locator(".status-chip")).toHaveCount(0);
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

for (const width of [400, 900]) {
  for (const [phase, label] of [
    ["ready", "Ready"],
    ["error", "Needs repair"],
  ] as const) {
    test(`Settings and About at ${width}px with engine ${phase}`, async ({ page }) => {
      await page.setViewportSize({ width, height: 700 });
      await mockSettings(page);
      await page.route("**/api/engine/status", (route) => route.fulfill({ json: { phase } }));
      await page.goto("/");
      await page.getByRole("button", { name: "Settings", exact: true }).click();
      const engine = page.locator(".list-row", { hasText: "Translation engine" });
      await expect(engine.locator("small")).toHaveText(label);
      await expect(engine.locator(".status-chip")).toHaveCount(0);
      await expect(engine.getByRole("button", { name: "Repair" })).toBeVisible();
      await expect(engine.getByRole("button", { name: "Test", exact: true })).toBeEnabled({
        enabled: phase === "ready",
      });
      await page.getByRole("button", { name: "About", exact: true }).click();
      await expect(page.getByRole("heading", { name: "About" })).toBeVisible();
      await expect(page.getByRole("link", { name: "Meowcal Sub on GitHub" })).toHaveAttribute(
        "href",
        "https://github.com/PeterShanxin/Meowcal-Sub",
      );
      await expect(page.getByText(/Not affiliated with/)).toBeVisible();
      await page.getByRole("button", { name: "Settings", exact: true }).first().click();
      await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
      expect(await page.locator(".page").evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(
        true,
      );
    });
  }
}
