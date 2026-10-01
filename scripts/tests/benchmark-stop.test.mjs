import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const native = readFileSync(new URL("../benchmark-native-stop.ps1", import.meta.url), "utf8")
  .match(/\$expression = @'\r?\n([\s\S]*?)\r?\n'@/)[1]
  .replace("SAMPLE_COUNT", "3");
const delayed = readFileSync(
  new URL("../benchmark-delayed-ocr.ps1", import.meta.url),
  "utf8",
).match(/\$result = Invoke-BenchmarkScript '([^']+)'/)[1];

for (const [name, expression] of [
  ["native", native],
  ["delayed", delayed],
]) {
  test(`${name}: early capture exit cannot produce a timed sample`, async () => {
    let stops = 0;
    const invoke = async (command) => {
      if (command === "is_translation_running") return false;
      if (command === "stop_translation") stops++;
    };
    const result = vm.runInNewContext(expression, {
      window: { TauriBridge: { invoke } },
      setTimeout: (callback) => callback(),
      performance: { now: () => 100 },
    });
    await assert.rejects(result, /Capture session exited before/);
    assert.equal(stops, name === "native" ? 1 : 0); // Native finally stops its own session.
  });
  test(`${name}: healthy capture produces measured stopped rows`, async () => {
    let running = name === "delayed";
    const invoke = async (command) => {
      if (command === "is_translation_running") return running;
      if (command === "start_translation") running = true;
      if (command === "stop_translation") running = false;
    };
    let clock = 0;
    const result = await vm.runInNewContext(expression, {
      window: { TauriBridge: { invoke } },
      setTimeout: (callback) => callback(),
      performance: { now: () => ++clock },
    });
    const rows = name === "native" ? result : [result];
    assert.equal(rows.length, name === "native" ? 3 : 1);
    for (const row of rows) {
      assert.equal(row.running, false);
      assert.equal(row.error, null);
      assert.equal(row.stoppedMs ?? row.stopMs, 1);
    }
  });
}
