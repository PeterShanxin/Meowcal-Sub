import { expect, test as base } from "@playwright/test";

const test = base.extend({
  page: async ({ page }, use) => {
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await use(page);
    expect(errors).toEqual([]);
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
        getCurrentWindow: () => ({ scaleFactor: async () => devicePixelRatio }),
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
