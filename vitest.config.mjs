import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["frontend-tests/unit/**/*.test.mjs"],
    watch: false,
  },
});
