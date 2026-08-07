// Enforces the runner policy in docs/SELF_HOSTED_RUNNERS.md.
//
// The failure this prevents is quiet: someone adds a job, reaches for
// `windows-latest` out of habit or as a fallback for when the self-hosted host
// is offline, and the repository resumes spending paid Windows minutes with
// nothing visibly broken. Queuing is the designed behavior when no runner is
// online, so there is no legitimate hosted-Windows fallback to allow.

import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const workflowDirectory = path.join(repositoryRoot, ".github", "workflows");

// Hosted runners that bill above the Linux rate. macOS is included because it is
// the same mistake at ten times the multiplier.
const FORBIDDEN_RUNNER = /\b(windows-latest|windows-\d{4}|windows-11-arm|macos-[\w.-]+)\b/g;

// Linux hosted runners stay allowed: three release jobs use them deliberately,
// to keep RELEASE_MIRROR_TOKEN and release-write permission off a long-lived
// host that also executes contributor pull request code.
const ALLOWED_HOSTED = new Set(["ubuntu-latest", "ubuntu-24.04", "ubuntu-22.04"]);

function checkForbiddenRunners(relativePath, contents) {
  const violations = [];
  for (const line of contents.split(/\r?\n/).entries()) {
    const [index, text] = line;
    if (text.trimStart().startsWith("#")) {
      continue;
    }
    for (const match of text.matchAll(FORBIDDEN_RUNNER)) {
      violations.push(
        `${relativePath}:${index + 1}: '${match[0]}' is a paid hosted runner. ` +
          `Use a meowcal-* self-hosted label; jobs queue when no runner is online.`,
      );
    }
  }
  return violations;
}

function checkRunsOnValues(relativePath, contents) {
  const violations = [];
  const lines = contents.split(/\r?\n/);

  for (const [index, text] of lines.entries()) {
    const match = /^(\s*)runs-on:\s*(.*)$/.exec(text);
    if (!match) {
      continue;
    }

    const [, indent, inlineValue] = match;
    let value = inlineValue.trim();

    // A block scalar (`>-`, `|`) or a bare list continues on the following
    // more-indented lines. Gather them so the check sees the whole value.
    if (value === "" || value === ">-" || value === ">" || value === "|") {
      const collected = [];
      for (let next = index + 1; next < lines.length; next += 1) {
        const line = lines[next];
        if (line.trim() === "") {
          continue;
        }
        const lineIndent = /^\s*/.exec(line)[0].length;
        if (lineIndent <= indent.length) {
          break;
        }
        collected.push(line.trim());
      }
      value = collected.join(" ");
    }

    const bare = value
      .replace(/^\[|\]$/g, "")
      .replaceAll('"', "")
      .replaceAll("'", "");
    const isAllowedHosted = ALLOWED_HOSTED.has(bare.trim());
    const isSelfHosted = /self-hosted/.test(value);

    if (!isAllowedHosted && !isSelfHosted) {
      violations.push(
        `${relativePath}:${index + 1}: runs-on '${value}' is neither an allowed ` +
          `Linux hosted runner nor a self-hosted label set.`,
      );
    }

    if (isSelfHosted && !/meowcal-/.test(value)) {
      violations.push(
        `${relativePath}:${index + 1}: runs-on '${value}' uses the bare self-hosted ` +
          `label. Name a meowcal-* label so the job cannot land on an unintended runner.`,
      );
    }
  }

  return violations;
}

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
  violations.push(...checkForbiddenRunners(relativePath, contents));
  violations.push(...checkRunsOnValues(relativePath, contents));
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
