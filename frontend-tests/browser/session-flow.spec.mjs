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
    localStorage.setItem("meowcal.setupAcknowledged", "true");
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
      confirm(region = this.region, startTranslation = true) {
        const selection = this.selection;
        this.selection = null;
        if (region) this.region = region;
        selection.resolve({ requestId: selection.requestId, region, startTranslation });
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
  await expect(
    page.getByRole("button", { name: "Select subtitle area", exact: true }),
  ).toBeEnabled();
  await expect(page.locator(".status-pill, .home-screen .dot, .support-line")).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Let’s watch", exact: true })).toBeVisible();
  await expect(page.locator(".home-description")).toHaveCount(0);
  await expect(page.getByText("Select subtitle area to start")).toHaveCount(0);
}

async function startConfirmed(page) {
  await page.getByRole("button", { name: "Select subtitle area", exact: true }).click();
  await expect.poll(() => page.evaluate(() => Boolean(window.sessionTest.selection))).toBe(true);
  await page.evaluate(() => window.sessionTest.confirm());
  await expect(page.getByRole("button", { name: "Stop translation", exact: true })).toBeEnabled();
}

test("saved area can start directly while selecting remains available", async ({ page }) => {
  await openMain(page);
  await expect(page.getByRole("button", { name: "Select subtitle area" })).toBeVisible();
  await page.getByRole("button", { name: "Start with saved area" }).click();
  await expect(page.getByRole("button", { name: "Stop translation" })).toBeEnabled();
  expect(
    await page.evaluate(() =>
      window.sessionTest.calls.some((c) => c.command === "request_area_selection"),
    ),
  ).toBe(false);
});

for (const width of [320, 900]) {
  test(`saved area actions fit at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 700 });
    await openMain(page);
    const select = await page.getByRole("button", { name: "Select subtitle area" }).boundingBox();
    const start = await page.getByRole("button", { name: "Start with saved area" }).boundingBox();
    expect(Math.abs(select.width - start.width)).toBeLessThan(1);
    expect(select.x).toBeGreaterThanOrEqual(0);
    expect(start.x + start.width).toBeLessThanOrEqual(width);
    if (width === 320) expect(start.y).toBeGreaterThanOrEqual(select.y + select.height);
    else expect(start.y).toBe(select.y);
  });
}

for (const width of [320, 560, 900]) {
  test(`Home elements stay aligned across session and engine states at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 700 });
    await openMain(page);
    const coordinates = () =>
      page.locator(".home-title, .language-pair, .home-actions").evaluateAll((elements) =>
        elements.map((element) => {
          const { x, y } = element.getBoundingClientRect();
          return { x, y };
        }),
      );
    const baseline = await coordinates();
    const ocrSettings = await page.evaluate(() => {
      const settings = structuredClone(
        document.querySelector("meowcal-app").controller.current().settings,
      );
      settings.translation.ocr.engine = "windows";
      settings.sourceLanguage = "ko-KR";
      return settings;
    });
    const cases = [
      [{ busy: "loading" }, "Getting things ready"],
      [{ busy: "selecting" }, "Let’s watch"],
      [{ busy: "warming" }, "Starting translation"],
      [{ busy: "starting" }, "Starting translation"],
      [{ running: true, notice: "Translation started" }, "Subtitles are live"],
      [{ running: true, busy: "stopping" }, "Subtitles are live"],
      [{ notice: "Translation stopped" }, "Let’s watch"],
      [{ engine: { phase: "notInstalled" } }, "Set up private translation"],
      [{ engine: { phase: "busy" } }, "Waiting for the engine"],
      [{ engine: { phase: "preparing" } }, "Warming up translation"],
      [{ engine: { phase: "error" }, error: "Engine check failed" }, "Translation needs a repair"],
      [{ engine: { phase: "backendUnavailable" } }, "Start the browser backend"],
      [{ multipleDisplays: true }, "Let’s watch"],
      [{ settings: ocrSettings, ocrLanguages: [] }, "Install the recognition language"],
    ];
    for (const [patch, title] of cases) {
      await page.evaluate((next) => {
        const controller = document.querySelector("meowcal-app").controller;
        if (next.ocrLanguages) next.ocrLanguages = new Set(next.ocrLanguages);
        controller.publish({
          busy: "idle",
          running: false,
          notice: null,
          error: null,
          multipleDisplays: false,
          engine: { phase: "ready" },
          ...next,
        });
      }, patch);
      await expect(page.locator("#home-title")).toHaveText(title);
      expect(await coordinates(), title).toEqual(baseline);
    }
  });
}

test("a notification leaves the running controls clear in a short window", async ({ page }) => {
  await page.setViewportSize({ width: 560, height: 430 });
  await openMain(page);
  await page.evaluate(() =>
    document
      .querySelector("meowcal-app")
      .controller.publish({ running: true, notice: "Translation started" }),
  );
  const action = await page.getByRole("button", { name: "Stop translation" }).boundingBox();
  const toast = await page.locator(".toast").boundingBox();
  expect(action.y + action.height).toBeLessThanOrEqual(toast.y);
});

test("cached crop needs explicit confirmation on every start, even after a long selection", async ({
  page,
}) => {
  await page.clock.install();
  await openMain(page);
  await page.getByRole("button", { name: "Select subtitle area", exact: true }).click();
  await expect(page.locator(".home-screen")).toHaveAttribute("inert", "");
  await page.clock.runFor(20_000);
  expect(
    await page.evaluate(() =>
      window.sessionTest.calls.filter((c) => c.command === "start_translation"),
    ),
  ).toHaveLength(0);
  await page.evaluate(() => window.sessionTest.confirm());
  await expect(page.getByRole("button", { name: "Stop translation", exact: true })).toBeEnabled();
  await page.getByRole("button", { name: "Stop translation", exact: true }).click();
  await page.getByRole("button", { name: "Select subtitle area", exact: true }).click();
  await expect.poll(() => page.evaluate(() => Boolean(window.sessionTest.selection))).toBe(true);
  await page.evaluate(() => window.sessionTest.confirm(null));
  await expect(
    page.getByRole("button", { name: "Select subtitle area", exact: true }),
  ).toBeEnabled();
  await expect(page.getByText("Select subtitle area to start")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.sessionTest.calls.filter((c) => c.command === "start_translation"),
    ),
  ).toHaveLength(1);
});

for (const region of [null, { x: 100, y: 180, width: 350, height: 100 }]) {
  test(`selection keeps Home in place with ${region ? "a saved area" : "no saved area"}`, async ({
    page,
  }) => {
    await openMain(page, region);
    const home = page.locator(".home-screen");
    const boxes = () =>
      page.locator("#home-title, .language-pair, .home-actions").evaluateAll((elements) =>
        elements.map((element) => {
          const { x, y, width, height } = element.getBoundingClientRect();
          return { x, y, width, height };
        }),
      );
    const before = await boxes();
    await page.getByRole("button", { name: "Select subtitle area" }).click();
    await expect(home).toHaveAttribute("inert", "");
    await expect(home.locator(".home-action")).toContainText("Confirm in the selector…");
    await expect(home.locator(".home-action")).toBeDisabled();
    expect(await boxes()).toEqual(before);
    await page.evaluate(() => window.sessionTest.confirm(null));
    await expect(home).not.toHaveAttribute("inert", "");
    expect(await boxes()).toEqual(before);
  });
}

test("first use confirms and starts without returning for a second Start", async ({ page }) => {
  await openMain(page, null);
  await expect(page.getByRole("button", { name: "Start with saved area" })).toHaveCount(0);
  await page.getByRole("button", { name: "Select subtitle area", exact: true }).click();
  await expect.poll(() => page.evaluate(() => Boolean(window.sessionTest.selection))).toBe(true);
  await page.evaluate(() => window.sessionTest.confirm({ x: 80, y: 200, width: 300, height: 70 }));
  await expect(page.getByRole("button", { name: "Stop translation", exact: true })).toBeEnabled();
  expect(
    await page.evaluate(() =>
      window.sessionTest.calls.filter((c) => c.command === "start_translation"),
    ),
  ).toHaveLength(1);
});

test("saving an area exits without starting translation", async ({ page }) => {
  await openMain(page, null);
  await page.getByRole("button", { name: "Select subtitle area", exact: true }).click();
  await expect.poll(() => page.evaluate(() => Boolean(window.sessionTest.selection))).toBe(true);
  await page.evaluate(() =>
    window.sessionTest.confirm({ x: 80, y: 200, width: 300, height: 70 }, false),
  );
  await expect(
    page.getByRole("button", { name: "Select subtitle area", exact: true }),
  ).toBeEnabled();
  await expect(page.getByText("Select subtitle area to start")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.sessionTest.calls.filter((c) => c.command === "start_translation"),
    ),
  ).toHaveLength(0);
});

test("healthy waiting preserves Home's running controls and support text", async ({ page }) => {
  await page.clock.install();
  await openMain(page);
  await startConfirmed(page);
  await page.clock.runFor(5000);
  await page.evaluate(() => {
    const app = window.sessionTest;
    app.emit("subtitle-activity", { sessionId: app.sessionId, captureId: 60, state: "waiting" });
  });
  await expect(page.getByRole("heading", { name: "Subtitles are live" })).toBeVisible();
  await expect(page.locator(".support-line")).toContainText("Overlay active · Local processing");
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(page.locator(".region-row")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Stop translation", exact: true })).toBeEnabled();
  expect(await page.evaluate(() => window.sessionTest.running)).toBe(true);
  expect(
    await page.evaluate(() =>
      window.sessionTest.calls.filter((c) => c.command === "request_area_selection"),
    ),
  ).toHaveLength(1);
});

test("capture failures remain visible until a healthy report proves recovery", async ({ page }) => {
  await openMain(page);
  await startConfirmed(page);
  await page.evaluate(() => {
    const app = window.sessionTest;
    app.emit("capture-status", { isError: true, message: "Capture is unavailable" });
    app.emit("subtitle-activity", {
      sessionId: app.sessionId - 1,
      captureId: 60,
      state: "waiting",
    });
  });
  await expect(page.getByRole("alert")).toContainText("Capture is unavailable");
  await page.evaluate(() => {
    const app = window.sessionTest;
    app.emit("subtitle-activity", { sessionId: app.sessionId, captureId: 61, state: "waiting" });
  });
  await expect(page.getByRole("alert")).toHaveCount(0);
});

test("an interrupted startup preserves its recovery instruction and offers Start again", async ({
  page,
}) => {
  await openMain(page);
  await page.evaluate(() => (window.sessionTest.failStartup = true));
  await page.getByRole("button", { name: "Select subtitle area", exact: true }).click();
  await expect.poll(() => page.evaluate(() => Boolean(window.sessionTest.selection))).toBe(true);
  await page.evaluate(() => window.sessionTest.confirm());
  await expect(page.getByRole("alert")).toContainText("Switch to the Compatibility profile");
  await expect(
    page.getByRole("button", { name: "Select subtitle area", exact: true }),
  ).toBeEnabled();
  await expect(page.getByRole("button", { name: "Stop translation", exact: true })).toHaveCount(0);
  await page
    .getByRole("navigation", { name: "Main navigation" })
    .getByRole("button", { name: "Settings", exact: true })
    .click();
  await page.evaluate(() => window.sessionTest.emit("translation-exited", null));
  await expect(
    page.getByRole("button", { name: "Select subtitle area", exact: true }),
  ).toBeEnabled();
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
    "Multiple displays detected. The screen containing this window will be used.",
  );
  await startConfirmed(page);
  await expect(page.locator(".display-notice")).toHaveText(
    "The selected screen stays in use during translation.",
  );
  await page.evaluate(() => {
    window.sessionTest.running = false;
    window.sessionTest.region = null;
    window.sessionTest.emit(
      "display-invalidated",
      "The display changed. Select the subtitle area again.",
    );
  });
  await expect(page.getByRole("alert")).toContainText("The display changed");
  await expect(
    page.getByRole("button", { name: "Select subtitle area", exact: true }),
  ).toBeEnabled();
  await expect(page.getByText("Select subtitle area to start")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.sessionTest.calls.filter((call) => call.command === "start_translation"),
    ),
  ).toHaveLength(1);
});

test("logging is opt-in and shows the captured-content warning", async ({ page }) => {
  await openMain(page);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  const level = page.getByRole("combobox", { name: "Log level", exact: true });
  await expect(level).toHaveValue("basic");
  await expect(page.locator("#logging-privacy")).toContainText("Operational diagnostics only.");
  await level.selectOption("verbose");
  await expect(page.locator("#logging-privacy")).toContainText("Saves captures, OCR images");
  await expect(page.locator("#logging-privacy")).toContainText("Content may be private");
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.sessionTest.calls.filter(({ command }) => command === "save_settings").at(-1)?.args
            .settings.logging.level,
      ),
    )
    .toBe("verbose");
  await level.selectOption("off");
  await expect(level).toHaveValue("off");
  await expect(page.locator("#logging-privacy")).toContainText("No new logs or captures.");
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.sessionTest.calls.filter(({ command }) => command === "save_settings").at(-1)?.args
            .settings.logging.level,
      ),
    )
    .toBe("off");
});
