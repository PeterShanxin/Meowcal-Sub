import { defineConfig, devices } from "@playwright/test";
import { allocatePorts } from "./scripts/allocate-port.mjs";

const portNames = ["MEOWCAL_FRONTEND_PORT", "MEOWCAL_HTTP_PORT"];
const ports = portNames.map((name) => {
  const value = process.env[name];
  if (!value) return null;
  const port = Number(value);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error(`${name} must be an integer between 1 and 65535.`);
  }
  return port;
});
if (ports[0] !== null && ports[0] === ports[1]) {
  throw new Error("MEOWCAL_FRONTEND_PORT and MEOWCAL_HTTP_PORT must be different.");
}

if (ports.includes(null)) {
  const candidates = (await allocatePorts(2)).filter((port) => !ports.includes(port));
  ports.forEach((port, index) => {
    ports[index] = port ?? candidates.shift();
  });
}
// Workers reevaluate this config, so they must inherit the server's ports.
portNames.forEach((name, index) => {
  process.env[name] = String(ports[index]);
});

const frontendOrigin = `http://127.0.0.1:${ports[0]}`;

export default defineConfig({
  testDir: "frontend-tests",
  testMatch: ["browser-linux/*.spec.mjs", "browser/overlay-toolbar.spec.mjs"],
  outputDir: "test-results/browser-linux",
  forbidOnly: Boolean(process.env.CI),
  workers: 1,
  retries: 0,
  timeout: 30_000,
  use: {
    ...devices["Desktop Chrome"],
    baseURL: frontendOrigin,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [
    { name: "chromium-desktop", use: { viewport: { width: 1280, height: 720 } } },
    { name: "chromium-compact", use: { viewport: { width: 560, height: 430 } } },
  ],
  // Deliberately no backend: these are real frontend/offline-state checks.
  // Windows keeps the separate browser-to-Rust smoke in playwright.config.mjs.
  webServer: {
    command: "npm run dev:browser",
    url: frontendOrigin,
    name: "Vite frontend (backend offline)",
    timeout: 30_000,
    reuseExistingServer: false,
    env: {
      MEOWCAL_FRONTEND_PORT: String(ports[0]),
      MEOWCAL_HTTP_PORT: String(ports[1]),
    },
  },
});
