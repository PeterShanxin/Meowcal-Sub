import { copyFile, mkdir, readFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { chromium } from "@playwright/test";

function run(args) {
  execFileSync(process.execPath, ["node_modules/@tauri-apps/cli/tauri.js", ...args], {
    stdio: "inherit",
  });
}

// The pinned Tauri CLI rasterizes the same vector for every packaged size.
await mkdir(".local/brand-assets", { recursive: true });
run(["icon", "docs/assets/logo.svg", "-o", "src-tauri/icons"]);
run(["icon", "docs/assets/logo.svg", "-p", "300", "-p", "1024", "-o", ".local/brand-assets"]);
await Promise.all([
  copyFile(".local/brand-assets/1024x1024.png", "docs/assets/logo.png"),
  copyFile(".local/brand-assets/1024x1024.png", "src/assets/meowcal-icon.png"),
  copyFile(".local/brand-assets/300x300.png", "docs/assets/store-listing-logo-300.png"),
]);

const browser = await chromium.launch({ channel: process.env.MEOWCAL_BROWSER_CHANNEL || "chrome" });
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 640 } });
  const source = await readFile("docs/assets/social-preview.svg", "utf8");
  await page.setContent(`<style>body { margin: 0 }</style>${source}`);
  await page.evaluate("document.fonts.ready");
  await page.screenshot({ path: "docs/assets/social-preview.png" });
} finally {
  await browser.close();
}
