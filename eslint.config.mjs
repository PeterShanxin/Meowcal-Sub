import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";

const recommendedWarnings = Object.fromEntries(
  Object.keys(js.configs.recommended.rules).map((ruleName) => [ruleName, "warn"]),
);

export default [
  {
    ignores: ["node_modules/**", "src-tauri/**", "test-results/**", "playwright-report/**"],
  },
  {
    files: ["src/**/*.ts", "frontend-tests/**/*.ts", "vite.config.mts"],
    languageOptions: {
      parser: tseslint.parser,
      parserOptions: {
        ecmaVersion: "latest",
        sourceType: "module",
      },
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
    plugins: {
      "@typescript-eslint": tseslint.plugin,
    },
    rules: {
      ...tseslint.configs.recommended.rules,
    },
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
    files: [
      "src/scripts/overlay-i18n.js",
      "src/scripts/overlay-diagnostics.js",
      "src/scripts/overlay-session-status.js",
      "src/scripts/overlay-toolbar.js",
      "src/scripts/overlay.js",
      "src/scripts/selector.js",
    ],
    languageOptions: { sourceType: "module" },
  },
  {
    files: ["frontend-tests/**/*.{js,mjs}", "scripts/**/*.mjs", "*.config.mjs"],
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
