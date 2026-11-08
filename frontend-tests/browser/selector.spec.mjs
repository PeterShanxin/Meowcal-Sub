import { expect, test } from "@playwright/test";

async function openSelector(page, scale = 2, correlated = false) {
  await page.addInitScript(
    ({ dpi, correlated }) => {
      const listeners = new Map();
      window.selectorTest = {
        calls: [],
        failure: null,
        request: correlated ? { requestId: "request-17", startTranslation: true } : null,
        emit(name, payload) {
          listeners.get(name)?.({ payload });
        },
      };
      Object.defineProperty(window, "screenX", { value: -1560 });
      Object.defineProperty(window, "screenY", { value: -260 });
      window.__TAURI__ = {
        core: {
          async invoke(command, args) {
            window.selectorTest.calls.push({ command, args });
            if (command === "get_selector_snapshot")
              return { selectionId: 17, width: 3120, height: 2080, dataUrl: "" };
            if (command === "get_capture_region") return null;
            if (command === "get_area_selection_request") return window.selectorTest.request;
            if (
              ["set_capture_region", "complete_area_selection"].includes(command) &&
              window.selectorTest.failure
            )
              throw window.selectorTest.failure;
          },
        },
        event: {
          async listen(name, callback) {
            listeners.set(name, callback);
            return () => {};
          },
          async emit() {},
        },
        webview: { getCurrentWebview: () => ({ setBackgroundColor: async () => {} }) },
        window: {
          getCurrentWindow: () => ({
            scaleFactor: async () => dpi,
            setFocus: async () => {},
            hide: async () => {},
          }),
        },
      };
    },
    { dpi: scale, correlated },
  );
  await page.goto("/selector.html");
  await expect.poll(() => page.locator("body").getAttribute("tabindex")).toBe("-1");
}

for (const scale of [1, 1.25, 1.5, 2]) {
  test(`selector saves local coordinates at ${scale}x with a negative desktop origin`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1000, height: 700 });
    await openSelector(page, scale);
    await page.mouse.move(100, 200);
    await page.mouse.down();
    await page.mouse.move(700, 280);
    await page.mouse.up();
    await expect(page.locator("#selection-box")).toHaveCSS("left", "100px");
    await expect(page.locator("#selection-box")).toHaveCSS("top", "200px");
    await page.getByRole("button", { name: "Use this area" }).click();
    expect(
      await page.evaluate(
        () => window.selectorTest.calls.find((c) => c.command === "set_capture_region")?.args,
      ),
    ).toEqual({ x: 100, y: 200, width: 600, height: 80, scaleFactor: scale, selectionId: 17 });
  });
}

test("reopening the selector replaces its token and clears an unavailable screenshot", async ({
  page,
}) => {
  await openSelector(page);
  await page.evaluate(() =>
    window.selectorTest.emit("selector-background-snapshot", {
      selectionId: 18,
      width: 3120,
      height: 2080,
      dataUrl: "",
    }),
  );
  await expect(page.locator("#desktop-snapshot")).not.toHaveAttribute("src");
  await page.keyboard.press("Enter");
  await expect(page.locator("#confirm-btn")).toBeFocused();
  await page.keyboard.press("Enter");
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.selectorTest.calls.find((c) => c.command === "set_capture_region")?.args
            .selectionId,
      ),
    )
    .toBe(18);
});

test("a display change explains the failure and Cancel leaves the area uncommitted", async ({
  page,
}) => {
  await openSelector(page);
  await page.evaluate(() => {
    window.selectorTest.failure = "The display changed. Select the subtitle area again.";
  });
  await page.keyboard.press("Enter");
  await page.keyboard.press("Enter");
  await expect(page.locator("#selector-error")).toHaveText(
    "The display changed. Select the subtitle area again.",
  );
  await page.keyboard.press("Escape");
  await expect
    .poll(() =>
      page.evaluate(
        () => window.selectorTest.calls.filter((c) => c.command === "close_area_selector").length,
      ),
    )
    .toBe(1);
});

test("session confirmation carries both the request identity and display selection token", async ({
  page,
}) => {
  await openSelector(page, 1.5, true);
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "Confirm and start" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.selectorTest.calls.find((call) => call.command === "complete_area_selection")
            ?.args,
      ),
    )
    .toEqual({
      requestId: "request-17",
      selectionId: 17,
      region: {
        x: Math.round(page.viewportSize().width * 0.2),
        y: Math.round(page.viewportSize().height * 0.75),
        width: Math.round(page.viewportSize().width * 0.6),
        height: Math.max(40, Math.round(page.viewportSize().height * 0.1)),
      },
      scaleFactor: 1.5,
    });
  expect(
    await page.evaluate(() =>
      window.selectorTest.calls.filter((call) => call.command === "set_capture_region"),
    ),
  ).toHaveLength(0);
});

test("Cancel during session confirmation sends no replacement crop", async ({ page }) => {
  await openSelector(page, 2, true);
  await page.keyboard.press("Enter");
  await page.keyboard.press("Escape");
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.selectorTest.calls.find((call) => call.command === "complete_area_selection")
            ?.args,
      ),
    )
    .toEqual({ requestId: "request-17", region: null });
  expect(
    await page.evaluate(() =>
      window.selectorTest.calls.filter((call) => call.command === "set_capture_region"),
    ),
  ).toHaveLength(0);
});
