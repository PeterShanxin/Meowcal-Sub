import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const expression = readFileSync(
  new URL("../acceptance-delayed-ocr.ps1", import.meta.url),
  "utf8",
).match(/\$stopExpression = @'\r?\n([\s\S]*?)\r?\n'@/)[1];

for (const scenario of ["healthy", "earlyExit", "stuckRunning", "overBudget", "missingBoundary"]) {
  test(`delayed cancellation: ${scenario}`, async () => {
    let running = scenario !== "earlyExit";
    let starts = 0;
    let clock = 0;
    const events = [];
    const invoke = async (command) => {
      if (command === "is_translation_running") return running;
      if (command === "stop_translation") {
        clock += scenario === "overBudget" ? 3100 : 220;
        running = scenario === "stuckRunning";
        if (scenario !== "missingBoundary") events.push({ displayState: "stopped", sessionId: 2 });
      }
      if (command === "start_translation") {
        starts++;
        running = true;
      }
    };
    const result = vm.runInNewContext(expression, {
      window: { TauriBridge: { invoke }, delayedEvents: events },
      performance: { now: () => clock },
    });
    if (scenario === "healthy") {
      assert.equal((await result).stoppedSessionId, 2);
      assert.equal(starts, 1);
    } else {
      await assert.rejects(result, /Capture exited|Stop left|budget|event missing/);
      assert.equal(starts, 0);
    }
  });
}

const source = readFileSync(new URL("../acceptance-delayed-ocr.ps1", import.meta.url), "utf8");
const embedded = (name) =>
  source.match(new RegExp(`\\$${name} = @'\\r?\\n([\\s\\S]*?)\\r?\\n'@`))[1];
const cleanup = (stop) =>
  embedded("cleanupExpression")
    .replace("TRIAL_TOKEN", "owned-trial")
    .replace("STOP_OWNED", String(stop));

for (const scenario of ["failedTrial", "passedTrial", "unrelatedTrial", "stopFailure"]) {
  test(`owned session cleanup: ${scenario}`, async () => {
    let stops = 0;
    let unlistens = 0;
    let running = true;
    const trial = {
      token: scenario === "unrelatedTrial" ? "another-trial" : "owned-trial",
      owned: true,
      unlisten: () => unlistens++,
    };
    const invoke = async (command) => {
      if (command === "stop_translation") {
        stops++;
        if (scenario === "stopFailure") throw new Error("stop failed");
        running = false;
      }
      if (command === "is_translation_running") return running;
    };
    const result = vm.runInNewContext(cleanup(scenario !== "passedTrial"), {
      window: { delayedTrial: trial, TauriBridge: { invoke } },
    });
    if (scenario === "stopFailure") await assert.rejects(result, /stop failed/);
    else await result;
    assert.equal(stops, ["failedTrial", "stopFailure"].includes(scenario) ? 1 : 0);
    assert.equal(unlistens, scenario === "unrelatedTrial" ? 0 : 1);
    if (scenario === "failedTrial") assert.equal(trial.owned, false);
  });
}

test("partially failed Start retains ownership for cleanup", async () => {
  let running = false;
  let unlistens = 0;
  const window = {
    TauriBridge: {
      event: { listen: async () => () => unlistens++ },
      invoke: async (command) => {
        if (command === "is_translation_running") return running;
        if (command === "start_translation") {
          running = true;
          throw new Error("start response failed");
        }
        if (command === "stop_translation") running = false;
      },
    },
  };
  await assert.rejects(
    vm.runInNewContext(embedded("startExpression").replace("TRIAL_TOKEN", "owned-trial"), {
      window,
    }),
    /start response failed/,
  );
  await vm.runInNewContext(cleanup(true), { window });
  assert.equal(running, false);
  assert.equal(unlistens, 1);
});

for (const scenario of [
  "visible",
  "hiddenClass",
  "cssHidden",
  "transparent",
  "offscreen",
  "emptyBounds",
]) {
  test(`native subtitle surface: ${scenario}`, async () => {
    const bounds = { left: 20, top: 20, right: 180, bottom: 60, width: 160, height: 40 };
    const textBounds = {
      ...bounds,
      left: scenario === "offscreen" ? -1 : 30,
      width: scenario === "emptyBounds" ? 0 : 140,
    };
    const container = {
      classList: {
        contains: (name) => name === (scenario === "hiddenClass" ? "hidden" : "visible"),
      },
      checkVisibility: () => !["cssHidden", "transparent"].includes(scenario),
      getBoundingClientRect: () => bounds,
    };
    const text = {
      textContent: "早上好。",
      checkVisibility: () => true,
      getBoundingClientRect: () => textBounds,
    };
    const result = await vm.runInNewContext(embedded("overlayExpression"), {
      window: {
        __TAURI__: { window: { getCurrentWindow: () => ({ isVisible: async () => true }) } },
      },
      document: { getElementById: (id) => (id === "subtitle-container" ? container : text) },
      innerWidth: 800,
      innerHeight: 600,
    });
    assert.equal(result.surfaceVisible, scenario === "visible");
  });
}
