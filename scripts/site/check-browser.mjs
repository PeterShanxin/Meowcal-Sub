// Check the built static site without JavaScript, including nested guide routes.
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { once } from "node:events";
import { createStaticServer } from "../serve-frontend.mjs";
import { chromium } from "@playwright/test";
import { CONTENT, PAGE_PATHS, SITE_URL, pagePath } from "./render.mjs";

const root = fileURLToPath(new URL("../../", import.meta.url));
const output = path.join(root, "_site");
const screenshots = path.join(root, "test-results/site");
await mkdir(screenshots, { recursive: true });
const server = createStaticServer({ rootDirectory: output });
server.listen(0, "127.0.0.1");
await once(server, "listening");
const origin = `http://127.0.0.1:${server.address().port}/`;
let browser;
let checked = 0;
try {
  browser = await chromium.launch();
  for (const width of [390, 1440]) {
    const context = await browser.newContext({
      viewport: { width, height: 960 },
      javaScriptEnabled: false,
      reducedMotion: "reduce",
    });
    const page = await context.newPage();
    for (const locale of Object.keys(CONTENT)) {
      for (const id of Object.keys(PAGE_PATHS)) {
        const relative = pagePath(id, locale);
        const response = await page.goto(origin + relative);
        assert.equal(response.status(), 200);
        assert.equal(await page.locator("h1").count(), 1);
        assert.equal(
          await page.locator('link[rel="canonical"]').getAttribute("href"),
          SITE_URL + relative,
        );
        assert.equal(await page.locator("html").getAttribute("lang"), locale);
        assert.equal(
          await page.evaluate(
            () => globalThis.document.documentElement.scrollWidth > globalThis.innerWidth,
          ),
          false,
          `Horizontal overflow: ${relative} at ${width}px`,
        );
        assert.equal(
          await page
            .locator("img")
            .evaluateAll((images) =>
              images.every((image) => image.complete && image.naturalWidth > 0),
            ),
          true,
          `Missing image: ${relative}`,
        );
        await page.evaluate(() => globalThis.scrollTo(0, 0));
        await page.screenshot({
          path: path.join(screenshots, `${id}-${locale}-${width}.png`),
          fullPage: true,
        });
        const other = locale === "en" ? "zh-CN" : "en";
        await page.locator("a.language").click();
        assert.equal(await page.locator("html").getAttribute("lang"), other);
        assert.equal(
          await page.locator('link[rel="canonical"]').getAttribute("href"),
          SITE_URL + pagePath(id, other),
        );
        checked += 1;
      }
    }
    await context.close();
  }
} finally {
  await browser?.close();
  await new Promise((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
}
console.log(
  `Checked ${checked} static page/viewport combinations; screenshots: test-results/site.`,
);
