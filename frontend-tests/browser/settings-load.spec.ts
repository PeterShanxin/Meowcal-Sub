import { expect, test } from "@playwright/test";
import { defaultSettings } from "../../src/ui/settings-defaults";

test("settings edits wait for the saved profile and recover from a failed read", async ({
  page,
}) => {
  let stored = { ...structuredClone(defaultSettings), sourceLanguage: "ja-JP" };
  stored.translation.ocr.engine = "windows";
  let writes = 0;
  let fail = true;
  let release!: () => void;
  const reading = new Promise<void>((resolve) => (release = resolve));
  await page.route("**/api/ocr/languages", (route) =>
    fail ? route.abort() : route.fulfill({ json: ["ja-JP"] }),
  );
  await page.route("**/api/settings", async (route) => {
    if (route.request().method() === "POST") {
      writes++;
      stored = route.request().postDataJSON();
    } else {
      await reading;
      if (fail) return route.fulfill({ status: 503, json: { error: "Read unavailable" } });
    }
    return route.fulfill({ json: stored });
  });
  await page.route("**/api/engine/{status,refresh}", (route) =>
    route.fulfill({ json: { phase: "ready" } }),
  );
  await page.goto("/");
  const source = page.getByRole("combobox", { name: "Original subtitle language" });
  await expect(source).toBeDisabled();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  const edit = page.getByRole("switch", { name: /^Translate any text/ });
  await expect(edit).toBeDisabled();
  release();
  await expect(page.getByRole("alert")).toContainText("Couldn’t load settings");
  await page.getByRole("button", { name: "Subtitle style", exact: true }).click();
  await expect(page.getByRole("slider")).toBeDisabled();
  await page.getByRole("button", { name: "Home", exact: true }).click();
  await expect(page.locator(".home-action")).toBeDisabled();
  await page.getByRole("button", { name: "Try again" }).click();
  await expect(page.getByRole("alert")).toContainText("Couldn’t load settings");
  expect(writes).toBe(0);
  fail = false;
  await page.getByRole("button", { name: "Try again" }).click();
  await expect(source).toHaveValue("ja-JP");
  await expect(source).toBeEnabled();
  await expect(page.getByRole("button", { name: "Select subtitle area" })).toBeEnabled();
  await expect(page.getByRole("heading", { level: 1 })).toBeFocused();
  await expect(page.getByRole("alert")).toHaveCount(0);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await edit.check();
  await expect.poll(() => writes).toBe(1);
  expect(stored.sourceLanguage).toBe("ja-JP");
});
