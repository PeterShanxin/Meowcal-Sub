import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const config = JSON.parse(
  readFileSync(fileURLToPath(new URL("../../src-tauri/tauri.conf.json", import.meta.url)), "utf8"),
);

const mainRs = readFileSync(
  fileURLToPath(new URL("../../src-tauri/src/main.rs", import.meta.url)),
  "utf8",
);

const windowByLabel = (label) => config.app.windows.find((entry) => entry.label === label);

describe("tauri window chrome", () => {
  // Tauri creates a tray icon for `app.trayIcon`, and main.rs builds a second
  // one with the menu and click handlers. Declaring both put two identical cats
  // in the tray, only one of which responded to clicks.
  it("declares the tray icon in exactly one place", () => {
    expect(config.app.trayIcon).toBeUndefined();
    expect(mainRs).toContain("TrayIconBuilder::new()");
  });

  it.each(["main", "foundry-wizard"])("draws its own title bar on %s", (label) => {
    expect(windowByLabel(label)?.decorations).toBe(false);
  });

  // The overlay relies on Win32 region clipping rather than webview alpha, and
  // the region is only ever computed for an undecorated, shadowless window.
  it.each(["overlay", "selector"])("keeps %s chromeless and shadowless", (label) => {
    const window = windowByLabel(label);
    expect(window?.decorations).toBe(false);
    expect(window?.shadow).toBe(false);
  });
});
