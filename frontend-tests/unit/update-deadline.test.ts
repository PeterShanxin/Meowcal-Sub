import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PendingUpdate, UiSnapshot } from "../../src/ui/contracts";
import { UpdateController } from "../../src/ui/update-controller";
import { deriveUpdatePresentation, UPDATE_CHECK_TIMEOUT_MS } from "../../src/ui/update-state";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function update(version = "0.9.1"): PendingUpdate {
  return {
    version,
    notes: null,
    install: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
  };
}

function harness(check: ReturnType<typeof vi.fn>) {
  const patches: Array<Partial<UiSnapshot>> = [];
  const invoke = vi.fn().mockResolvedValue(null);
  vi.stubGlobal("window", {
    TauriBridge: {
      invoke,
      isBrowserMode: () => false,
      updates: { currentVersion: vi.fn(), check, restart: vi.fn() },
    },
  });
  const controller = new UpdateController((patch) => patches.push(patch));
  return { controller, patches, invoke };
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date", "performance"] });
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("update check deadline", () => {
  it("ends a hung metadata check at ten seconds and enables retry", async () => {
    const first = deferred<PendingUpdate | null>();
    const check = vi.fn().mockReturnValueOnce(first.promise).mockResolvedValueOnce(null);
    const { controller, patches } = harness(check);
    const request = controller.check();

    await vi.advanceTimersByTimeAsync(UPDATE_CHECK_TIMEOUT_MS - 1);
    expect(patches.at(-1)?.update?.kind).toBe("checking");
    await vi.advanceTimersByTimeAsync(1);
    await request;

    const status = patches.at(-1)?.update;
    expect(status).toMatchObject({
      kind: "error",
      message: expect.stringContaining("update service"),
    });
    expect(deriveUpdatePresentation(status!, "0.9.0")).toMatchObject({
      action: "check",
      actionDisabled: false,
    });
    expect(check).toHaveBeenCalledWith({ timeout: 10_000 });
    await controller.check();
    expect(patches.at(-1)?.update?.kind).toBe("upToDate");
    expect(check).toHaveBeenCalledTimes(2);
  });

  it("joins the automatic check's remaining deadline after manual takeover", async () => {
    const check = vi.fn(() => new Promise<PendingUpdate | null>(() => {}));
    const { controller, patches, invoke } = harness(check);
    const automatic = controller.check("automatic");
    await vi.advanceTimersByTimeAsync(8_000);
    expect(patches).toEqual([]);
    const manual = controller.check();
    await vi.advanceTimersByTimeAsync(1_999);
    expect(patches.at(-1)?.update?.kind).toBe("checking");
    await vi.advanceTimersByTimeAsync(1);
    await Promise.all([automatic, manual]);

    expect(check).toHaveBeenCalledTimes(1);
    expect(patches.at(-1)?.update?.kind).toBe("error");
    const events = invoke.mock.calls
      .filter(([name]) => name === "record_update_check")
      .map(([, args]) => args.event);
    expect(events).toEqual([
      expect.objectContaining({
        source: "automatic",
        intent: "automatic",
        phase: "started",
        elapsedMs: 0,
      }),
      expect.objectContaining({
        source: "automatic",
        intent: "manual",
        phase: "manualTakeover",
        elapsedMs: 8_000,
      }),
      expect.objectContaining({
        source: "automatic",
        intent: "manual",
        phase: "finished",
        elapsedMs: 10_000,
        result: "timeout",
        errorKind: "timeout",
      }),
    ]);
  });

  it("keeps an automatic timeout quiet and leaves a known update installable", async () => {
    const known = update();
    const check = vi
      .fn()
      .mockResolvedValueOnce(known)
      .mockReturnValueOnce(new Promise(() => {}));
    const { controller, patches } = harness(check);
    await controller.check("automatic");
    const next = controller.check("automatic");
    await vi.advanceTimersByTimeAsync(10_000);
    await next;
    expect(patches.map((patch) => patch.update?.kind)).toEqual(["available"]);
    await controller.install();
    expect(known.install).toHaveBeenCalledOnce();
  });

  it.each(["resolve", "reject"] as const)(
    "ignores a late %s from a timed-out request",
    async (outcome) => {
      const first = deferred<PendingUpdate | null>();
      const second = deferred<PendingUpdate | null>();
      const check = vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
      const { controller, patches } = harness(check);
      const old = controller.check();
      await vi.advanceTimersByTimeAsync(10_000);
      await old;
      const retry = controller.check();
      const stale = update("9.0.0");
      if (outcome === "resolve") first.resolve(stale);
      else first.reject(new Error("late network failure"));
      await vi.advanceTimersByTimeAsync(1);
      expect(patches.at(-1)?.update?.kind).toBe("checking");
      second.resolve(null);
      await retry;
      expect(patches.at(-1)?.update?.kind).toBe("upToDate");
      expect(stale.close).toHaveBeenCalledTimes(outcome === "resolve" ? 1 : 0);
      expect(vi.getTimerCount()).toBe(0);
    },
  );

  it("clears the deadline on success and does not time out downloading", async () => {
    const download = deferred<void>();
    const found = update();
    found.install = vi.fn(() => download.promise);
    const { controller, patches } = harness(vi.fn().mockResolvedValue(found));
    await controller.check();
    expect(vi.getTimerCount()).toBe(0);
    const installation = controller.install();
    await vi.advanceTimersByTimeAsync(30_000);
    expect(patches.at(-1)?.update?.kind).toBe("downloading");
    download.resolve();
    await installation;
    expect(patches.at(-1)?.update?.kind).toBe("installing");
  });

  it("cancels a pending check on disposal and releases its late update", async () => {
    const first = deferred<PendingUpdate | null>();
    const { controller, patches, invoke } = harness(vi.fn(() => first.promise));
    const request = controller.check();
    controller.dispose();
    await request;
    const found = update();
    first.resolve(found);
    await vi.advanceTimersByTimeAsync(1);
    expect(found.close).toHaveBeenCalledOnce();
    expect(patches.map((patch) => patch.update?.kind)).toEqual(["checking"]);
    expect(invoke).toHaveBeenCalledWith("record_update_check", {
      event: expect.objectContaining({ result: "cancelled" }),
    });
    expect(vi.getTimerCount()).toBe(0);
  });

  it("releases a superseded update and any pending update when disposed", async () => {
    const found = update();
    const next = update("0.9.2");
    const { controller } = harness(
      vi.fn().mockResolvedValueOnce(found).mockResolvedValueOnce(next),
    );
    await controller.check();
    await controller.check();
    expect(found.close).toHaveBeenCalledOnce();
    controller.dispose();
    expect(next.close).toHaveBeenCalledOnce();
    await controller.check();
    await controller.install();
    expect(next.install).not.toHaveBeenCalled();
  });

  it("classifies native timeout and network failures without logging private details", async () => {
    const check = vi
      .fn()
      .mockRejectedValueOnce(new Error("request timed out https://private.example/?key=secret"))
      .mockRejectedValueOnce(new Error("dns error private-host"))
      .mockRejectedValueOnce(new Error("manifest invalid"));
    const { controller, patches, invoke } = harness(check);
    await controller.check();
    expect(patches.at(-1)?.update).toMatchObject({
      kind: "error",
      message: expect.not.stringContaining("secret"),
    });
    await controller.check();
    await controller.check();
    const outcomes = invoke.mock.calls
      .filter(([, args]) => args.event.phase === "finished")
      .map(([, args]) => args.event);
    expect(outcomes.map((event) => event.errorKind)).toEqual(["timeout", "network", "unknown"]);
    expect(JSON.stringify(outcomes)).not.toContain("secret");
    expect(JSON.stringify(outcomes)).not.toContain("private-host");
  });

  it("does not let diagnostic-write failures change the update result", async () => {
    const { controller, patches, invoke } = harness(vi.fn().mockResolvedValue(null));
    invoke.mockRejectedValue(new Error("log unavailable"));
    await controller.check();
    expect(patches.at(-1)?.update?.kind).toBe("upToDate");
  });

  it("keeps the install resource owned through preparation, download, and page disposal", async () => {
    const preparation = deferred<void>();
    const download = deferred<void>();
    const found = update();
    found.install = vi.fn(() => download.promise);
    const check = vi.fn().mockResolvedValue(found);
    const { controller, invoke, patches } = harness(check);
    invoke.mockImplementation((command) =>
      command === "prepare_for_update" ? preparation.promise : Promise.resolve(null),
    );
    await controller.check();
    const installation = controller.install();
    await controller.install();
    await controller.check("automatic");
    expect(await controller.checkAutomatic({})).toBeNull();
    expect(found.close).not.toHaveBeenCalled();
    expect(found.install).not.toHaveBeenCalled();
    preparation.resolve();
    await vi.advanceTimersByTimeAsync(30_000);
    expect(found.install).toHaveBeenCalledOnce();
    await controller.check();
    expect(check).toHaveBeenCalledOnce();
    controller.dispose();
    expect(found.close).not.toHaveBeenCalled();
    download.resolve();
    await installation;
    expect(found.close).toHaveBeenCalledOnce();
    expect(patches.at(-1)?.update?.kind).toBe("downloading");
    expect(invoke.mock.calls.filter(([command]) => command === "prepare_for_update")).toHaveLength(
      1,
    );
  });

  it("does not start the installer during a manual metadata check", async () => {
    const found = update();
    const next = deferred<PendingUpdate | null>();
    const { controller, invoke } = harness(
      vi.fn().mockResolvedValueOnce(found).mockReturnValueOnce(next.promise),
    );
    await controller.check("automatic");
    const checking = controller.check("manual");
    await controller.install();
    expect(found.install).not.toHaveBeenCalled();
    expect(invoke.mock.calls.some(([command]) => command === "prepare_for_update")).toBe(false);
    next.resolve(null);
    await checking;
    expect(found.close).toHaveBeenCalledOnce();
  });

  it("installs an available update after cancelling a background recheck", async () => {
    const found = update();
    const background = deferred<PendingUpdate | null>();
    const download = deferred<void>();
    found.install = vi.fn(() => download.promise);
    const { controller, patches, invoke } = harness(
      vi.fn().mockResolvedValueOnce(found).mockReturnValueOnce(background.promise),
    );
    await controller.check();
    const checking = controller.checkAutomatic({});
    const installation = controller.install();
    await controller.install();
    await vi.advanceTimersByTimeAsync(0);
    expect(await checking).toBeNull();
    expect(found.install).toHaveBeenCalledOnce();
    expect(found.close).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("record_update_check", {
      event: expect.objectContaining({ source: "automatic", result: "cancelled" }),
    });
    const stale = update("9.0.0");
    background.resolve(stale);
    await vi.advanceTimersByTimeAsync(0);
    expect(stale.close).toHaveBeenCalledOnce();
    expect(patches.at(-1)?.update).toMatchObject({ kind: "downloading", version: found.version });
    download.resolve();
    await installation;
    expect(found.close).toHaveBeenCalledOnce();
    expect(invoke.mock.calls.filter(([command]) => command === "prepare_for_update")).toHaveLength(
      1,
    );
  });

  it("does not begin a handoff if disposed while cancelling the background recheck", async () => {
    const found = update();
    const background = deferred<PendingUpdate | null>();
    const { controller, invoke } = harness(
      vi.fn().mockResolvedValueOnce(found).mockReturnValueOnce(background.promise),
    );
    await controller.check();
    const checking = controller.checkAutomatic({});
    const installation = controller.install();
    controller.dispose();
    await installation;
    expect(await checking).toBeNull();
    expect(found.close).toHaveBeenCalledOnce();
    expect(found.install).not.toHaveBeenCalled();
    expect(invoke.mock.calls.some(([command]) => command === "prepare_for_update")).toBe(false);
  });

  it("does not record cancelled automatic checks as completed", async () => {
    const { controller } = harness(vi.fn(() => new Promise(() => {})));
    const checking = controller.checkAutomatic({});
    controller.dispose();
    expect(await checking).toBeNull();
    expect(await controller.checkAutomatic({})).toBeNull();
  });
});
