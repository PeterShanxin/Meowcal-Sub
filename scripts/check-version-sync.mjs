import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function requireMatch(contents, pattern, sourceName) {
  const match = contents.match(pattern);
  if (!match) {
    throw new Error(`Could not read the product version from ${sourceName}.`);
  }
  return match[1];
}

const [packageJsonContents, packageLockContents, tauriContents, cargoContents, cargoLockContents] =
  await Promise.all([
    readFile(path.join(repositoryRoot, "package.json"), "utf8"),
    readFile(path.join(repositoryRoot, "package-lock.json"), "utf8"),
    readFile(path.join(repositoryRoot, "src-tauri", "tauri.conf.json"), "utf8"),
    readFile(path.join(repositoryRoot, "src-tauri", "Cargo.toml"), "utf8"),
    readFile(path.join(repositoryRoot, "src-tauri", "Cargo.lock"), "utf8"),
  ]);

const packageJson = JSON.parse(packageJsonContents);
const packageLock = JSON.parse(packageLockContents);
const tauriConfig = JSON.parse(tauriContents);
const cargoPackage = requireMatch(
  cargoContents,
  /\[package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/,
  "src-tauri/Cargo.toml",
);
const cargoLockPackage = requireMatch(
  cargoLockContents,
  /\[\[package\]\]\s*\nname\s*=\s*"meowcal-sub"\s*\nversion\s*=\s*"([^"]+)"/,
  "src-tauri/Cargo.lock",
);

const versions = new Map([
  ["package.json", packageJson.version],
  ["package-lock.json", packageLock.version],
  ['package-lock.json packages[""]', packageLock.packages?.[""]?.version],
  ["src-tauri/tauri.conf.json", tauriConfig.version],
  ["src-tauri/Cargo.toml", cargoPackage],
  ["src-tauri/Cargo.lock", cargoLockPackage],
]);
const distinctVersions = new Set(versions.values());
const expectedIndex = process.argv.indexOf("--expected");
const expectedVersion = expectedIndex >= 0 ? process.argv[expectedIndex + 1] : undefined;

if (expectedIndex >= 0 && !expectedVersion) {
  throw new Error("--expected requires a semantic version value.");
}
if (distinctVersions.size !== 1) {
  const details = [...versions].map(([source, version]) => `${source}: ${version}`).join("\n");
  throw new Error(`Product version records are not synchronized:\n${details}`);
}

const [actualVersion] = distinctVersions;
if (expectedVersion && actualVersion !== expectedVersion) {
  throw new Error(`Expected product version ${expectedVersion}, found ${actualVersion}.`);
}

console.log(`Product version records are synchronized at ${actualVersion}.`);
