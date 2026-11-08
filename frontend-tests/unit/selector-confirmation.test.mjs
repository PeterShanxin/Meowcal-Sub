import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import vm from "node:vm";
import { describe, expect, it, vi } from "vitest";

const require = createRequire(import.meta.url);
const source = readFileSync(new URL("../../src/scripts/selector.js", import.meta.url), "utf8");
const region = { x: 10, y: 800, width: 1200, height: 120 };

function fixture({
  origin = { x: 0, y: 0 },
  scale = 1.5,
  width = 1920,
  height = 1080,
  cached = region,
} = {}) {
  const invoke = vi.fn().mockResolvedValue(undefined);
  const coreInvoke = vi
    .fn()
    .mockImplementation(async (command) => (command === "get_capture_region" ? cached : undefined));
  const labels = [{ textContent: "" }, { textContent: "" }];
  class TestElement {
    constructor() {
      const classes = new Set();
      this.classList = {
        add: (...names) => names.forEach((name) => classes.add(name)),
        remove: (...names) => names.forEach((name) => classes.delete(name)),
        contains: (name) => classes.has(name),
      };
      this.dataset = {};
      this.style = {};
      this.offsetHeight = 36;
    }
    closest() {
      return null;
    }
    focus() {}
    addEventListener() {}
    querySelectorAll() {
      return [];
    }
  }
  const box = new TestElement();
  const buttons = new TestElement();
  const dimSegments = Array.from({ length: 4 }, () => new TestElement());
  const context = {
    window: {
      TauriBridge: { invoke },
      __TAURI__: {
        core: { invoke: coreInvoke },
        window: {
          getCurrentWindow: () => ({
            scaleFactor: async () => scale,
            innerPosition: async () => origin,
          }),
        },
      },
      SelectorGeometry: require("../../src/scripts/selector-geometry.js"),
      RegionGeometry: require("../../src/scripts/region-geometry.js"),
      screenX: origin.x / scale,
      screenY: origin.y / scale,
      innerWidth: width,
      innerHeight: height,
    },
    document: {
      addEventListener() {},
      querySelectorAll: () => labels,
      body: new TestElement(),
    },
    Element: TestElement,
    HTMLElement: TestElement,
    console: { error: vi.fn(), warn: vi.fn() },
    box,
    buttons,
    dimSegments,
  };
  vm.runInNewContext(
    source +
      `
    selectionBox = box;
    actionButtons = buttons;
    [overlayTop, overlayLeft, overlayRight, overlayBottom] = dimSegments;
    errorText = {};
    globalThis.actions = {state, confirmSelection, cancelSelection, applySelectionRequest, errorText,
      restoreExistingSelection, resetSelection, handleMouseDown, handleMouseMove, handleMouseUp,
      handleDragStart, handleResizeStart, handleKeyDown};
  `,
    context,
  );
  const actions = context.actions;
  actions.state.region = { ...region };
  actions.state.selectionId = 17;
  actions.applySelectionRequest({ requestId: "current", startTranslation: true });
  const mouse = (clientX, clientY, target = context.document.body) => ({
    clientX,
    clientY,
    screenX: context.window.screenX + clientX,
    screenY: context.window.screenY + clientY,
    target,
    preventDefault() {},
    stopPropagation() {},
  });
  const key = (key, options = {}) =>
    actions.handleKeyDown({
      key,
      target: context.document.body,
      ctrlKey: false,
      shiftKey: false,
      preventDefault() {},
      ...options,
    });
  return { invoke, actions, labels, context, box, mouse, key, TestElement };
}

describe("selector confirmation adapter", () => {
  it("confirms a cached rectangle with request identity and current DPI", async () => {
    const { invoke, actions, labels } = fixture();
    expect(labels.map((label) => label.textContent)).toEqual([
      "Confirm and start",
      "Confirm and start",
    ]);
    await actions.confirmSelection();
    expect(invoke).toHaveBeenCalledExactlyOnceWith("complete_area_selection", {
      requestId: "current",
      region,
      scaleFactor: 1.5,
      selectionId: 17,
    });
  });

  it("cancels without submitting a replacement crop", async () => {
    const { invoke, actions } = fixture();
    await actions.cancelSelection();
    expect(invoke).toHaveBeenCalledExactlyOnceWith("complete_area_selection", {
      requestId: "current",
      region: null,
    });
  });

  it("ignores duplicate confirmation and cancellation while saving", async () => {
    const { invoke, actions } = fixture();
    let finish;
    invoke.mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    const confirming = actions.confirmSelection();
    await actions.confirmSelection();
    await actions.cancelSelection();
    await Promise.resolve();
    expect(invoke).toHaveBeenCalledTimes(1);
    finish();
    await confirming;
  });

  it("keeps the selector open and permits retry when completion fails", async () => {
    const { invoke, actions, context } = fixture();
    invoke.mockRejectedValueOnce(new Error("Could not hide selector"));
    await actions.confirmSelection();
    expect(context.document.body.dataset.phase).toBe("error");
    expect(actions.errorText.textContent).toBe("Couldn’t save the area. Try again.");
    expect(actions.state.region).toEqual(region);
    expect(actions.state.isSaving).toBe(false);
    await actions.confirmSelection();
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("uses neutral confirmation copy when selecting without starting", () => {
    const { actions, labels } = fixture();
    actions.applySelectionRequest({ requestId: "select", startTranslation: false });
    expect(labels.every((label) => label.textContent === "Use this area")).toBe(true);
  });

  it.each(["drag", "resize"])(
    "clears an unfinished %s before restoring the next selection",
    async (mode) => {
      const { actions, mouse, box, TestElement } = fixture();
      await actions.restoreExistingSelection();
      if (mode === "drag") actions.handleDragStart(mouse(50, 820, box));
      else {
        const handle = new TestElement();
        handle.dataset.position = "se";
        actions.handleResizeStart(mouse(1210, 920, handle));
      }
      actions.resetSelection();
      await actions.restoreExistingSelection();
      actions.handleMouseMove(mouse(300, 300));
      expect(actions.state.region).toEqual(region);
    },
  );
});

for (const display of [
  { origin: { x: -2560, y: -120 }, scale: 1.5 },
  { origin: { x: 1920, y: 240 }, scale: 2 },
]) {
  describe(`selector on display at (${display.origin.x}, ${display.origin.y}) and ${display.scale}x DPI`, () => {
    it("restores and confirms the cached local crop without adding the display origin", async () => {
      const cached = { x: 100, y: 550, width: 800, height: 80 };
      const { actions, invoke, box, context } = fixture({
        ...display,
        width: 1280,
        height: 720,
        cached,
      });
      await actions.restoreExistingSelection();
      expect(actions.state.region).toEqual(cached);
      expect(box.style.left).toBe("100px");
      expect(box.style.top).toBe("550px");
      expect(context.dimSegments[0].style.height).toBe("550px");
      await actions.confirmSelection();
      expect(invoke).toHaveBeenCalledExactlyOnceWith("complete_area_selection", {
        requestId: "current",
        region: cached,
        scaleFactor: display.scale,
        selectionId: 17,
      });
    });

    it("draws, moves, resizes and nudges a crop using only client coordinates", async () => {
      const { actions, invoke, mouse, key, box, TestElement } = fixture({
        ...display,
        width: 1280,
        height: 720,
      });
      actions.resetSelection();
      actions.handleMouseDown(mouse(100, 450));
      actions.handleMouseMove(mouse(300, 490));
      actions.handleMouseUp(mouse(600, 510));
      expect(actions.state.region).toEqual({ x: 100, y: 450, width: 500, height: 60 });
      actions.handleDragStart(mouse(120, 460, box));
      actions.handleMouseMove(mouse(140, 480));
      actions.handleMouseUp(mouse(140, 480));
      const handle = new TestElement();
      handle.dataset.position = "se";
      actions.handleResizeStart(mouse(620, 530, handle));
      actions.handleMouseMove(mouse(640, 540));
      actions.handleMouseUp(mouse(640, 540));
      key("ArrowLeft", { ctrlKey: true });
      key("ArrowDown", { shiftKey: true });
      const selected = { x: 110, y: 470, width: 520, height: 71 };
      expect(actions.state.region).toEqual(selected);
      expect(box.style.left).toBe("110px");
      expect(box.style.top).toBe("470px");
      await actions.confirmSelection();
      expect(invoke).toHaveBeenCalledExactlyOnceWith("complete_area_selection", {
        requestId: "current",
        region: selected,
        scaleFactor: display.scale,
        selectionId: 17,
      });
    });

    it("creates a keyboard crop inside the local viewport", async () => {
      const { actions, invoke, key } = fixture({ ...display, width: 1280, height: 720 });
      actions.resetSelection();
      key("Enter");
      expect(actions.state.region).toEqual({ x: 256, y: 540, width: 768, height: 72 });
      key("ArrowLeft", { ctrlKey: true });
      await actions.confirmSelection();
      expect(invoke).toHaveBeenCalledExactlyOnceWith("complete_area_selection", {
        requestId: "current",
        region: { x: 246, y: 540, width: 768, height: 72 },
        scaleFactor: display.scale,
        selectionId: 17,
      });
    });

    it("contains restored crops and mouse and keyboard adjustments within the current viewport", async () => {
      const { actions, mouse, key, box } = fixture({
        ...display,
        width: 1280,
        height: 720,
        cached: { x: 1900, y: 1050, width: 1200, height: 120 },
      });
      await actions.restoreExistingSelection();
      const edge = { x: 80, y: 600, width: 1200, height: 120 };
      expect(actions.state.region).toEqual(edge);
      key("ArrowRight", { ctrlKey: true });
      key("ArrowDown", { shiftKey: true });
      expect(actions.state.region).toEqual(edge);
      actions.handleDragStart(mouse(100, 620, box));
      actions.handleMouseMove(mouse(2000, 750));
      actions.handleMouseUp(mouse(2000, 750));
      expect(actions.state.region).toEqual(edge);
      actions.resetSelection();
      actions.handleMouseDown(mouse(-50, -25));
      actions.handleMouseUp(mouse(1400, 800));
      expect(actions.state.region).toEqual({ x: 0, y: 0, width: 1280, height: 720 });
    });
  });
}
