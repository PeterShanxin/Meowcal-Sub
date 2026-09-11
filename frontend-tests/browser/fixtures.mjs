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

import { expect, test as base } from "@playwright/test";

export { expect };
export const test = base.extend({
  page: async ({ page }, use, testInfo) => {
    const failures = [];
    page.on("requestfailed", (request) => {
      failures.push({ url: request.url(), error: request.failure()?.errorText });
    });
    page.on("pageerror", (error) => failures.push({ error: error.message }));
    await use(page);
    if (testInfo.status !== testInfo.expectedStatus && failures.length) {
      await testInfo.attach("browser-load-failures", {
        body: JSON.stringify(failures, null, 2),
        contentType: "application/json",
      });
    }
  },
});
