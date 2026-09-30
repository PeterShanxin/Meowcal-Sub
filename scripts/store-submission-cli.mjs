import { createHash } from "node:crypto";
import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { createGitHubClient, prepareRelease } from "./store-release-assets.mjs";
import {
  createStoreClient,
  requireCondition,
  safeFailure,
  stageDraft,
} from "./store-submission.mjs";

const root = path.dirname(fileURLToPath(import.meta.url));
async function main() {
  const argumentsMap = new Map();
  for (let i = 2; i < process.argv.length; i += 2) {
    requireCondition(
      process.argv[i]?.startsWith("--") &&
        process.argv[i + 1] &&
        !argumentsMap.has(process.argv[i]),
      "arguments_invalid",
    );
    argumentsMap.set(process.argv[i], process.argv[i + 1]);
  }
  const stage = argumentsMap.get("--stage") === "true";
  requireCondition(
    [...argumentsMap.keys()].every((key) => ["--tag", "--commit", "--stage"].includes(key)) &&
      argumentsMap.has("--tag") &&
      argumentsMap.has("--commit") &&
      (!argumentsMap.has("--stage") || ["true", "false"].includes(argumentsMap.get("--stage"))),
    "arguments_invalid",
  );
  requireCondition(process.env.GITHUB_TOKEN?.length > 0, "github_token_missing");
  const directory = await mkdtemp(path.join(tmpdir(), "meowcal-store-draft-"));
  const release = await prepareRelease(
    createGitHubClient(process.env.GITHUB_TOKEN),
    argumentsMap.get("--tag"),
    argumentsMap.get("--commit"),
    directory,
    (location, version) => {
      const result = spawnSync(
        "pwsh",
        [
          "-NoProfile",
          "-File",
          path.join(root, "inspect-store-release.ps1"),
          "-Directory",
          location,
          "-StoreVersion",
          version,
        ],
        { stdio: "ignore" },
      );
      requireCondition(result.status === 0, "msix_inspection_failed");
    },
  );
  console.log(
    JSON.stringify({
      release: argumentsMap.get("--tag"),
      commit: release.commit,
      storeVersion: release.storeVersion,
      verified: true,
      staged: false,
    }),
  );
  if (!stage) return;
  const bundlePath = path.join(directory, "submission.zip");
  const result = spawnSync(
    "pwsh",
    [
      "-NoProfile",
      "-File",
      path.join(root, "new-store-submission-zip.ps1"),
      "-Directory",
      directory,
      "-StoreVersion",
      release.storeVersion,
    ],
    { stdio: "ignore" },
  );
  requireCondition(result.status === 0, "submission_bundle_failed");
  const bytes = await readFile(bundlePath);
  requireCondition(bytes.length > 1000 && bytes.length < 300_000_000, "submission_bundle_invalid");
  const md5 = createHash("md5").update(bytes).digest("base64");
  const client = createStoreClient(process.env);
  const staged = await stageDraft(
    client,
    release,
    { bytes, md5 },
    {
      stage: true,
      onCreated: (submissionId) =>
        console.log(JSON.stringify({ submissionId, status: "created_uncommitted" })),
    },
  );
  console.log(JSON.stringify(staged));
}

try {
  await main();
} catch (error) {
  console.error(safeFailure(error));
  process.exitCode = 1;
}
