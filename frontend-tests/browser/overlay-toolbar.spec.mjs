import { expect, test as base } from "@playwright/test";

const test = base.extend({
  page: async ({ page }, use) => {
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.addInitScript(() => {
      window.overlayErrors = [];
      window.addEventListener("error", (event) => window.overlayErrors.push(event.message));
    });
    await use(page);
    expect(errors).toEqual([]);
    expect(await page.evaluate(() => window.overlayErrors)).toEqual([]);
  },
});

// Native IPC is the test boundary. These checks exercise the real overlay DOM,
// styling and clip payloads, but do not establish Windows window behaviour.
async function openOverlay(page, region = { x: 100, y: 180, width: 350, height: 100 }) {
  await page.addInitScript((captureRegion) => {
    const listeners = new Map();
    window.overlayTest = {
      region: captureRegion,
      calls: [],
      exitFailure: null,
      pendingExit: null,
      cursor: { x: captureRegion.x * devicePixelRatio, y: captureRegion.y * devicePixelRatio },
      emit(name, payload) {
        for (const listener of listeners.get(name) ?? []) listener({ payload });
      },
    };
    window.__TAURI__ = {
      core: {
        async invoke(command, args) {
          window.overlayTest.calls.push({ command, args });
          if (command === "get_capture_region") return window.overlayTest.region;
          if (command === "get_settings") return { overlay: {} };
          if (command === "exit_translation") {
            if (window.overlayTest.exitFailure) throw new Error(window.overlayTest.exitFailure);
            await new Promise((resolve) => (window.overlayTest.pendingExit = resolve));
          }
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
      webview: { getCurrentWebview: () => ({ setBackgroundColor: async () => {} }) },
      window: {
        getCurrentWindow: () => ({
          scaleFactor: async () => devicePixelRatio,
          innerPosition: async () => ({ x: 0, y: 0 }),
        }),
        cursorPosition: async () => window.overlayTest.cursor,
      },
    };
  }, region);
  await page.goto("/overlay.html");
  await expect.poll(() => page.locator("#capture-frame").getAttribute("style")).toContain("width");
  await page.evaluate(() => window.overlayTest.emit("overlay-visibility", true));
  await page.locator("#capture-frame").hover({ position: { x: 20, y: 10 } });
}

test("exit is keyboard accessible, ignores repeat activation and resets for the next session", async ({
  page,
}) => {
  await openOverlay(page);
  const exit = page.getByRole("button", { name: "Exit translation", exact: true });
  await expect(exit).toBeVisible();
  await exit.focus();
  await page.keyboard.press("Enter");
  await expect(exit).toBeDisabled();
  await page.keyboard.press("Enter");
  expect(
    await page.evaluate(
      () => window.overlayTest.calls.filter((c) => c.command === "exit_translation").length,
    ),
  ).toBe(1);
  await page.evaluate(() => {
    window.overlayTest.emit("overlay-visibility", false);
    window.overlayTest.pendingExit();
  });
  await expect(page.locator("#overlay-toolbar")).toBeHidden();
  await expect(page.getByRole("dialog")).toBeHidden();
  await page.evaluate(() => window.overlayTest.emit("overlay-visibility", true));
  await page.locator("#capture-frame").hover({ position: { x: 20, y: 10 } });
  await expect(exit).toBeEnabled();
});

test("failed exit stays usable with an error and a working retry", async ({ page }) => {
  await openOverlay(page);
  await page.evaluate(() => (window.overlayTest.exitFailure = "Capture is still stopping."));
  const exit = page.getByRole("button", { name: "Exit translation", exact: true });
  await exit.click();
  await expect(page.getByRole("alert")).toContainText("Capture is still stopping.");
  await expect(exit).toBeEnabled();
  await expect(page.locator("#capture-frame")).not.toHaveClass(/dragging/);
  await page.evaluate(() => (window.overlayTest.exitFailure = null));
  await exit.click();
  await expect(page.getByRole("alert")).toBeHidden();
  await expect(exit).toBeDisabled();
});

test("native close requests use the same guarded exit action", async ({ page }) => {
  await openOverlay(page);
  await page.evaluate(() => {
    window.overlayTest.emit("overlay-exit-requested");
    window.overlayTest.emit("overlay-exit-requested");
  });
  await expect(page.getByRole("button", { name: "Exit translation", exact: true })).toBeDisabled();
  expect(
    await page.evaluate(
      () => window.overlayTest.calls.filter((c) => c.command === "exit_translation").length,
    ),
  ).toBe(1);
});

for (const scale of [1, 1.25, 2]) {
  test.describe(`Overlay at ${scale}x scale`, () => {
    test.use({ deviceScaleFactor: scale });
    test("style controls retain stable geometry, clear resize handles and belong to the native clip", async ({
      page,
    }) => {
      await openOverlay(page);
      const toolbar = page.locator("#overlay-toolbar");
      const style = page.getByRole("button", { name: "Subtitle style", exact: true });
      await style.click();
      await expect(style).toHaveAttribute("aria-expanded", "true");
      await expect(page.getByRole("dialog")).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(page.getByRole("dialog")).toBeHidden();
      await expect(style).toBeFocused();
      const geometry = await toolbar.evaluate((element) => {
        const rect = element.getBoundingClientRect();
        const frame = document.querySelector("#capture-frame");
        return {
          x: Math.round(rect.x),
          y: Math.round(rect.y),
          width: Math.round(rect.width),
          height: Math.round(rect.height),
          transform: getComputedStyle(frame).transform,
          filter: getComputedStyle(frame).filter,
          buttons: [...element.querySelectorAll("button")].map(
            (b) => b.getBoundingClientRect().width,
          ),
        };
      });
      expect(geometry.transform).toBe("none");
      expect(geometry.filter).toBe("none");
      expect(geometry.buttons.every((width) => width >= 32)).toBe(true);
      await expect
        .poll(() =>
          page.evaluate(({ x, y, width, height }) => {
            const clips = window.overlayTest.calls.filter(
              (c) => c.command === "set_overlay_window_clip",
            );
            return clips
              .at(-1)
              ?.args.handleBounds?.some(
                (b) => b.x === x && b.y === y && b.width === width && b.height === height,
              );
          }, geometry),
        )
        .toBe(true);
      await expect(toolbar).toBeInViewport();
      const box = await toolbar.boundingBox();
      for (const handle of await page.locator(".resize-handle").all()) {
        const handleBox = await handle.boundingBox();
        expect(
          box.x + box.width <= handleBox.x ||
            handleBox.x + handleBox.width <= box.x ||
            box.y + box.height <= handleBox.y ||
            handleBox.y + handleBox.height <= box.y,
        ).toBe(true);
      }
      await toolbar.screenshot({
        path: test.info().outputPath("overlay-toolbar.png"),
        omitBackground: true,
      });
    });
  });
}

test("controls fit a small region at the screen edge and fade without leaving click targets", async ({
  page,
}) => {
  await page.clock.install();
  await openOverlay(page, { x: 0, y: 0, width: 50, height: 50 });
  const toolbar = page.locator("#overlay-toolbar");
  await expect(toolbar).toBeInViewport();
  expect((await toolbar.boundingBox()).x).toBeGreaterThan(57);
  await page.evaluate(() => {
    window.overlayTest.cursor = { x: 2000, y: 2000 };
    document.activeElement?.blur();
  });
  await page.mouse.move(400, 300);
  await page.clock.runFor(4700);
  await expect(toolbar).toBeHidden();
  await page.evaluate(() => {
    window.overlayTest.cursor = { x: 20, y: 20 };
  });
  await page.clock.runFor(300);
  await expect(toolbar).toBeVisible();
});

test("wide top-edge capture keeps the toolbar entirely outside source pixels", async ({ page }) => {
  const width = page.viewportSize().width;
  await openOverlay(page, { x: 0, y: 0, width, height: 100 });
  const toolbar = page.locator("#overlay-toolbar");
  await expect(toolbar).toBeInViewport();
  expect((await toolbar.boundingBox()).y).toBeGreaterThan(100);
  const status = page.locator("#overlay-session-status");
  await expect(status).toBeInViewport();
  expect((await status.boundingBox()).y).toBeGreaterThan(100);
});

test("capture covering the viewport hides controls and recovers when space is available", async ({
  page,
}) => {
  const { width, height } = page.viewportSize();
  await openOverlay(page, { x: 0, y: 0, width, height });
  await expect(page.locator("#overlay-toolbar")).toBeHidden();
  await expect(page.locator("#overlay-session-status")).toBeHidden();
  await page.evaluate(() =>
    window.overlayTest.emit("overlay-update-region", {
      x: 100,
      y: 180,
      width: 350,
      height: 100,
    }),
  );
  await expect(page.locator("#overlay-toolbar")).toBeVisible();
  await expect(page.locator("#overlay-session-status")).toBeVisible();
});

test("keyboard window activation reveals faded controls and preserves Tab order", async ({
  page,
}) => {
  await page.clock.install();
  await openOverlay(page);
  await page.evaluate(() => {
    window.overlayTest.cursor = { x: 2000, y: 2000 };
  });
  await page.mouse.move(500, 400);
  await page.clock.runFor(4700);
  await expect(page.locator("#overlay-toolbar")).toBeHidden();
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expect(page.getByRole("button", { name: "Subtitle style", exact: true })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "Exit translation", exact: true })).toBeFocused();
  await page.clock.runFor(4700);
  await expect(page.locator("#overlay-toolbar")).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "Exit translation", exact: true })).toBeDisabled();
});

test("waiting persists until source subtitles return and can appear again for a later gap", async ({
  page,
}) => {
  await page.clock.install();
  await openOverlay(page);
  await page.evaluate(() => {
    window.overlayTest.emit("subtitle-activity", { sessionId: 1, captureId: 1, state: "active" });
    window.overlayTest.emit("translation-update", {
      sessionId: 1,
      captureId: 2,
      displayState: "warming",
      translated: "",
      timestamp: Date.now(),
    });
  });
  const status = page.locator("#overlay-session-status");
  await expect(status).toContainText("Preparing translation…");
  await page.evaluate(() =>
    window.overlayTest.emit("translation-update", {
      sessionId: 1,
      captureId: 3,
      displayState: "noSubtitleText",
      translated: "",
      timestamp: Date.now(),
    }),
  );
  await expect(status).toBeHidden();
  await page.evaluate(() =>
    window.overlayTest.emit("translation-update", {
      sessionId: 1,
      captureId: 4,
      displayState: "warming",
      translated: "",
      timestamp: Date.now(),
    }),
  );
  await expect(status).toContainText("Preparing translation…");
  await page.evaluate(() =>
    window.overlayTest.emit("subtitle-activity", {
      sessionId: 1,
      captureId: 60,
      state: "waiting",
    }),
  );
  await expect(status).toContainText("Waiting for subtitles…");
  await expect(status).not.toHaveClass(/has-error/);
  await page.clock.runFor(20_000);
  await expect(status).toBeVisible();
  await page.evaluate(() => window.overlayTest.emit("capture-status", { isError: true }));
  await expect(status).toHaveClass(/has-error/);
  await page.evaluate(() =>
    window.overlayTest.emit("subtitle-activity", {
      sessionId: 1,
      captureId: 61,
      state: "waiting",
    }),
  );
  await expect(status).toContainText("Waiting for subtitles…");
  await expect(status).not.toHaveClass(/has-error/);
  await page.evaluate(() =>
    window.overlayTest.emit("subtitle-activity", {
      sessionId: 1,
      captureId: 62,
      state: "active",
    }),
  );
  await expect(status).toBeHidden();
  await page.evaluate(() =>
    window.overlayTest.emit("translation-update", {
      sessionId: 1,
      captureId: 63,
      displayState: "noSubtitleText",
      translated: "",
      timestamp: Date.now(),
    }),
  );
  await expect(status).toBeHidden();
  await page.evaluate(() =>
    window.overlayTest.emit("subtitle-activity", {
      sessionId: 1,
      captureId: 123,
      state: "waiting",
    }),
  );
  await expect(status).toContainText("Waiting for subtitles…");
  await expect(page.locator("#subtitle-text")).toHaveText("");
});

test("normal subtitle gaps and transient diagnostics leave the movie quiet", async ({ page }) => {
  await openOverlay(page);
  await page.evaluate(() =>
    window.overlayTest.emit("translation-update", {
      sessionId: 1,
      captureId: 1,
      displayState: "translated",
      translated: "A readable subtitle",
      backendUsed: "local_engine",
      warnings: [],
      timestamp: Date.now(),
    }),
  );
  const subtitles = page.locator("#subtitle-container");
  await expect(subtitles).toBeVisible();
  await expect(page.locator("#overlay-session-status")).toBeHidden();
  const before = await subtitles.boundingBox();
  for (const [captureId, displayState, warnings] of [
    [2, "sourceUnreadable", ["garbled"]],
    [3, "engineSlow", []],
    [4, "temporarilyUnavailable", ["local_engine: timeout"]],
    [5, "translated", ["local_engine: context_leakage", "local_engine: recovered_after_retry"]],
  ]) {
    await page.evaluate(
      ({ captureId, displayState, warnings }) =>
        window.overlayTest.emit("translation-update", {
          sessionId: 1,
          captureId,
          displayState,
          translated: "A readable subtitle",
          backendUsed: "local_engine",
          warnings,
          timestamp: Date.now(),
        }),
      { captureId, displayState, warnings },
    );
    await expect(page.locator("#subtitle-text")).toHaveText("A readable subtitle");
    await expect(page.locator("#overlay-session-status")).toBeHidden();
    expect(await subtitles.boundingBox()).toEqual(before);
  }
  await page.evaluate(() => {
    window.overlayTest.emit("translation-update", {
      sessionId: 1,
      captureId: 6,
      displayState: "noSubtitleText",
      translated: "",
      timestamp: Date.now(),
    });
    window.overlayTest.emit("subtitle-activity", { sessionId: 1, captureId: 7, state: "waiting" });
  });
  await expect(subtitles).toHaveClass(/hidden/);
  await expect(subtitles).toHaveCSS("opacity", "0");
  await expect(page.locator("#subtitle-text")).toHaveText("");
  await expect(page.locator("#overlay-session-status")).toContainText("Waiting for subtitles…");
  await page.evaluate(() => {
    window.overlayTest.emit("subtitle-activity", { sessionId: 1, captureId: 8, state: "active" });
    window.overlayTest.emit("translation-update", {
      sessionId: 1,
      captureId: 9,
      displayState: "translated",
      translated: "A readable subtitle",
      backendUsed: "local_engine",
      warnings: [],
      timestamp: Date.now(),
    });
  });
  await expect(page.locator("#subtitle-text")).toHaveText("A readable subtitle");
  await expect(subtitles).toBeVisible();
  await expect(page.locator("#overlay-session-status")).toBeHidden();
});

test("preparation and actionable failures do not move subtitle geometry", async ({ page }) => {
  await page.clock.install();
  await openOverlay(page);
  const status = page.locator("#overlay-session-status");
  await expect(status).toContainText("Preparing translation…");
  await expect(status.getByRole("button")).toBeHidden();
  await page.evaluate(() =>
    window.overlayTest.emit("translation-update", {
      sessionId: 1,
      captureId: 1,
      displayState: "translated",
      translated: "A readable subtitle",
      timestamp: Date.now(),
    }),
  );
  const subtitles = page.locator("#subtitle-container");
  await expect(subtitles).toBeVisible();
  const before = await subtitles.boundingBox();
  await page.evaluate(() =>
    window.overlayTest.emit("capture-status", {
      isError: true,
      message: "Screen capture failed: private endpoint details",
    }),
  );
  await expect(status).toContainText("Translation is interrupted.");
  await expect(status).not.toContainText("private");
  expect(await subtitles.boundingBox()).toEqual(before);
  const box = await status.boundingBox();
  await expect
    .poll(() =>
      page.evaluate(
        ({ x, y, width, height }) =>
          window.overlayTest.calls
            .filter((call) => call.command === "set_overlay_window_clip")
            .at(-1)
            ?.args.handleBounds?.some(
              (bound) =>
                bound.x === Math.round(x) &&
                bound.y === Math.round(y) &&
                bound.width === Math.round(width) &&
                bound.height === Math.round(height),
            ),
        box,
      ),
    )
    .toBe(true);
  await page.clock.runFor(500);
  await page.evaluate(() =>
    window.overlayTest.emit("subtitle-activity", { sessionId: 1, captureId: 2, state: "active" }),
  );
  await expect(status).toBeHidden();
  await page.clock.runFor(400);
  expect(
    await page.evaluate(
      ({ x, y, width, height }) =>
        window.overlayTest.calls
          .filter((call) => call.command === "set_overlay_window_clip")
          .at(-1)
          ?.args.handleBounds?.some(
            (bound) =>
              bound.x === Math.round(x) &&
              bound.y === Math.round(y) &&
              bound.width === Math.round(width) &&
              bound.height === Math.round(height),
          ),
      box,
    ),
  ).toBe(false);
  await page.evaluate(() => {
    window.overlayTest.emit("translation-update", {
      sessionId: 1,
      captureId: 3,
      displayState: "temporarilyUnavailable",
      warnings: ["local_engine: engine_recovery_failed"],
      timestamp: Date.now(),
    });
    window.overlayTest.emit("translation-update", {
      sessionId: 1,
      captureId: 4,
      displayState: "noSubtitleText",
      timestamp: Date.now(),
    });
  });
  await expect(status).toContainText("Translation engine stopped.");
  await expect(status.getByRole("button", { name: "Exit to Home" })).toBeVisible();
  await status.getByRole("button", { name: "Exit to Home" }).click();
  await expect(page.getByRole("button", { name: "Exit translation", exact: true })).toBeDisabled();
  expect(
    await page.evaluate(
      () => window.overlayTest.calls.filter((call) => call.command === "exit_translation").length,
    ),
  ).toBe(1);
});

test("successive cues replace text without a slide or a moving subtitle anchor", async ({
  page,
}) => {
  await page.setViewportSize({ width: 900, height: 650 });
  const region = { x: 120, y: 520, width: 400, height: 80 };
  await openOverlay(page, region);
  const sample = await page.evaluate(async () => {
    const emit = (text, id) =>
      window.overlayTest.emit("translation-update", {
        sessionId: 1,
        captureId: id,
        updateId: id,
        cueId: id,
        displayState: "translated",
        backendUsed: "local_engine",
        translated: text,
        timestamp: Date.now(),
        original: "Authored source",
      });
    const plate = document.querySelector("#subtitle-container");
    const anchor = () => {
      const rect = plate.getBoundingClientRect();
      return { bottom: rect.bottom, centerX: rect.x + rect.width / 2 };
    };
    emit("Wait here.", 1);
    await new Promise((resolve) => setTimeout(resolve, 400));
    const before = anchor();
    emit("Please wait by the entrance until the last train arrives at the station.", 2);
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
    const text = document.querySelector("#subtitle-text");
    return {
      before,
      after: anchor(),
      transform: getComputedStyle(text).transform,
      animation: getComputedStyle(text).animationName,
      opacity: getComputedStyle(text).opacity,
      bottom: plate.getBoundingClientRect().bottom,
    };
  });
  expect(sample.transform).toBe("none");
  expect(sample.animation).toBe("none");
  expect(sample.opacity).toBe("1");
  expect(sample.bottom).toBeLessThan(region.y);
  expect(sample.after.bottom).toBeCloseTo(sample.before.bottom, 1);
  expect(sample.after.centerX).toBeCloseTo(sample.before.centerX, 1);
});
