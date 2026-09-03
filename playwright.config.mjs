import { defineConfig, devices } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { allocatePorts } from "./scripts/allocate-port.mjs";

// The browser backend uses the debug build's development namespace under the
// APPDATA value supplied here. The smoke gets a disposable profile of its own,
// so it cannot touch a developer's or installed app's state.
const smokeProfile = join(tmpdir(), "meowcal-browser-smoke");
mkdirSync(smokeProfile, { recursive: true });

// Ports the OS has just confirmed are free, rather than 3000 and 3001 by decree.
// This machine is shared with unrelated work, and the old fixed pair could only
// be honoured by finding and stopping whatever else held them (#35). An explicit
// MEOWCAL_FRONTEND_PORT / MEOWCAL_HTTP_PORT still wins, so the smoke can be
// pinned to a known address while it is being debugged.
//
// Written back into the environment, which is load-bearing: this file is
// evaluated again in every worker process, and an unpinned allocation would give
// each worker a different pair of ports from the one the servers were actually
// started on - the tests would then navigate to a port nothing is listening on.
async function resolvePorts() {
  const explicitFrontend = process.env.MEOWCAL_FRONTEND_PORT
    ? Number(process.env.MEOWCAL_FRONTEND_PORT)
    : null;
  const explicitBackend = process.env.MEOWCAL_HTTP_PORT
    ? Number(process.env.MEOWCAL_HTTP_PORT)
    : null;

  if (explicitFrontend !== null && explicitFrontend === explicitBackend) {
    throw new Error(
      `MEOWCAL_FRONTEND_PORT and MEOWCAL_HTTP_PORT are both ${explicitFrontend}; ` +
        "the frontend and backend cannot share a port.",
    );
  }

  if (explicitFrontend === null || explicitBackend === null) {
    // Allocate only what was not given, and never hand back a port the caller
    // already pinned: allocating a fresh pair and using half of it can produce
    // the explicit port twice, which fails as "the second server would not
    // start" rather than as the collision it is. Over-allocating by the number
    // of pinned ports guarantees enough distinct candidates, because
    // allocatePorts holds them all at once.
    const pinned = new Set([explicitFrontend, explicitBackend].filter((port) => port !== null));
    const needed = (explicitFrontend === null ? 1 : 0) + (explicitBackend === null ? 1 : 0);
    const candidates = (await allocatePorts(needed + pinned.size)).filter(
      (port) => !pinned.has(port),
    );

    process.env.MEOWCAL_FRONTEND_PORT = String(explicitFrontend ?? candidates.shift());
    process.env.MEOWCAL_HTTP_PORT = String(explicitBackend ?? candidates.shift());
  }

  return {
    frontendPort: Number(process.env.MEOWCAL_FRONTEND_PORT),
    backendPort: Number(process.env.MEOWCAL_HTTP_PORT),
  };
}

const { frontendPort, backendPort } = await resolvePorts();

const frontendOrigin = `http://127.0.0.1:${frontendPort}`;
const backendOrigin = `http://127.0.0.1:${backendPort}`;

// Read by the smoke's fixture for its direct backend requests.
process.env.MEOWCAL_SMOKE_BACKEND_ORIGIN = backendOrigin;

export default defineConfig({
  testDir: "frontend-tests/browser",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 30_000,
  use: {
    baseURL: frontendOrigin,
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
      url: `${backendOrigin}/api/health`,
      name: "Rust browser backend",
      timeout: 900_000,
      reuseExistingServer: false,
      env: { APPDATA: smokeProfile, MEOWCAL_HTTP_PORT: String(backendPort) },
    },
    {
      command: "npm run dev:browser",
      url: frontendOrigin,
      name: "Static frontend",
      timeout: 30_000,
      reuseExistingServer: false,
      // Both: the first decides where this server listens, the second is what
      // the dev server injects into every page so the bridge calls this run's
      // backend. If that injection ever breaks, the bridge falls back to the
      // default port and the first smoke test fails on settings - which is the
      // failure this arrangement is supposed to have.
      env: {
        MEOWCAL_FRONTEND_PORT: String(frontendPort),
        MEOWCAL_HTTP_PORT: String(backendPort),
      },
    },
  ],
});
