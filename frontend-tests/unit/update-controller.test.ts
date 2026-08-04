import { afterEach, describe, expect, it, vi } from "vitest";
import type { PendingUpdate, TauriBridgeApi, UiSnapshot } from "../../src/ui/contracts";
import { UpdateController } from "../../src/ui/update-controller";

function harness(updates?: TauriBridgeApi["updates"], invoke = vi.fn().mockResolvedValue(null)) {
  const patches: Array<Partial<UiSnapshot>> = [];
  vi.stubGlobal("window", {
    TauriBridge: {
      invoke,
      isBrowserMode: () => updates === undefined,
      event: { listen: vi.fn(), emit: vi.fn() },
      updates,
    },
  });
  return { controller: new UpdateController((patch) => patches.push(patch)), patches, invoke };
}

function pending(overrides: Partial<PendingUpdate> = {}): PendingUpdate {
  return {
    version: "0.6.7",
    notes: "Fixes",
    install: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("initial state", () => {
  it("reports the installed version so the screen can name it", async () => {
    const { controller } = harness({
      currentVersion: vi.fn().mockResolvedValue("0.6.6"),
      check: vi.fn(),
      restart: vi.fn(),
    });

    expect(await controller.initialState()).toEqual({
      update: { kind: "idle" },
      appVersion: "0.6.6",
    });
  });

  it("stays usable when the app cannot report its own version", async () => {
    const { controller } = harness({
      currentVersion: vi.fn().mockRejectedValue(new Error("no app api")),
      check: vi.fn(),
      restart: vi.fn(),
    });

    expect(await controller.initialState()).toEqual({
      update: { kind: "idle" },
      appVersion: null,
    });
  });

  it("marks updating unsupported where there is no installation to replace", async () => {
    const { controller } = harness(undefined);

    expect(await controller.initialState()).toEqual({
      update: { kind: "unsupported" },
      appVersion: null,
    });
  });
});

describe("checking", () => {
  it("reports being current when the endpoint has nothing newer", async () => {
    const { controller, patches } = harness({
      currentVersion: vi.fn(),
      check: vi.fn().mockResolvedValue(null),
      restart: vi.fn(),
    });

    await controller.check();

    expect(patches.map((patch) => patch.update?.kind)).toEqual(["checking", "upToDate"]);
  });

  it("surfaces a failed check as retryable rather than as up to date", async () => {
    const { controller, patches } = harness({
      currentVersion: vi.fn(),
      check: vi.fn().mockRejectedValue(new Error("endpoint unreachable")),
      restart: vi.fn(),
    });

    await controller.check();

    expect(patches.at(-1)?.update).toEqual({
      kind: "error",
      message: "endpoint unreachable",
    });
  });

  // A stale update object surviving a failed re-check would let the user
  // install something the endpoint no longer offers.
  it("forgets the previous answer before checking again", async () => {
    const update = pending();
    const check = vi.fn().mockResolvedValueOnce(update).mockRejectedValueOnce(new Error("offline"));
    const { controller } = harness({ currentVersion: vi.fn(), check, restart: vi.fn() });

    await controller.check();
    await controller.check();
    await controller.install();

    expect(update.install).not.toHaveBeenCalled();
  });
});

describe("installing", () => {
  it("does nothing when no check has found an update", async () => {
    const { controller, invoke } = harness({
      currentVersion: vi.fn(),
      check: vi.fn().mockResolvedValue(null),
      restart: vi.fn(),
    });

    await controller.check();
    await controller.install();

    expect(invoke).not.toHaveBeenCalled();
  });

  // The installer overwrites files the app is holding open, so the backend has
  // to be quiesced before the download hands off to it.
  it("quiesces the backend before handing off to the installer", async () => {
    const order: string[] = [];
    const invoke = vi.fn(async (command: string) => {
      order.push(command);
      return null;
    });
    const update = pending({
      install: vi.fn(async () => {
        order.push("install");
      }),
    });
    const { controller } = harness(
      { currentVersion: vi.fn(), check: vi.fn().mockResolvedValue(update), restart: vi.fn() },
      invoke,
    );

    await controller.check();
    await controller.install();

    expect(order).toEqual(["prepare_for_update", "install"]);
  });

  it("does not download when the backend refuses to quiesce", async () => {
    const update = pending();
    const invoke = vi.fn().mockRejectedValue(new Error("translation will not stop"));
    const { controller, patches } = harness(
      { currentVersion: vi.fn(), check: vi.fn().mockResolvedValue(update), restart: vi.fn() },
      invoke,
    );

    await controller.check();
    await controller.install();

    expect(update.install).not.toHaveBeenCalled();
    expect(patches.at(-1)?.update).toEqual({
      kind: "error",
      message: "translation will not stop",
    });
  });

  it("turns the plugin's chunk events into a rising percentage", async () => {
    const update = pending({
      install: vi.fn(async (onProgress: (event: never) => void) => {
        const emit = onProgress as unknown as (event: unknown) => void;
        emit({ event: "Started", data: { contentLength: 100 } });
        emit({ event: "Progress", data: { chunkLength: 40 } });
        emit({ event: "Progress", data: { chunkLength: 40 } });
        emit({ event: "Finished" });
      }),
    });
    const { controller, patches } = harness({
      currentVersion: vi.fn(),
      check: vi.fn().mockResolvedValue(update),
      restart: vi.fn(),
    });

    await controller.check();
    await controller.install();

    const percentages = patches
      .map((patch) => patch.update)
      .filter((status) => status?.kind === "downloading")
      .map((status) => (status.kind === "downloading" ? status.percent : undefined));
    expect(percentages).toEqual([null, 0, 40, 80]);
    expect(patches.at(-1)?.update).toEqual({ kind: "installing", version: "0.6.7" });
  });

  // The install ends this process, so a UI that still says a session is running
  // would be the last thing on screen before it disappears.
  it("stops claiming translation is running once the handoff starts", async () => {
    const update = pending();
    const { controller, patches } = harness({
      currentVersion: vi.fn(),
      check: vi.fn().mockResolvedValue(update),
      restart: vi.fn(),
    });

    await controller.check();
    await controller.install();

    expect(patches.some((patch) => patch.running === false)).toBe(true);
  });

  it("restarts into the installed version when the process survives the install", async () => {
    const restart = vi.fn().mockResolvedValue(undefined);
    const { controller } = harness({
      currentVersion: vi.fn(),
      check: vi.fn().mockResolvedValue(pending()),
      restart,
    });

    await controller.check();
    await controller.install();

    expect(restart).toHaveBeenCalledOnce();
  });

  it("reports a failed download instead of leaving the screen mid-progress", async () => {
    const update = pending({
      install: vi.fn().mockRejectedValue(new Error("signature verification failed")),
    });
    const { controller, patches } = harness({
      currentVersion: vi.fn(),
      check: vi.fn().mockResolvedValue(update),
      restart: vi.fn(),
    });

    await controller.check();
    await controller.install();

    expect(patches.at(-1)?.update).toEqual({
      kind: "error",
      message: "signature verification failed",
    });
  });
});
