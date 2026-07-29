import js from "@eslint/js";
import globals from "globals";

const recommendedWarnings = Object.fromEntries(
  Object.keys(js.configs.recommended.rules).map((ruleName) => [ruleName, "warn"]),
);

export default [
  {
    ignores: ["node_modules/**", "src-tauri/**", "test-results/**", "playwright-report/**"],
  },
  {
    files: ["src/scripts/**/*.js"],
    languageOptions: {
      ecmaVersion: "latest",
      sourceType: "script",
      globals: {
        ...globals.browser,
        TauriBridge: "readonly",
      },
    },
    rules: {
      ...recommendedWarnings,
    },
  },
  {
    files: ["src/scripts/backend-status.js"],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.commonjs,
      },
    },
    rules: {
      ...js.configs.recommended.rules,
    },
  },
  {
    files: ["frontend-tests/**/*.{js,mjs}", "scripts/*.mjs", "*.config.mjs"],
    languageOptions: {
      ecmaVersion: "latest",
      sourceType: "module",
      globals: {
        ...globals.node,
      },
    },
    rules: {
      ...js.configs.recommended.rules,
    },
  },
  {
    files: ["frontend-tests/browser/**/*.mjs"],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
  },
];
