import { expect, test } from "@playwright/test";
import { defaultSettings } from "../../src/ui/settings-defaults";

test("a profile sets the advanced recognition choices, and a manual change is custom", async ({
  page,
}) => {
  let stored = structuredClone(defaultSettings);
  await page.route("**/api/ocr/languages", (route) => route.fulfill({ json: ["en-US"] }));
  await page.route("**/api/engine/status", (route) => route.fulfill({ json: { phase: "ready" } }));
  await page.route("**/api/settings", (route) => {
    if (route.request().method() === "POST") stored = route.request().postDataJSON();
    return route.fulfill({ json: stored });
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();

  const profile = page.getByRole("combobox", { name: /^Profile/ });
  await expect(profile).toHaveValue("automatic");
  await expect(
    page.getByRole("img", { name: /Korean subtitles always use Windows OCR/ }),
  ).toBeVisible();

  await profile.selectOption("compatibility");
  await page.getByText("Advanced", { exact: true }).click();
  await expect(page.getByRole("combobox", { name: /^Text recognition/ })).toHaveValue("windows");
  await expect(page.getByRole("switch", { name: /^Run the engine on CPU only/ })).toBeChecked();
  await expect(page.getByRole("combobox", { name: /^Recognition quality/ })).toBeVisible();

  await page.getByRole("combobox", { name: /^Text recognition/ }).selectOption("ppOcrV6");
  await expect(profile).toHaveValue("custom");
  await expect(page.getByRole("combobox", { name: /^Recognition quality/ })).toHaveCount(0);
  await expect.poll(() => stored.translation.ocr.engine).toBe("ppOcrV6");
  expect(stored.translation.localEngine.cpuOnly).toBe(true);

  await profile.selectOption("automatic");
  await expect.poll(() => stored.translation.localEngine.cpuOnly).toBe(false);
  await expect(profile).toHaveValue("automatic");
  await expect(page.getByRole("switch", { name: /^Run the engine on CPU only/ })).not.toBeChecked();
});
