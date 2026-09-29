import { readFileSync } from "node:fs";
import config from "../../vite.config.mts";
import { defaultSettings } from "../../src/ui/settings-defaults.ts";

let settings = structuredClone(defaultSettings);
let mode = "ready";
const json = (res, status, body) => {
  res.writeHead(status, { "Content-Type": "application/json" });
  res.end(JSON.stringify(body));
};
export default {
  ...config,
  server: { ...config.server, allowedHosts: ["terminal.local"] },
  plugins: [
    {
      name: "ui-audit-fixtures",
      apply: "serve",
      transformIndexHtml() {
        return [
          {
            tag: "script",
            injectTo: "head-prepend",
            children: 'window.__MEOWCAL_API_BASE__="/__ui-audit/api";',
          },
        ];
      },
      configureServer(server) {
        server.middlewares.use(async (req, res, next) => {
          const path = new URL(req.url, "http://localhost").pathname;
          if (path === "/audit") {
            res.setHeader("Content-Type", "text/html");
            res.end(readFileSync(new URL("./viewport.html", import.meta.url), "utf8"));
            return;
          }
          if (!path.startsWith("/__ui-audit/")) return next();
          let body = "";
          for await (const chunk of req) body += chunk;
          if (path === "/__ui-audit/mode") {
            mode = JSON.parse(body).mode;
            return json(res, 200, { mode });
          }
          if (mode === "offline") return json(res, 503, { error: "Backend unavailable" });
          const route = path.replace("/__ui-audit/api", "");
          if (route === "/settings") {
            if (mode === "slow") await new Promise((resolve) => setTimeout(resolve, 1500));
            if (req.method === "POST") {
              if (mode === "save-error")
                return json(res, 503, { error: "Settings could not be saved" });
              settings = JSON.parse(body);
            }
            return json(res, 200, settings);
          }
          if (route === "/ocr/languages") return json(res, 200, ["en-US", "zh-Hans-CN", "ja-JP"]);
          if (["/engine/status", "/engine/refresh"].includes(route))
            return json(res, 200, { phase: "ready", serviceRunning: true, installed: true });
          if (route === "/capture-region") return json(res, 200, null);
          return json(res, 501, {
            browserMode: true,
            message:
              "This action requires the Windows app. The UI audit does not run native capture or translation.",
          });
        });
      },
    },
  ],
};
