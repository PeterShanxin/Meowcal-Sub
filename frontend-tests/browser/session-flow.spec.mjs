import { expect, test as base } from "@playwright/test";

const test = base.extend({
  page: async ({ page }, use) => {
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await use(page);
    expect(errors).toEqual([]);
  },
});

// The native selector and capture are mocked; the main page, controller and bridge are real.
async function openMain(page, region = { x: 100, y: 180, width: 350, height: 100 }) {
  await page.addInitScript((savedRegion) => {
    localStorage.setItem("meowcal.onboardingComplete", "true");
    const listeners = new Map();
    window.sessionTest = {
      region: savedRegion,
      calls: [],
      selection: null,
      running: false,
      failStartup: false,
      sessionId: 0,
      emit(name, payload) {
        for (const listener of listeners.get(name) ?? []) listener({ payload });
      },
      confirm(region = this.region) {
        const selection = this.selection;
        this.selection = null;
        if (region) this.region = region;
        selection.resolve({ requestId: selection.requestId, region });
      },
    };
    window.__TAURI__ = {
      app: { getIdentifier: async () => "com.meowcal.sub.dev" },
      core: {
        async invoke(command, args) {
          const app = window.sessionTest;
          app.calls.push({ command, args });
          if (command === "get_settings") return { autoCheckUpdates: false };
          if (command === "get_ocr_languages") return [];
          if (command === "get_capture_region") return app.region;
          if (command === "get_display_context")
            return { multipleDisplays: app.multipleDisplays ?? false, region: app.region };
          if (command === "is_translation_running") return app.running;
          if (["get_engine_status", "refresh_engine_status", "make_engine_ready"].includes(command))
            return { phase: "ready" };
          if (command === "request_area_selection")
            return new Promise((resolve) => {
              app.selection = { ...args, resolve };
            });
          if (command === "start_translation") {
            app.running = true;
            app.sessionId += 2;
            app.emit("subtitle-activity", {
              sessionId: app.sessionId,
              captureId: 0,
              state: "active",
            });
            if (app.failStartup) {
              app.running = false;
              app.emit("capture-status", {
                isError: true,
                message: "Switch to the Compatibility profile to use Windows OCR.",
              });
              app.emit("translation-interrupted", { sessionId: app.sessionId });
            }
          }
          if (command === "stop_translation") app.running = false;
        },
      },
      event: {
        async listen(name, callback) {
          if (!listeners.has(name)) listeners.set(name, []);
          listeners.get(name).push(callback);
          return () => {};
        },
        async emit() {},
      },
    };
  }, region);
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Start translation", exact: true })).toBeEnabled();
}

async function startConfirmed(page) {
  await page.getByRole("button", { name: "Start translation", exact: true }).click();
  await expect.poll(() => page.evaluate(() => Boolean(window.sessionTest.selection))).toBe(true);
  await page.evaluate(() => window.sessionTest.confirm());
  await expect(page.getByRole("button", { name: "Stop translation", exact: true })).toBeEnabled();
}

test("cached crop needs explicit confirmation on every start, even after a long selection", async ({
  page,
}) => {
  await page.clock.install();
  await openMain(page);
  await page.getByRole("button", { name: "Start translation", exact: true }).click();
  await expect(page.getByRole("combobox", { name: "Original subtitle language" })).toBeDisabled();
  await page.clock.runFor(20_000);
  expect(
    await page.evaluate(() =>
      window.sessionTest.calls.filter((c) => c.command === "start_translation"),
    ),
  ).toHaveLength(0);
  await page.evaluate(() => window.sessionTest.confirm());
  await expect(page.getByRole("button", { name: "Stop translation", exact: true })).toBeEnabled();
  await page.getByRole("button", { name: "Stop translation", exact: true }).click();
  await page.getByRole("button", { name: "Start translation", exact: true }).click();
  await expect.poll(() => page.evaluate(() => Boolean(window.sessionTest.selection))).toBe(true);
  await page.evaluate(() => window.sessionTest.confirm(null));
  await expect(page.getByRole("button", { name: "Start translation", exact: true })).toBeEnabled();
  await expect(page.getByText("Subtitle area selected", { exact: true })).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.sessionTest.calls.filter((c) => c.command === "start_translation"),
    ),
  ).toHaveLength(1);
});

test("first use confirms and starts without returning for a second Start", async ({ page }) => {
  await openMain(page, null);
  await page.getByRole("button", { name: "Start translation", exact: true }).click();
  await expect.poll(() => page.evaluate(() => Boolean(window.sessionTest.selection))).toBe(true);
  await page.evaluate(() => window.sessionTest.confirm({ x: 80, y: 200, width: 300, height: 70 }));
  await expect(page.getByRole("button", { name: "Stop translation", exact: true })).toBeEnabled();
  expect(
    await page.evaluate(() =>
      window.sessionTest.calls.filter((c) => c.command === "start_translation"),
    ),
  ).toHaveLength(1);
});

test("waiting stays neutral, preserves Running and offers a single stop-confirm-resume flow", async ({
  page,
}) => {
  await page.clock.install();
  await openMain(page);
  await startConfirmed(page);
  await page.clock.runFor(5000);
  await page.evaluate(() => {
    const app = window.sessionTest;
    app.emit("subtitle-activity", { sessionId: app.sessionId, captureId: 60, state: "waiting" });
  });
  await expect(page.locator(".status-pill")).toHaveText("Running");
  await expect(page.locator(".support-line")).toContainText("Waiting for subtitles");
  await expect(page.getByRole("alert")).toHaveCount(0);
  await page.screenshot({ path: test.info().outputPath("home-waiting.png") });
  await page.getByRole("button", { name: /Subtitle area selected.*Check area/ }).click();
  await expect.poll(() => page.evaluate(() => Boolean(window.sessionTest.selection))).toBe(true);
  expect(await page.evaluate(() => window.sessionTest.running)).toBe(false);
  await page.evaluate(() => window.sessionTest.confirm());
  await expect(page.getByRole("button", { name: "Stop translation", exact: true })).toBeEnabled();
  await expect(page.locator(".support-line")).not.toContainText("Waiting for subtitles");
  expect(
    await page.evaluate(() =>
      window.sessionTest.calls.filter((c) => c.command === "start_translation"),
    ),
  ).toHaveLength(2);
});

test("capture failures remain visible until a healthy report proves recovery", async ({ page }) => {
  await openMain(page);
  await startConfirmed(page);
  await page.evaluate(() => {
    const app = window.sessionTest;
    app.emit("capture-status", { isError: true, message: "Capture is unavailable" });
    app.emit("subtitle-activity", { sessionId: app.sessionId, captureId: 60, state: "waiting" });
  });
  await expect(page.getByRole("alert")).toContainText("Capture is unavailable");
  await page.evaluate(() => {
    const app = window.sessionTest;
    app.emit("subtitle-activity", { sessionId: app.sessionId, captureId: 61, state: "active" });
  });
  await expect(page.getByRole("alert")).toHaveCount(0);
});

test("an interrupted startup preserves its recovery instruction and offers Start again", async ({
  page,
}) => {
  await openMain(page);
  await page.evaluate(() => (window.sessionTest.failStartup = true));
  await page.getByRole("button", { name: "Start translation", exact: true }).click();
  await expect.poll(() => page.evaluate(() => Boolean(window.sessionTest.selection))).toBe(true);
  await page.evaluate(() => window.sessionTest.confirm());
  await expect(page.getByRole("alert")).toContainText("Switch to the Compatibility profile");
  await expect(page.getByRole("button", { name: "Start translation", exact: true })).toBeEnabled();
  await expect(page.getByRole("button", { name: "Stop translation", exact: true })).toHaveCount(0);
  await page
    .getByRole("navigation", { name: "Main navigation" })
    .getByRole("button", { name: "Settings", exact: true })
    .click();
  await page.evaluate(() => window.sessionTest.emit("translation-exited", null));
  await expect(page.getByRole("button", { name: "Start translation", exact: true })).toBeEnabled();
  await expect(page.getByRole("alert")).toContainText("Switch to the Compatibility profile");
});

test("display targeting notice survives confirmation and invalidation still stops the session", async ({
  page,
}) => {
  await openMain(page);
  await page.evaluate(() => {
    window.sessionTest.multipleDisplays = true;
    window.sessionTest.emit("display-context-changed", null);
  });
  await expect(page.locator(".display-notice")).toHaveText(
    "检测到多个显示器，将使用当前窗口所在的屏幕。",
  );
  await startConfirmed(page);
  await expect(page.locator(".display-notice")).toHaveText("翻译期间将保持使用已选屏幕。");
  await page.evaluate(() => {
    window.sessionTest.running = false;
    window.sessionTest.region = null;
    window.sessionTest.emit(
      "display-invalidated",
      "The display changed. Select the subtitle area again.",
    );
  });
  await expect(page.getByRole("alert")).toContainText("The display changed");
  await expect(page.getByRole("button", { name: "Start translation", exact: true })).toBeEnabled();
  await expect(page.getByText("No subtitle area yet", { exact: true })).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.sessionTest.calls.filter((call) => call.command === "start_translation"),
    ),
  ).toHaveLength(1);
});
