import { resolve } from "node:path";
import { defineConfig } from "vite";

export default defineConfig({
  root: "src",
  publicDir: false,
  server: {
    host: "127.0.0.1",
    strictPort: true,
  },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "es2022",
    rollupOptions: {
      input: {
        main: resolve(import.meta.dirname, "src/index.html"),
        wizard: resolve(import.meta.dirname, "src/wizard.html"),
        overlay: resolve(import.meta.dirname, "src/overlay.html"),
        selector: resolve(import.meta.dirname, "src/selector.html"),
      },
    },
  },
});
