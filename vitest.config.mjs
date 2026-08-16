import { defineConfig } from "vitest/config";
import { readFileSync } from "node:fs";

const baseline = JSON.parse(
  readFileSync(new URL("./config/maintainability-baseline.json", import.meta.url), "utf8"),
);

// The measured scope of the frontend coverage claim lives in the baseline
// manifest, next to the floors it is the denominator of. Keeping it there is
// what lets the maintainability ratchet enforce the rule that matters: the scope
// may grow but never shrink, and a floor may only fall in a change that widens
// it (#35). A glob here would have been shorter and would have lied by omission
// - with this provider a file no test touches never appears, so `src/**` would
// report the same number while saying nothing about the modules nothing
// exercises.
//
// frontend-tests/unit/coverage-scope.test.mjs keeps the list honest from the
// other side: a module a test exercises must be in it.
export default defineConfig({
  test: {
    coverage: {
      provider: "v8",
      include: baseline.frontendCoverageScope,
      reporter: ["text", "json-summary"],
      thresholds: baseline.frontendCoverageMinimum,
    },
    environment: "node",
    include: ["frontend-tests/unit/**/*.test.{mjs,ts}"],
    watch: false,
  },
});
