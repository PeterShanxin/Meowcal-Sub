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
    initialFontSize: 28,
    initialLight: false,
    onOpenChange: (open) => openStates.push(open),
    onFontSize: () => {},
    onLight: () => {},
    onCommit: () => {},
    ...overrides,
  });
  return { button, menu, closeButton, openStates, controller };
}

function fakeRadio(value) {
  return Object.assign(fakeElement(), { value, checked: false });
}

function fakeSlider() {
  const properties = new Map();
  return Object.assign(fakeElement(), {
    min: "20",
    max: "48",
    value: "",
    style: { setProperty: (name, value) => properties.set(name, value) },
    properties,
  });
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
    const inner = {};
    const menu = fakeElement([inner]);
    const { button, openStates } = buildMenu({ menu, closeButton: null });

    button.fire("click");
    document.fire("click", { target: inner });

    expect(menu.classes.has("visible")).toBe(true);
    expect(openStates).toEqual([true]);
  });

  it("reports the chosen plate scheme and saves it", () => {
    const dark = fakeRadio("dark");
    const light = fakeRadio("light");
    const choices = [];
    let commits = 0;
    buildMenu({
      plateInputs: [dark, light],
      onLight: (on) => choices.push(on),
      onCommit: () => (commits += 1),
    });

    expect(dark.checked).toBe(true);
    light.checked = true;
    light.fire("change");
    dark.checked = false;
    dark.fire("change");

    expect(choices).toEqual([true]);
    expect(commits).toBe(1);
  });

  it("keeps its controls in step with a change made in the main window", () => {
    const slider = fakeSlider();
    const display = { textContent: "" };
    const dark = fakeRadio("dark");
    const light = fakeRadio("light");
    const { controller } = buildMenu({
      fontSizeSlider: slider,
      fontSizeDisplay: display,
      plateInputs: [dark, light],
    });

    controller.syncFontSize(34);
    controller.syncPlate(true);

    expect(slider.value).toBe("34");
    expect(slider.properties.get("--ratio")).toBe(String(14 / 28));
    expect(display.textContent).toBe("34 px");
    expect([dark.checked, light.checked]).toEqual([false, true]);
  });

  it("shows a dragged text size before it is saved on release", () => {
    const slider = fakeSlider();
    const display = { textContent: "" };
    const sizes = [];
    let commits = 0;
    buildMenu({
      fontSizeSlider: slider,
      fontSizeDisplay: display,
      onFontSize: (size) => sizes.push(size),
      onCommit: () => (commits += 1),
    });

    slider.fire("input", { target: { value: "40" } });
    expect([sizes, display.textContent, commits]).toEqual([[40], "40 px", 0]);

    slider.fire("change");
    expect(commits).toBe(1);
  });
});
