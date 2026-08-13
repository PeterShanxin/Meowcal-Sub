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
        "src/scripts/ocr-language-tags.js",
        "src/scripts/pipeline-update.js",
        "src/scripts/translation-display.js",
        "src/ui/languages.ts",
        "src/ui/home-state.ts",
        "src/ui/sample-translations.ts",
        "src/ui/setup-progress.ts",
        "src/ui/update-state.ts",
      ],
      reporter: ["text", "json-summary"],
      thresholds: baseline.frontendCoverageMinimum,
    },
    environment: "node",
    include: ["frontend-tests/unit/**/*.test.{mjs,ts}"],
    watch: false,
  },
});
