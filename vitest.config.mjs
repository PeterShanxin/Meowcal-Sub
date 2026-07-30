import { defineConfig } from "vitest/config";
import { readFileSync } from "node:fs";

const baseline = JSON.parse(
  readFileSync(new URL("./config/maintainability-baseline.json", import.meta.url), "utf8"),
);

export default defineConfig({
  test: {
    coverage: {
      provider: "v8",
      include: [
        "scripts/serve-frontend.mjs",
        "src/scripts/backend-status.js",
        "src/scripts/ocr-language-tags.js",
        "src/scripts/pipeline-update.js",
        "src/scripts/translation-display.js",
        "src/scripts/wizard-state.js",
      ],
      reporter: ["text", "json-summary"],
      thresholds: baseline.frontendCoverageMinimum,
    },
    environment: "node",
    include: ["frontend-tests/unit/**/*.test.mjs"],
    watch: false,
  },
});
