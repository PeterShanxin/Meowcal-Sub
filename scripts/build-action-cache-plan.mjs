// Reads .github/workflows and reports what the action archive cache must hold.
// The rules themselves live in action-cache-plan.mjs, which touches no disk and
// no network so it can be tested against crafted workflows.
//
// Two modes, because resolving a ref to a commit SHA needs an authenticated
// GitHub call and this script deliberately makes none:
//
//   --refs                    one `owner/repo@ref` per line, no network
//   --plan <resolutions.json> the archives to place, as JSON
//
// scripts/sync-action-cache.ps1 runs the first, resolves each reference through
// `gh`, and feeds the answers back into the second.

import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { buildActionCachePlan, collectCacheableActionUses } from "./action-cache-plan.mjs";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const workflowDirectory = path.join(repositoryRoot, ".github", "workflows");

async function readWorkflows() {
  const entries = await readdir(workflowDirectory, { withFileTypes: true });
  const names = entries
    .filter((entry) => entry.isFile() && /\.ya?ml$/.test(entry.name))
    .map((entry) => entry.name)
    .sort();

  if (names.length === 0) {
    throw new Error(`No workflow files found under ${workflowDirectory}.`);
  }

  return Promise.all(
    names.map(async (name) => ({
      path: path.posix.join(".github/workflows", name),
      contents: await readFile(path.join(workflowDirectory, name), "utf8"),
    })),
  );
}

const [mode, argument] = process.argv.slice(2);

if (mode !== "--refs" && mode !== "--plan") {
  console.error("Usage: build-action-cache-plan.mjs --refs | --plan <resolutions.json>");
  process.exit(2);
}

const refs = collectCacheableActionUses(await readWorkflows());

if (refs.length === 0) {
  // Not an error state to reason about later: an empty plan is what would make
  // the sync prune a populated cache, so say so loudly here instead.
  console.error("No self-hosted job in .github/workflows uses a downloadable action.");
  process.exit(1);
}

if (mode === "--refs") {
  console.log(refs.join("\n"));
  process.exit(0);
}

if (!argument) {
  console.error("--plan needs the path of a JSON file mapping each ref to { nameWithOwner, sha }.");
  process.exit(2);
}

const resolutions = JSON.parse(await readFile(path.resolve(argument), "utf8"));
console.log(JSON.stringify(buildActionCachePlan(refs, resolutions), null, 2));
