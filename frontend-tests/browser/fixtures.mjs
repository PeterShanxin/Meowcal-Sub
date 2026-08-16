// Where this run's backend is.
//
// The origin is decided per run by playwright.config.mjs, which allocates a
// free port instead of demanding 3001 (#35). The served pages learn about it
// from the dev server, which injects `window.__MEOWCAL_API_BASE__`; the direct
// API requests in the smoke learn about it from here.

export const backendOrigin = process.env.MEOWCAL_SMOKE_BACKEND_ORIGIN;

if (!backendOrigin) {
  throw new Error(
    "MEOWCAL_SMOKE_BACKEND_ORIGIN is not set. Run the smoke through playwright.config.mjs.",
  );
}

export { expect, test } from "@playwright/test";
