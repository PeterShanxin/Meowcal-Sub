// Enforces the runner policy in docs/SELF_HOSTED_RUNNERS.md against every
// workflow file. The rules themselves live in workflow-runner-policy.mjs.

import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { findRunnerPolicyViolations } from "./workflow-runner-policy.mjs";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const workflowDirectory = path.join(repositoryRoot, ".github", "workflows");

const entries = await readdir(workflowDirectory, { withFileTypes: true });
const workflows = entries
  .filter((entry) => entry.isFile() && /\.ya?ml$/.test(entry.name))
  .map((entry) => entry.name)
  .sort();

if (workflows.length === 0) {
  console.error(`No workflow files found under ${workflowDirectory}.`);
  process.exit(1);
}

const violations = [];
for (const name of workflows) {
  const relativePath = path.posix.join(".github/workflows", name);
  const contents = await readFile(path.join(workflowDirectory, name), "utf8");
  violations.push(...findRunnerPolicyViolations(relativePath, contents));
}

if (violations.length > 0) {
  console.error("Runner policy violations:");
  for (const violation of violations) {
    console.error(`  ${violation}`);
  }
  console.error("\nSee docs/SELF_HOSTED_RUNNERS.md.");
  process.exit(1);
}

console.log(`Runner policy satisfied across ${workflows.length} workflow file(s).`);
