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

const defaultCapability = JSON.parse(
  readFileSync(
    fileURLToPath(new URL("../../src-tauri/capabilities/default.json", import.meta.url)),
    "utf8",
  ),
);

const titlebar = readFileSync(
  fileURLToPath(new URL("../../src/ui/meowcal-titlebar.ts", import.meta.url)),
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

  // `core:default` grants `allow-is-maximized` but none of the controls that
  // act on the window, so the title bar rendered three buttons whose calls the
  // ACL rejected. Nothing surfaced: the promise rejected and the window sat
  // there. An undecorated window with dead controls cannot be closed at all.
  it.each([
    ["minimize", "core:window:allow-minimize"],
    ["toggleMaximize", "core:window:allow-toggle-maximize"],
    ["close", "core:window:allow-close"],
  ])("grants the permission its %s button needs", (action, permission) => {
    expect(titlebar).toContain(`this.run("${action}")`);
    expect(defaultCapability.permissions).toContain(permission);
  });

  // Dragging an undecorated window is the only way to move it.
  it("grants the drag-region permission the title bar relies on", () => {
    expect(titlebar).toContain("data-tauri-drag-region");
    expect(defaultCapability.permissions).toContain("core:window:allow-start-dragging");
  });

  // A literal asset path resolved under the dev server and 404'd once the
  // bundler emitted the file under a content hash, so the app shipped with no
  // logo in its own title bar.
  it("imports the title bar logo instead of hardcoding its path", () => {
    expect(titlebar).toContain('from "../assets/meowcal-icon.png"');
    expect(titlebar).not.toContain('src="./assets/');
  });

  // The overlay relies on Win32 region clipping rather than webview alpha, and
  // the region is only ever computed for an undecorated, shadowless window.
  it.each(["overlay", "selector"])("keeps %s chromeless and shadowless", (label) => {
    const window = windowByLabel(label);
    expect(window?.decorations).toBe(false);
    expect(window?.shadow).toBe(false);
  });
});
