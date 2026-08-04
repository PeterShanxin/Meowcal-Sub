#!/usr/bin/env node
// =============================================================================
// BUILD-UPDATER-MANIFEST - the latest.json the in-app updater reads
// =============================================================================
// The updater asks one URL what the newest version is, and gets back a per
// platform installer URL and the minisign signature over it. Getting the
// platform keys the wrong way round hands an ARM64 machine an x64 installer,
// which installs and then does not run - so the mapping is derived from the
// artifact names and asserted, never assumed.
//
// Usage:
//   node scripts/build-updater-manifest.mjs \
//     --directory release-assets --version 0.6.6 --tag v0.6.6 \
//     --repo owner/name --notes-file docs/releases/v0.6.6.md \
//     --output release-assets/latest.json
// =============================================================================

import { readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

/** Tauri's architecture token in an artifact name, to the updater's platform key. */
export const UPDATER_PLATFORMS = Object.freeze({
  x64: "windows-x86_64",
  arm64: "windows-aarch64",
});

const INSTALLER_SUFFIX = "-setup.exe";
const SIGNATURE_SUFFIX = ".sig";

/**
 * GitHub rewrites anything outside `[A-Za-z0-9._-]` to a period when it stores
 * a release asset, so `Meowcal Sub_0.6.6_x64-setup.exe` is downloadable only as
 * `Meowcal.Sub_0.6.6_x64-setup.exe`. Building the URL from the local file name
 * produces a manifest that 404s for every user.
 */
export function githubAssetName(fileName) {
  return fileName.replace(/[^A-Za-z0-9._-]/g, ".");
}

function architectureOf(fileName) {
  const name = fileName.toLowerCase();
  // Checked before x64 because neither token is a substring of the other, and
  // relying on that quietly would break the day a name carries both.
  if (name.includes("arm64")) return "arm64";
  if (name.includes("x64")) return "x64";
  return null;
}

/**
 * Pick the NSIS installer and its signature for each architecture.
 *
 * Throws rather than skipping: a manifest missing an architecture silently
 * stops updating every machine of that kind, and nothing else would notice.
 */
export function selectUpdaterArtifacts(fileNames) {
  const installers = new Map();
  for (const fileName of fileNames) {
    if (!fileName.endsWith(INSTALLER_SUFFIX)) continue;
    const architecture = architectureOf(fileName);
    if (architecture === null) {
      throw new Error(`Installer with no recognisable architecture: ${fileName}`);
    }
    const existing = installers.get(architecture);
    if (existing !== undefined) {
      throw new Error(`Two ${architecture} installers found: ${existing} and ${fileName}`);
    }
    installers.set(architecture, fileName);
  }

  const available = new Set(fileNames);
  const selected = {};
  for (const [architecture, platform] of Object.entries(UPDATER_PLATFORMS)) {
    const installer = installers.get(architecture);
    if (installer === undefined) {
      throw new Error(`No ${architecture} installer found for ${platform}.`);
    }
    const signature = `${installer}${SIGNATURE_SUFFIX}`;
    if (!available.has(signature)) {
      throw new Error(
        `Missing ${signature}. The build did not sign ${installer}; check that ` +
          "TAURI_SIGNING_PRIVATE_KEY was set for the packaging job.",
      );
    }
    selected[platform] = { installer, signature };
  }
  return selected;
}

/**
 * Assemble the manifest. `signatures` maps a signature file name to its
 * contents, which the updater compares against the downloaded installer.
 */
export function buildUpdaterManifest({ artifacts, signatures, version, notes, pubDate, baseUrl }) {
  const platforms = {};
  for (const [platform, { installer, signature }] of Object.entries(artifacts)) {
    const content = signatures[signature];
    if (typeof content !== "string" || content.trim() === "") {
      throw new Error(`Signature ${signature} is empty; the updater would reject the download.`);
    }
    platforms[platform] = {
      signature: content.trim(),
      url: `${baseUrl}/${githubAssetName(installer)}`,
    };
  }
  return { version, notes, pub_date: pubDate, platforms };
}

function parseArguments(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    if (!flag.startsWith("--")) throw new Error(`Unexpected argument: ${flag}`);
    const value = argv[index + 1];
    if (value === undefined) throw new Error(`Missing value for ${flag}`);
    options[flag.slice(2)] = value;
  }
  for (const required of ["directory", "version", "tag", "repo", "output"]) {
    if (!options[required]) throw new Error(`Missing required --${required}`);
  }
  return options;
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const entries = await readdir(options.directory, { withFileTypes: true, recursive: true });
  const files = entries.filter((entry) => entry.isFile());
  const artifacts = selectUpdaterArtifacts(files.map((entry) => entry.name));

  const locate = (name) => {
    const entry = files.find((candidate) => candidate.name === name);
    return path.join(entry.parentPath ?? entry.path ?? options.directory, name);
  };

  const signatures = {};
  for (const { signature } of Object.values(artifacts)) {
    signatures[signature] = await readFile(locate(signature), "utf8");
  }

  const notes = options["notes-file"] ? (await readFile(options["notes-file"], "utf8")).trim() : "";

  const manifest = buildUpdaterManifest({
    artifacts,
    signatures,
    version: options.version,
    notes,
    pubDate: options["pub-date"] ?? new Date().toISOString(),
    baseUrl: `https://github.com/${options.repo}/releases/download/${options.tag}`,
  });

  await writeFile(options.output, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
  for (const [platform, entry] of Object.entries(manifest.platforms)) {
    console.log(`${platform} -> ${entry.url}`);
  }
}

// Only run when invoked directly, so the tests can import the pure helpers.
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
