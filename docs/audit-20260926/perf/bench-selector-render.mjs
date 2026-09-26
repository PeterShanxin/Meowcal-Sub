// Webview half of the area-selector open: the built selector page receives the
// snapshot the way Tauri IPC delivers it (a JSON string, parsed) and paints it.
//
// Serves ../../../dist (run `npm run build:web` first) and the payloads written
// by snapshot-bench into fixtures/out. `window.__TAURI__` is a stub whose
// `get_selector_snapshot` fetches the payload as text, then times JSON.parse
// separately - the local fetch is not a model of IPC transfer and is not
// reported as part of the render.
//
// Usage: node bench-selector-render.mjs <runs> <results.json> [variant,...]
import { createServer } from "node:http";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join, extname, dirname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const here = dirname(fileURLToPath(import.meta.url));
const dist = resolve(here, "../../../dist");
const outDir = join(here, "fixtures/out");
const runs = Number(process.argv[2] ?? 5);
const resultsPath = process.argv[3] ?? join(here, "results/selector-render.json");
const variants = (process.argv[4] ?? "baseline-rgba-balanced,rgb-fast,jpeg-q85").split(",");
const fixtures = JSON.parse(readFileSync(join(here, "fixtures/fixtures.json"), "utf8"));

const types = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".png": "image/png",
};
let payload = "";
const server = createServer((req, res) => {
  if (req.url === "/__snapshot") {
    res.writeHead(200, { "content-type": "application/json" });
    return res.end(payload);
  }
  // Resolve inside dist only: a "..%2f" request must not read other files.
  const path = resolve(dist, `.${decodeURIComponent(req.url.split("?")[0])}`);
  if (!path.startsWith(dist + sep) || !existsSync(path)) {
    res.writeHead(404);
    return res.end();
  }
  res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
  res.end(readFileSync(path));
});
await new Promise((listening) => server.listen(0, "127.0.0.1", listening));
const origin = `http://127.0.0.1:${server.address().port}`;

const stub = () => {
  window.__bench = {};
  const noop = async () => {};
  window.__TAURI__ = {
    core: {
      invoke: async (command) => {
        if (command !== "get_selector_snapshot") return null;
        const text = await (await fetch("/__snapshot")).text();
        const t0 = performance.now();
        const snapshot = JSON.parse(text);
        window.__bench.parseMs = performance.now() - t0;
        window.__bench.deliveredAt = performance.now();
        return snapshot;
      },
    },
    event: { listen: async () => noop },
    window: { getCurrentWindow: () => ({ setFocus: noop, setBackgroundColor: noop }) },
    webviewWindow: { getCurrentWebviewWindow: () => ({ setBackgroundColor: noop }) },
  };
};

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const browser = await chromium.launch({ executablePath: "/opt/pw-browsers/chromium" });
const results = [];
for (const fixture of fixtures) {
  for (const variant of variants) {
    const file = join(outDir, `${fixture.name}.${variant}.json`);
    if (!existsSync(file)) continue;
    payload = readFileSync(file, "utf8");
    const samples = [];
    for (let i = 0; i < runs + 1; i++) {
      const context = await browser.newContext({
        viewport: { width: 1280, height: 720 },
        deviceScaleFactor: 1,
      });
      const page = await context.newPage();
      await page.addInitScript(stub);
      await page.goto(`${origin}/selector.html`);
      const sample = await page.evaluate(async () => {
        const img = document.getElementById("desktop-snapshot");
        while (!img.getAttribute("src")) await new Promise((r) => setTimeout(r, 1));
        await img.decode();
        await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
        const paintedAt = performance.now();
        return {
          parseMs: window.__bench.parseMs,
          decodePaintMs: paintedAt - window.__bench.deliveredAt,
          naturalWidth: img.naturalWidth,
        };
      });
      await context.close();
      if (i > 0) samples.push(sample); // first run warms the browser process
    }
    const parse = samples.map((s) => s.parseMs);
    const paint = samples.map((s) => s.decodePaintMs);
    const total = samples.map((s) => s.parseMs + s.decodePaintMs);
    console.error(
      `${fixture.name.padEnd(24)} ${variant.padEnd(24)} parse p50 ${median(parse).toFixed(1).padStart(7)} ms  decode+paint p50 ${median(paint).toFixed(1).padStart(7)} ms  total p50 ${median(total).toFixed(1).padStart(7)} ms  (${samples[0].naturalWidth}px)`,
    );
    results.push({
      fixture: fixture.name,
      variant,
      payloadBytes: payload.length,
      samples,
      p50: { parseMs: median(parse), decodePaintMs: median(paint), totalMs: median(total) },
    });
  }
}
await browser.close();
server.close();
writeFileSync(resultsPath, JSON.stringify(results, null, 2));
