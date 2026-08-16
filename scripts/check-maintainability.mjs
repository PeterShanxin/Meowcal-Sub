import { existsSync } from "node:fs";
import { readdir, readFile } from "node:fs/promises";
import { execFile } from "node:child_process";
import path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import { findRatchetRegressions } from "./maintainability-ratchet.mjs";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const baselinePath = path.join(repositoryRoot, "config", "maintainability-baseline.json");
const productionRoots = ["src", "src-tauri/src"];
const productionExtensions = new Set([".css", ".html", ".js", ".rs", ".ts"]);
const execFileAsync = promisify(execFile);

async function listProductionFiles(relativeDirectory) {
  const absoluteDirectory = path.join(repositoryRoot, relativeDirectory);
  const entries = await readdir(absoluteDirectory, { withFileTypes: true });
  const files = [];

  for (const entry of entries) {
    const relativePath = path.posix.join(relativeDirectory.replaceAll("\\", "/"), entry.name);
    if (entry.isDirectory()) {
      files.push(...(await listProductionFiles(relativePath)));
    } else if (productionExtensions.has(path.extname(entry.name))) {
      files.push(relativePath);
    }
  }

  return files;
}

function countLines(contents) {
  const lines = contents.split(/\r?\n/);
  if (lines.at(-1) === "") {
    lines.pop();
  }
  return lines.length;
}

function requireNonNegativeInteger(value, name) {
  if (!Number.isInteger(value) || value < 0) {
    throw new Error(`${name} must be a non-negative integer.`);
  }
}

const baseline = JSON.parse(await readFile(baselinePath, "utf8"));
let previousBaseline = null;
try {
  const { stdout: headContents } = await execFileAsync(
    "git",
    ["show", "HEAD:config/maintainability-baseline.json"],
    { cwd: repositoryRoot, encoding: "utf8" },
  );
  const headBaseline = JSON.parse(headContents);
  if (JSON.stringify(headBaseline) !== JSON.stringify(baseline)) {
    previousBaseline = headBaseline;
  } else {
    const { stdout: parentContents } = await execFileAsync(
      "git",
      ["show", "HEAD^:config/maintainability-baseline.json"],
      { cwd: repositoryRoot, encoding: "utf8" },
    );
    previousBaseline = JSON.parse(parentContents);
  }
} catch {
  console.warn("Previous maintainability baseline unavailable; checking current ceilings only.");
}
requireNonNegativeInteger(baseline.newProductionFileMaxLines, "newProductionFileMaxLines");
requireNonNegativeInteger(baseline.eslintMaxWarnings, "eslintMaxWarnings");

for (const [metric, minimum] of Object.entries(baseline.frontendCoverageMinimum)) {
  requireNonNegativeInteger(minimum, `frontendCoverageMinimum.${metric}`);
  if (minimum > 100) {
    throw new Error(`frontendCoverageMinimum.${metric} cannot exceed 100.`);
  }
}

const productionFiles = (await Promise.all(productionRoots.map(listProductionFiles))).flat();
const measuredFiles = new Map();
// The ratchet stays pure; the filesystem question - does this scoped module
// still exist? - is answered here, so a deleted module can leave the scope while
// a live one still cannot.
const fileExists = (relativePath) => existsSync(path.join(repositoryRoot, relativePath));
const violations = previousBaseline
  ? findRatchetRegressions(baseline, previousBaseline, { fileExists })
  : [];

for (const relativePath of baseline.frontendCoverageScope ?? []) {
  if (!fileExists(relativePath)) {
    violations.push(`frontendCoverageScope names ${relativePath}, which does not exist`);
  }
}

for (const relativePath of productionFiles) {
  const contents = await readFile(path.join(repositoryRoot, relativePath), "utf8");
  const lineCount = countLines(contents);
  measuredFiles.set(relativePath, lineCount);
  const ceiling = baseline.legacyFileMaxLines[relativePath] ?? baseline.newProductionFileMaxLines;

  if (lineCount > ceiling) {
    violations.push(`${relativePath}: ${lineCount} lines exceeds ceiling ${ceiling}`);
  } else if (Object.hasOwn(baseline.legacyFileMaxLines, relativePath) && lineCount < ceiling) {
    violations.push(
      `${relativePath}: lower the legacy ceiling from ${ceiling} to the measured ${lineCount} lines`,
    );
  }
}

for (const [relativePath, ceiling] of Object.entries(baseline.legacyFileMaxLines)) {
  requireNonNegativeInteger(ceiling, `legacyFileMaxLines.${relativePath}`);
  if (!measuredFiles.has(relativePath)) {
    violations.push(`${relativePath}: legacy exception points to a missing production file`);
  } else if (ceiling <= baseline.newProductionFileMaxLines) {
    violations.push(
      `${relativePath}: remove the legacy exception now that its ceiling is at or below the new-file ceiling`,
    );
  }
}

if (violations.length > 0) {
  console.error("Maintainability ratchet failed:");
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  process.exit(1);
}

console.log(
  `Maintainability ratchet passed for ${productionFiles.length} production files ` +
    `(${Object.keys(baseline.legacyFileMaxLines).length} explicit legacy exceptions).`,
);
