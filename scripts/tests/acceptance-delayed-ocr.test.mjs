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
