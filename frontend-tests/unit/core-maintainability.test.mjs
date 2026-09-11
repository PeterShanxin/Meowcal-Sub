import { execFileSync, spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";

it("measures Core source and rejects a file above the production ceiling", () => {
  const root = mkdtempSync(path.join(tmpdir(), "meowcal-core-ratchet-"));
  const source = fileURLToPath(new URL("../../scripts/", import.meta.url));
  try {
    for (const directory of ["scripts", "config", "src", "src-tauri/src", "core/src"]) {
      mkdirSync(path.join(root, directory), { recursive: true });
    }
    for (const file of ["check-maintainability.mjs", "maintainability-ratchet.mjs"]) {
      copyFileSync(path.join(source, file), path.join(root, "scripts", file));
    }
    writeFileSync(
      path.join(root, "config/maintainability-baseline.json"),
      JSON.stringify({
        newProductionFileMaxLines: 400,
        eslintMaxWarnings: 0,
        frontendCoverageMinimum: {},
        frontendCoverageScope: [],
        legacyFileMaxLines: {},
      }),
    );
    const file = path.join(root, "core/src/service.rs");
    writeFileSync(file, "// measured\n".repeat(400));
    const command = path.join(root, "scripts/check-maintainability.mjs");
    expect(execFileSync(process.execPath, [command], { cwd: root, encoding: "utf8" })).toContain(
      "passed for 1 production files",
    );
    writeFileSync(file, "// measured\n".repeat(401));
    const rejected = spawnSync(process.execPath, [command], { cwd: root, encoding: "utf8" });
    expect(rejected.status).toBe(1);
    expect(rejected.stderr).toContain("core/src/service.rs: 401 lines exceeds ceiling 400");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
