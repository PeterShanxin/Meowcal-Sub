import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
import vm from "node:vm";

function loadStartHelper(invoke) {
  const context = {
    console,
    window: {
      TauriBridge: { invoke },
      renderFoundryStatusChecking: vi.fn(),
    },
  };
  vm.runInNewContext(
    readFileSync(new URL("../../src/scripts/translation-start.js", import.meta.url), "utf8"),
    context,
  );
  return { helper: context.window.TranslationStart, window: context.window };
}

describe("managed translation start", () => {
  it("starts an installed but stopped engine without a second click", async () => {
    const invoke = vi.fn().mockResolvedValue({ phase: "ready" });
    const { helper, window } = loadStartHelper(invoke);
    const startButton = { disabled: false };

    const status = await helper.prepareManagedEngineForStart({ phase: "notRunning" }, startButton);

    expect(invoke).toHaveBeenCalledWith("make_engine_ready");
    expect(startButton.disabled).toBe(true);
    expect(window.renderFoundryStatusChecking).toHaveBeenCalledWith(
      "Starting the private translation engine...",
    );
    expect(status).toEqual({ phase: "ready" });
  });

  it.each([["ready"], ["notInstalled"], ["noModels"], ["error"]])(
    "does not start an engine in %s state",
    async (phase) => {
      const invoke = vi.fn();
      const { helper } = loadStartHelper(invoke);

      await expect(
        helper.prepareManagedEngineForStart({ phase }, { disabled: false }),
      ).resolves.toEqual({ phase });
      expect(invoke).not.toHaveBeenCalled();
    },
  );
});
