// Home window cold start in the built frontend: navigation to the first
// rendered "Start translation" control, with paint and long-task timings.
//
// Backend commands answer instantly from fixed fixtures, so the numbers are the
// frontend's own cost. Real startup adds the Rust command latencies on top;
// those need Windows and are not measured here.
//
// Usage: node bench-home-startup.mjs <runs> <results.json>
import { createServer } from "node:http";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join, extname, dirname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const here = dirname(fileURLToPath(import.meta.url));
const dist = resolve(here, "../../../dist");
const runs = Number(process.argv[2] ?? 10);
const resultsPath = process.argv[3] ?? join(here, "results/home-startup.json");

const api = {
  "/api/settings": {},
  "/api/ocr/languages": ["en-US", "zh-Hans-CN"],
  "/api/engine/status": { phase: "ready" },
  "/api/capture-region": null,
};
const types = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".png": "image/png",
};
const server = createServer((req, res) => {
  const url = req.url.split("?")[0];
  if (url.startsWith("/api/")) {
    res.writeHead(200, { "content-type": "application/json", "access-control-allow-origin": "*" });
    return res.end(JSON.stringify(api[url] ?? {}));
  }
  // Resolve inside dist only: a "..%2f" request must not read other files.
  const path = resolve(dist, `.${url === "/" ? "/index.html" : decodeURIComponent(url)}`);
  if (!path.startsWith(dist + sep) || !existsSync(path)) {
    res.writeHead(404);
    return res.end();
  }
  res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
  res.end(readFileSync(path));
});
await new Promise((listening) => server.listen(0, "127.0.0.1", listening));
const origin = `http://127.0.0.1:${server.address().port}`;

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const browser = await chromium.launch({ executablePath: "/opt/pw-browsers/chromium" });
const samples = [];
for (let i = 0; i < runs + 1; i++) {
  const context = await browser.newContext({ viewport: { width: 900, height: 680 } });
  const page = await context.newPage();
  await page.addInitScript((base) => {
    window.__MEOWCAL_API_BASE__ = base;
    window.__longTasks = [];
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) window.__longTasks.push(entry.duration);
    }).observe({ type: "longtask", buffered: true });
    window.__lcp = 0;
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) window.__lcp = entry.startTime;
    }).observe({ type: "largest-contentful-paint", buffered: true });
  }, `${origin}/api`);
  await page.goto(`${origin}/index.html`);
  // Lit renders into shadow roots; Playwright's text locators pierce them.
  await page
    .getByRole("button", { name: /start translation/i })
    .first()
    .waitFor();
  const sample = await page.evaluate(async () => {
    const readyAt = performance.now();
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    const nav = performance.getEntriesByType("navigation")[0];
    const fcp = performance.getEntriesByName("first-contentful-paint")[0]?.startTime ?? null;
    return {
      domContentLoadedMs: nav.domContentLoadedEventEnd,
      loadMs: nav.loadEventEnd,
      fcpMs: fcp,
      lcpMs: window.__lcp,
      startButtonMs: readyAt,
      longTasks: window.__longTasks,
      transferBytes: performance
        .getEntriesByType("resource")
        .reduce((sum, r) => sum + (r.encodedBodySize || 0), nav.encodedBodySize || 0),
    };
  });
  await context.close();
  if (i > 0) samples.push(sample);
}
await browser.close();
server.close();
const keys = ["domContentLoadedMs", "fcpMs", "lcpMs", "startButtonMs", "loadMs"];
const p50 = Object.fromEntries(keys.map((k) => [k, median(samples.map((s) => s[k]))]));
p50.longTaskTotalMs = median(samples.map((s) => s.longTasks.reduce((a, b) => a + b, 0)));
p50.transferBytes = median(samples.map((s) => s.transferBytes));
console.error(JSON.stringify(p50));
writeFileSync(resultsPath, JSON.stringify({ runs, p50, samples }, null, 2));
