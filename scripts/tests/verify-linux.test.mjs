import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { test } from "node:test";

const script = resolve(import.meta.dirname, "../verify-linux.sh");

// Stand-ins exercise the gate's exit propagation, not the checks themselves.
function gate(stage, failure = "") {
  const directory = mkdtempSync(join(tmpdir(), "meowcal-linux-gate-"));
  const log = join(directory, "calls");
  const command = `#!/usr/bin/env bash
set -euo pipefail
name="\${0##*/}"
echo "$name $*" >> "$GATE_CALLS"
if [[ "$name $*" == "$GATE_FAILURE"* && -n "$GATE_FAILURE" ]]; then exit 19; fi
if [[ "$name" == npm && "\${1:-}" == --version ]]; then echo 11.9.0; fi
if [[ "$name" == rustc ]]; then
  while [[ $# -gt 0 ]]; do
    if [[ "$1" == -o ]]; then
      shift
      printf '#!/usr/bin/env bash\\nexit "\${GATE_APP_EXIT:-0}"\\n' > "$1"
      chmod +x "$1"
      break
    fi
    shift
  done
fi
`;
  try {
    for (const tool of ["cargo", "rustc", "rustfmt", "pkg-config", "node", "npm"]) {
      writeFileSync(join(directory, tool), command, { mode: 0o755 });
    }
    const result = spawnSync("bash", [script, stage], {
      cwd: tmpdir(),
      env: {
        ...process.env,
        PATH: `${directory}:${process.env.PATH}`,
        GATE_CALLS: log,
        GATE_FAILURE: failure === "app-tests" ? "" : failure,
        GATE_APP_EXIT: failure === "app-tests" ? "19" : "0",
      },
      encoding: "utf8",
      timeout: 10_000,
    });
    assert.ifError(result.error);
    const calls = readFileSync(log, "utf8");
    return { ...result, calls };
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

test("full Linux gate runs backend and frontend checks from outside the checkout", () => {
  const result = gate("all");
  assert.equal(result.status, 0, result.stderr);
  for (const command of [
    "cargo test --manifest-path core/Cargo.toml --locked --all-targets",
    "rustc --edition 2021 --test",
    "npm ci --ignore-scripts",
    "npm run test:frontend",
    "npm run test:browser:linux",
    "npm audit --audit-level=high",
  ]) {
    assert.ok(result.calls.includes(command), `Missing ${command}`);
  }
  assert.match(result.stdout, /Linux all verification passed/);
});

for (const failure of [
  "cargo clippy",
  "cargo test",
  "rustc",
  "app-tests",
  "npm ci",
  "npm run test:frontend",
  "npm exec -- playwright install",
  "npm run test:browser:linux",
  "npm audit",
]) {
  test(`Linux gate fails closed when ${failure} fails`, () => {
    const result = gate("all", failure);
    assert.equal(result.status, 19, result.stderr);
    assert.doesNotMatch(result.stdout, /verification passed/);
    if (failure.startsWith("cargo") || failure === "rustc" || failure === "app-tests") {
      assert.doesNotMatch(result.calls, /npm ci/);
    }
  });
}

test("unknown stages cannot produce a passing gate", () => {
  const result = spawnSync("bash", [script, "All"], { encoding: "utf8" });
  assert.equal(result.status, 2);
  assert.match(result.stderr, /Unknown stage/);
});
