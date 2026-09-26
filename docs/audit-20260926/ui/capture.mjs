// Drives the shell windows through the audited journeys and saves screenshots.
//
//   node docs/audit-20260926/ui/capture.mjs <frontend origin> <before|after>
//
// The origin is a running `npm run dev:browser`. `tauri-shim.js` stands in for
// the Tauri runtime, so the pages run their Tauri code paths against scripted
// backend answers; this is presentation and flow evidence, not Windows evidence.
import { chromium } from "@playwright/test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const [origin = "http://127.0.0.1:3000", label = "after"] = process.argv.slice(2);
const shim = readFileSync(join(here, "tauri-shim.js"), "utf8");
const executablePath = process.env.MEOWCAL_AUDIT_CHROMIUM || undefined;

const DESKTOP = { width: 680, height: 500 }; // default window
const COMPACT = { width: 560, height: 430 }; // minimum window
const WIDE = { width: 1440, height: 900 }; // maximised on a 1080p display
const PHONE = { width: 390, height: 844 }; // not a supported window size

const browser = await chromium.launch({ executablePath });
const results = [];

async function open(path, scenario, viewport, { tauri = true, onboarded = true } = {}) {
  const context = await browser.newContext({ viewport });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  if (tauri) {
    const seen = onboarded ? 'localStorage.setItem("meowcal.onboardingComplete","true");' : "";
    await page.addInitScript(`window.__MOCK_SCENARIO__=${JSON.stringify(scenario)};${seen}`);
    await page.addInitScript(shim);
  }
  await page.goto(origin + path);
  await page.waitForTimeout(500);
  return { page, errors, close: () => context.close() };
}

const shot = (page, name) => page.screenshot({ path: join(here, `${label}-${name}.png`) });
const toast = async (page) => (await page.locator(".toast").allInnerTexts()).join(" / ") || null;
const record = (journey, check, value) => {
  results.push({ journey, check, value });
  console.log(`[${journey}] ${check}: ${JSON.stringify(value)}`);
};

// J1 first run: setup wizard from welcome to "Ready to watch", then a failure.
for (const [size, viewport] of [
  ["desktop", DESKTOP],
  ["compact", COMPACT],
]) {
  const { page, errors, close } = await open(
    "/wizard.html",
    { phase: "notInstalled", ocr: ["en-US"] },
    viewport,
  );
  await page.getByRole("button", { name: /Continue/ }).click();
  await page.getByRole("button", { name: /Install recognition/ }).click();
  await page.getByRole("button", { name: /Prepare translation/ }).click();
  await page.waitForTimeout(1200);
  await shot(page, `j1-setup-progress-${size}`);
  await page.getByRole("heading", { name: "Ready to watch" }).waitFor();
  await shot(page, `j1-setup-done-${size}`);
  record("J1", `${size} reaches Ready to watch`, errors.length === 0);
  await close();
}
{
  const { page, close } = await open(
    "/wizard.html",
    { phase: "notInstalled", wizardFailAt: 2 },
    COMPACT,
  );
  await page.getByRole("button", { name: /Continue/ }).click();
  await page.getByRole("button", { name: /Prepare translation/ }).click();
  await page.getByRole("button", { name: /Try again/ }).waitFor();
  await shot(page, "j1-setup-failed-compact");
  const fits = await page.evaluate(() => {
    const content = document.querySelector(".wizard-content");
    return content.scrollHeight <= content.clientHeight;
  });
  record("J1", "compact failure fits without scrolling", fits);
  await close();
}

// J2 Home: select area -> start -> running -> stop -> reload.
for (const [size, viewport] of [
  ["desktop", DESKTOP],
  ["compact", COMPACT],
  ["wide", WIDE],
]) {
  const { page, errors, close } = await open("/", { phase: "ready", startDelayMs: 1500 }, viewport);
  await shot(page, `j2-home-no-area-${size}`);
  await page.getByRole("button", { name: "Select subtitle area" }).click();
  await page.getByRole("heading", { name: "Ready for subtitles" }).waitFor();
  record("J2", `${size} toast after selecting`, await toast(page));
  await page.getByRole("button", { name: "Start translation" }).click();
  await page.waitForTimeout(300);
  await shot(page, `j2-home-starting-${size}`);
  record(
    "J2",
    `${size} language and area locked while starting`,
    await page.evaluate(() =>
      [...document.querySelectorAll(".session-panel select, .region-row")].every((e) => e.disabled),
    ),
  );
  await page.getByRole("heading", { name: "Subtitles are live" }).waitFor();
  await shot(page, `j2-home-running-${size}`);
  await page.getByRole("button", { name: "Stop translation" }).click();
  await page.getByRole("heading", { name: "Ready for subtitles" }).waitFor();
  await page.reload();
  await page.getByRole("heading", { name: "Ready for subtitles" }).waitFor();
  record("J2", `${size} stop and reload return to Ready`, errors.length === 0);
  await close();
}

// J3 Change the saved area, then cancel the selector.
{
  const region = { x: 320, y: 820, width: 1280, height: 120 };
  const { page, close } = await open(
    "/",
    { phase: "ready", region, selectorPicks: false },
    DESKTOP,
  );
  await page.getByRole("button", { name: /Change/ }).click();
  await page.waitForTimeout(800);
  await shot(page, "j3-change-area-cancelled-desktop");
  record("J3", "toast after cancelling Change (expect none)", await toast(page));
  await close();
}

// J4 Subtitle style: keyboard text size and plate.
for (const [size, viewport] of [
  ["desktop", DESKTOP],
  ["compact", COMPACT],
]) {
  const { page, close } = await open("/", { phase: "ready" }, viewport);
  await page.getByRole("button", { name: "Subtitle style" }).click();
  await page.getByRole("slider").focus();
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await page.getByRole("button", { name: "Light" }).click();
  await page.waitForTimeout(300);
  await shot(page, `j4-style-light-${size}`);
  record("J4", `${size} text size after two ArrowRight`, await page.locator("output").innerText());
  await close();
}

// J5 Settings: slow sample test, then messages over the end of the page.
{
  const { page, close } = await open("/", { phase: "ready", testDelayMs: 6000 }, DESKTOP);
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Test" }).click();
  await page.waitForTimeout(300);
  await shot(page, "j5-settings-test-running-desktop");
  await page.waitForTimeout(4500);
  record("J5", "message 4.8 s into a 6 s sample test", await toast(page));
  await page.waitForTimeout(1500);
  record("J5", "message after the sample test", await toast(page));
  await page.getByText("Developer options").click();
  await page.getByRole("switch", { name: /Developer mode/ }).check();
  await page.locator(".screen").evaluate((screen) => screen.scrollTo(0, screen.scrollHeight));
  await page.waitForTimeout(200);
  await shot(page, "j5-settings-end-with-message-desktop");
  const clear = await page.evaluate(() => {
    const last = [...document.querySelectorAll(".page .list-row")].at(-1).getBoundingClientRect();
    const message = document.querySelector(".toast")?.getBoundingClientRect();
    return !message || last.bottom <= message.top;
  });
  record("J5", "last Settings row clear of the message", clear);
  await close();
}

// J6 Capture reports during a session: a fallback warning while running, and
// an error raised while the player had focus, then the window regains focus.
for (const [size, viewport] of [
  ["desktop", DESKTOP],
  ["compact", COMPACT],
]) {
  const region = { x: 320, y: 820, width: 1280, height: 120 };
  const { page, close } = await open("/", { phase: "ready", region }, viewport);
  await page.getByRole("button", { name: "Start translation" }).click();
  await page.getByRole("heading", { name: "Subtitles are live" }).waitFor();
  await page.evaluate(() =>
    window.__MOCK_EMIT__("capture-status", {
      isError: false,
      usingFallback: true,
      message: "Using GDI fallback - video content may not capture correctly",
    }),
  );
  await page.waitForTimeout(4500); // past the "Translation started" message
  await shot(page, `j6-home-capture-warning-${size}`);
  record(
    "J6",
    `${size} support line with a fallback`,
    await page.locator(".support-line").innerText(),
  );
  await page.evaluate(() => {
    window.__MOCK_EMIT__("capture-status", {
      isError: true,
      usingFallback: false,
      message: "Capture failed: the capture item was closed",
    });
    window.dispatchEvent(new Event("focus"));
  });
  await page.waitForTimeout(400);
  await shot(page, `j6-home-capture-error-after-focus-${size}`);
  record("J6", `${size} message after regaining focus`, await toast(page));
  await close();
}

// Browser mode as a developer runs it: no Tauri runtime and no backend.
{
  const { page, close } = await open("/", {}, DESKTOP, { tauri: false });
  await page.waitForTimeout(800);
  await shot(page, "browser-mode-desktop");
  record("Browser", "window controls shown", await page.locator(".titlebar-button").count());
  record(
    "Browser",
    "Home status without a backend",
    await page.locator(".status-pill").innerText(),
  );
  await page.getByRole("button", { name: "Settings" }).click();
  await page.waitForTimeout(300);
  await shot(page, "browser-mode-settings-desktop");
  const engine = page.locator(".list-row", { hasText: "Translation engine" });
  record("Browser", "Settings engine chip", await engine.locator(".status-chip").innerText());
  await close();
}

// Phone width is outside the window's 560 px minimum; captured to record that.
{
  const { page, close } = await open("/", { phase: "ready" }, PHONE);
  await shot(page, "phone-home-unsupported");
  await close();
}

await browser.close();
