import { readFile } from "node:fs/promises";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const baseline = JSON.parse(
  await readFile(path.join(repositoryRoot, "config", "maintainability-baseline.json"), "utf8"),
);
const eslintEntry = path.join(repositoryRoot, "node_modules", "eslint", "bin", "eslint.js");
const result = spawnSync(
  process.execPath,
  [
    eslintEntry,
    "src",
    "frontend-tests",
    "scripts/*.mjs",
    "*.config.{mjs,mts}",
    "--max-warnings",
    String(baseline.eslintMaxWarnings),
  ],
  {
    cwd: repositoryRoot,
    stdio: "inherit",
  },
);

if (result.error) {
  throw result.error;
}

process.exit(result.status ?? 1);
