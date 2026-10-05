import { afterEach, expect, it, vi } from "vitest";
import { EngineStatusController } from "../../src/ui/engine-status-controller";

afterEach(() => vi.unstubAllGlobals());

function setup(phase = "notRunning") {
  let status = { phase };
  let finish!: (value: { phase: string }) => void;
  let fail!: (error: Error) => void;
  const pending = new Promise<{ phase: string }>((resolve, reject) => {
    finish = resolve;
    fail = reject;
  });
  const publish = vi.fn();
  const invoke = vi.fn(async (command: string) => {
    if (command === "make_engine_ready") {
      status = { phase: "busy" };
      status = await pending;
    }
    return status;
  });
  vi.stubGlobal("window", { TauriBridge: { invoke }, setTimeout, clearTimeout });
  const controller = new EngineStatusController(publish);
  return {
    controller,
    invoke,
    publish,
    finish,
    fail,
  };
}

it("shares preparation across repeated selections and immediate Start without reading busy", async () => {
  const { controller, invoke, finish } = setup();
  await controller.read("get_engine_status", {});
  const first = controller.prewarm();
  const second = controller.prewarm();
  const start = controller.ready();
  expect(invoke.mock.calls.map(([name]) => name)).toEqual([
    "get_engine_status",
    "make_engine_ready",
  ]);
  finish({ phase: "ready" });
  await Promise.all([first, second]);
  await expect(start).resolves.toEqual({ phase: "ready" });
  expect(invoke.mock.calls.map(([name]) => name)).toEqual([
    "get_engine_status",
    "make_engine_ready",
    "refresh_engine_status",
  ]);
  controller.dispose();
});

it.each(["ready", "busy", "notInstalled", "error", "damaged", "unknown", "preparing"])(
  "does not speculatively prepare a %s engine",
  async (phase) => {
    const { controller, invoke } = setup(phase);
    await controller.read("get_engine_status", {});
    await controller.prewarm();
    expect(invoke).not.toHaveBeenCalledWith("make_engine_ready");
    controller.dispose();
  },
);

it("reports background failure and releases preparation for an explicit retry", async () => {
  const { controller, invoke, publish, fail } = setup();
  await controller.read("get_engine_status", {});
  const warmup = controller.prewarm();
  fail(new Error("Model failed to load"));
  await warmup;
  expect(publish).toHaveBeenCalledWith({ error: "Model failed to load" });
  invoke.mockImplementation(async (command) => ({
    phase: command === "make_engine_ready" ? "ready" : "notRunning",
  }));
  await expect(controller.ready()).resolves.toEqual({ phase: "ready" });
  controller.dispose();
});

it("does not publish warmup after disposal", async () => {
  const { controller, publish, finish } = setup();
  await controller.read("get_engine_status", {});
  const warmup = controller.prewarm();
  controller.dispose();
  publish.mockClear();
  finish({ phase: "ready" });
  await warmup;
  expect(publish).not.toHaveBeenCalled();
  controller.dispose();
});

it("rechecks readiness after joining warmup so a changed execution policy is respected", async () => {
  const { controller, invoke, finish } = setup();
  await controller.read("get_engine_status", {});
  const warmup = controller.prewarm();
  const start = controller.ready();
  invoke.mockImplementation(async (command) => ({
    phase: command === "make_engine_ready" ? "ready" : "notRunning",
  }));
  finish({ phase: "ready" });
  await warmup;
  await expect(start).resolves.toEqual({ phase: "ready" });
  expect(invoke.mock.calls.filter(([name]) => name === "make_engine_ready")).toHaveLength(2);
  controller.dispose();
});
