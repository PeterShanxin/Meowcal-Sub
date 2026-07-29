import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const baselinePath = path.join(repositoryRoot, "config", "maintainability-baseline.json");
const productionRoots = ["src", "src-tauri/src"];
const productionExtensions = new Set([".css", ".html", ".js", ".rs"]);

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
const violations = [];

for (const relativePath of productionFiles) {
  const contents = await readFile(path.join(repositoryRoot, relativePath), "utf8");
  const lineCount = countLines(contents);
  measuredFiles.set(relativePath, lineCount);
  const ceiling = baseline.legacyFileMaxLines[relativePath] ?? baseline.newProductionFileMaxLines;

  if (lineCount > ceiling) {
    violations.push(`${relativePath}: ${lineCount} lines exceeds ceiling ${ceiling}`);
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
