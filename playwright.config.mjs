import { defineConfig, devices } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// The browser backend reads and writes the config profile `%APPDATA%` points at,
// which is the *installed app's* profile (issue #68) - and since #71 gave dev
// mode the app's durable loader, starting it can refresh the backup, and on a
// damaged config quarantine and restore it. Correct for a developer running the
// server deliberately; not something a test suite should do to the machine it
// runs on. So the smoke gets a profile of its own.
const smokeProfile = join(tmpdir(), "meowcal-browser-smoke");
mkdirSync(smokeProfile, { recursive: true });

export default defineConfig({
  testDir: "frontend-tests/browser",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 30_000,
  use: {
    baseURL: "http://127.0.0.1:3000",
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "Google Chrome",
      use: {
        ...devices["Desktop Chrome"],
        channel: process.env.MEOWCAL_BROWSER_CHANNEL || "chrome",
      },
    },
  ],
  webServer: [
    {
      command: "npm run dev:backend",
      url: "http://127.0.0.1:3001/api/health",
      name: "Rust browser backend",
      timeout: 900_000,
      reuseExistingServer: false,
      env: { APPDATA: smokeProfile },
    },
    {
      command: "npm run dev:browser",
      url: "http://127.0.0.1:3000",
      name: "Static frontend",
      timeout: 30_000,
      reuseExistingServer: false,
    },
  ],
});
