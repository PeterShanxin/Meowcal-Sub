import { createRequire } from "node:module";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const { setupSettingsMenu } = require("../../src/scripts/overlay-settings-menu.js");

function fakeElement(children = []) {
  const listeners = new Map();
  const classes = new Set();
  return {
    listeners,
    classes,
    style: {},
    classList: {
      toggle: (name, force) => (force ? classes.add(name) : classes.delete(name)),
      add: (name) => classes.add(name),
      remove: (name) => classes.delete(name),
      contains: (name) => classes.has(name),
    },
    contains: (node) => children.includes(node),
    addEventListener: (type, handler) => {
      if (!listeners.has(type)) listeners.set(type, []);
      listeners.get(type).push(handler);
    },
    fire: (type, event = {}) => {
      const payload = { preventDefault() {}, stopPropagation() {}, ...event };
      (listeners.get(type) ?? []).forEach((handler) => handler(payload));
    },
  };
}

function fakeDocument() {
  const listeners = new Map();
  return {
    addEventListener: (type, handler) => {
      if (!listeners.has(type)) listeners.set(type, []);
      listeners.get(type).push(handler);
    },
    fire: (type, event = {}) => {
      const payload = { preventDefault() {}, stopPropagation() {}, ...event };
      (listeners.get(type) ?? []).forEach((handler) => handler(payload));
    },
  };
}

function buildMenu(overrides = {}) {
  const button = fakeElement();
  const menu = fakeElement();
  const closeButton = fakeElement();
  const openStates = [];
  const controller = setupSettingsMenu({
    button,
    menu,
    closeButton,
    fontSizeSlider: null,
    fontSizeDisplay: null,
    diagnosticsToggle: null,
    initialFontSize: 24,
    initialDiagnostics: false,
    onOpenChange: (open) => openStates.push(open),
    onFontSize: () => {},
    onDiagnostics: () => {},
    onCommit: () => {},
    ...overrides,
  });
  return { button, menu, closeButton, openStates, controller };
}

describe("overlay settings menu", () => {
  let document;

  beforeEach(() => {
    document = fakeDocument();
    globalThis.document = document;
  });

  afterEach(() => {
    delete globalThis.document;
  });

  it("closes from the close button when no outside click can reach the overlay", () => {
    const { button, menu, closeButton, openStates } = buildMenu();

    button.fire("click");
    expect(menu.classes.has("visible")).toBe(true);

    closeButton.fire("click");

    expect(menu.classes.has("visible")).toBe(false);
    expect(menu.classes.has("hidden")).toBe(true);
    expect(openStates).toEqual([true, false]);
  });

  it("closes on Escape", () => {
    const { button, menu, openStates } = buildMenu();

    button.fire("click");
    document.fire("keydown", { key: "Escape" });

    expect(menu.classes.has("visible")).toBe(false);
    expect(openStates).toEqual([true, false]);
  });

  it("ignores Escape while already closed", () => {
    const { openStates } = buildMenu();

    document.fire("keydown", { key: "Escape" });

    expect(openStates).toEqual([]);
  });

  it("toggles closed from the gear button", () => {
    const { button, menu } = buildMenu();

    button.fire("click");
    button.fire("click");

    expect(menu.classes.has("hidden")).toBe(true);
  });

  // An inline pointer-events value outranks `.settings-menu.hidden` and
  // `.capture-frame.faded .settings-button`, which leaves an invisible popup
  // hit-testing over the video: stray drags moved the font slider and stray
  // clicks flipped the diagnostics toggle.
  it("never writes pointer-events inline", () => {
    const { button, menu, closeButton } = buildMenu();

    button.fire("click");
    closeButton.fire("click");

    expect(button.style.pointerEvents).toBeUndefined();
    expect(menu.style.pointerEvents).toBeUndefined();
  });

  it("keeps the menu open when a click lands inside it", () => {
    const button = fakeElement();
    const inner = {};
    const menu = fakeElement([inner]);
    const openStates = [];
    setupSettingsMenu({
      button,
      menu,
      closeButton: null,
      fontSizeSlider: null,
      fontSizeDisplay: null,
      diagnosticsToggle: null,
      initialFontSize: 24,
      initialDiagnostics: false,
      onOpenChange: (open) => openStates.push(open),
      onFontSize: () => {},
      onDiagnostics: () => {},
      onCommit: () => {},
    });

    button.fire("click");
    document.fire("click", { target: inner });

    expect(menu.classes.has("visible")).toBe(true);
    expect(openStates).toEqual([true]);
  });
});
