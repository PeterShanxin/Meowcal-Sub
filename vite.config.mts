import { resolve } from "node:path";
import { defineConfig, type Plugin } from "vite";

/**
 * Tells pages served by the dev server which backend to talk to.
 *
 * The bridge defaults to the backend's default port and reads
 * `window.__MEOWCAL_API_BASE__` when something set it. Injecting that global
 * here is what makes a moved `MEOWCAL_HTTP_PORT` actually work end to end: the
 * frontend and the backend are separate processes, and without it a relocated
 * backend leaves the page calling a port nothing answers on (#35).
 *
 * Dev server only. A production build has no dev backend to point at, and
 * baking an origin into shipped HTML would be a way to send a user's app
 * somewhere it should never go.
 */
function backendOriginPlugin(): Plugin {
  return {
    name: "meowcal-backend-origin",
    apply: "serve",
    transformIndexHtml() {
      const port = process.env.MEOWCAL_HTTP_PORT;
      if (!port) {
        return [];
      }
      const apiBase = `http://127.0.0.1:${Number(port)}/api`;
      return [
        {
          tag: "script",
          // Before every page script, so the bridge sees it at initialization.
          injectTo: "head-prepend",
          children: `window.__MEOWCAL_API_BASE__=${JSON.stringify(apiBase)};`,
        },
      ];
    },
  };
}

export default defineConfig({
  root: "src",
  publicDir: false,
  plugins: [backendOriginPlugin()],
  server: {
    host: "127.0.0.1",
    // The dev server's port is configurable so browser verification can take a
    // port it has confirmed is free instead of requiring 3000 to be available on
    // a shared machine (#35). strictPort stays on: silently sliding to 3001
    // would put the frontend on the backend's default port.
    port: Number(process.env.MEOWCAL_FRONTEND_PORT || 3000),
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
