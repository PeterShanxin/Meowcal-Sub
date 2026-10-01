import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = readFileSync(new URL("store-direct-coexistence.ps1", import.meta.url), "utf8");
const expression = source.match(/\$settings = Invoke-AppScript @"\r?\n([\s\S]*?)\r?\n"@/)[1];

test("baseline language survives a later UI snapshot save", async () => {
  let stored = { sourceLanguage: "en-US", autoCheckUpdates: true };
  let snapshot = { ...stored };
  const controller = {
    current: () => ({ busy: "idle", settings: snapshot }),
    async updatePreference(key, value) {
      snapshot = { ...snapshot, [key]: value };
      stored = { ...snapshot };
    },
    async setLanguage(kind, value) {
      assert.equal(kind, "source");
      snapshot = { ...snapshot, sourceLanguage: value };
      stored = { ...snapshot };
    },
  };
  const result = await vm.runInNewContext(expression, {
    document: { querySelector: () => ({ controller }) },
    window: { __TAURI__: { core: { invoke: async () => ({ ...stored }) } } },
  });
  assert.equal(result.sourceLanguage, "ja-JP");
  assert.equal(snapshot.autoCheckUpdates, false);
  // This is the pinned app's later full-snapshot save, which reverted the RPC seed.
  stored = { ...snapshot, lastUpdateCheckTimeMs: 123 };
  assert.equal(stored.sourceLanguage, "ja-JP");
  assert.match(source, /Direct baseline language did not survive its initial close/);
});

test("missing settings UI fails before seeding a baseline", async () => {
  let time = 0;
  await assert.rejects(
    vm.runInNewContext(expression, {
      Date: { now: () => (time += 31000) },
      document: { querySelector: () => null },
    }),
    /settings UI did not initialize/,
  );
});
