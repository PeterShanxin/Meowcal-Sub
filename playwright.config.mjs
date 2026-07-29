import { defineConfig, devices } from "@playwright/test";

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
      timeout: 180_000,
      reuseExistingServer: false,
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
