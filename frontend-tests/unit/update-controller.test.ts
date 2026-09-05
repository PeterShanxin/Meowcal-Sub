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

  it("prevents overlapping concurrent checks", async () => {
    let resolveFirst: (value: PendingUpdate | null) => void = () => {};
    const firstCheck = new Promise<PendingUpdate | null>((res) => {
      resolveFirst = res;
    });
    const check = vi.fn().mockReturnValueOnce(firstCheck);
    const { controller } = harness({ currentVersion: vi.fn(), check, restart: vi.fn() });

    const call1 = controller.check();
    const call2 = controller.check();
    resolveFirst(null);
    await Promise.all([call1, call2]);

    expect(check).toHaveBeenCalledTimes(1);
  });
});

describe("automatic checking", () => {
  it("publishes available without publishing checking when update is found", async () => {
    const update = pending();
    const { controller, patches } = harness({
      currentVersion: vi.fn(),
      check: vi.fn().mockResolvedValue(update),
      restart: vi.fn(),
    });

    await controller.check("automatic");

    expect(patches.map((p) => p.update?.kind)).toEqual(["available"]);
    expect(patches[0]?.update).toEqual({
      kind: "available",
      version: update.version,
      notes: update.notes,
    });
  });

  it("publishes upToDate quietly without publishing checking when no update is found", async () => {
    const { controller, patches } = harness({
      currentVersion: vi.fn(),
      check: vi.fn().mockResolvedValue(null),
      restart: vi.fn(),
    });

    await controller.check("automatic");

    expect(patches.map((p) => p.update?.kind)).toEqual(["upToDate"]);
  });

  it("fails quietly on network error without publishing error or clearing pending update", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const update = pending();
    const check = vi
      .fn()
      .mockResolvedValueOnce(update)
      .mockRejectedValueOnce(new Error("network error"));
    const { controller, patches } = harness({ currentVersion: vi.fn(), check, restart: vi.fn() });

    await controller.check("automatic");
    expect(patches.at(-1)?.update?.kind).toBe("available");

    // Second check automatically fails quietly
    await controller.check("automatic");
    expect(patches.some((p) => p.update?.kind === "error")).toBe(false);
    expect(warnSpy).toHaveBeenCalled();

    // The pending update was kept intact and can still be installed
    await controller.install();
    expect(update.install).toHaveBeenCalled();
  });

  it("does not trigger installation during automatic check", async () => {
    const update = pending();
    const { controller } = harness({
      currentVersion: vi.fn(),
      check: vi.fn().mockResolvedValue(update),
      restart: vi.fn(),
    });

    await controller.check("automatic");

    expect(update.install).not.toHaveBeenCalled();
  });

  it("skips checkAutomatic in browser mode", async () => {
    const { controller } = harness(undefined);

    const result = await controller.checkAutomatic({}, () => 12345);

    expect(result).toBeNull();
  });

  it("skips checkAutomatic when autoCheckUpdates is disabled or not due", async () => {
    const check = vi.fn().mockResolvedValue(null);
    const { controller } = harness({ currentVersion: vi.fn(), check, restart: vi.fn() });
    const now = 1_700_000_000_000;

    const disabledResult = await controller.checkAutomatic({ autoCheckUpdates: false }, () => now);
    expect(disabledResult).toBeNull();
    expect(check).not.toHaveBeenCalled();

    const notDueResult = await controller.checkAutomatic(
      { lastUpdateCheckTimeMs: now - 1000 },
      () => now,
    );
    expect(notDueResult).toBeNull();
    expect(check).not.toHaveBeenCalled();
  });

  it("runs checkAutomatic when due and returns clock timestamp", async () => {
    const check = vi.fn().mockResolvedValue(null);
    const { controller } = harness({ currentVersion: vi.fn(), check, restart: vi.fn() });
    const now = 1_700_000_000_000;

    const result = await controller.checkAutomatic({}, () => now);

    expect(result).toBe(now);
    expect(check).toHaveBeenCalledTimes(1);
  });

  it("uses default Date.now clock if none provided", async () => {
    const check = vi.fn().mockResolvedValue(null);
    const { controller } = harness({ currentVersion: vi.fn(), check, restart: vi.fn() });
    const now = 1_700_000_000_000;
    vi.spyOn(Date, "now").mockReturnValue(now);

    const result = await controller.checkAutomatic({});

    expect(result).toBe(now);
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
