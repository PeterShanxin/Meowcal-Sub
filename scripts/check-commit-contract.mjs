// Enforces docs/CHANGE_CONTRACT.md against the commits a pull request adds and
// against its title. The rules themselves live in commit-contract.mjs.
//
//   node scripts/check-commit-contract.mjs --base origin/main --head HEAD
//
// The pull request title is read from the PR_TITLE environment variable and its
// author from PR_AUTHOR, so a title containing quotes or shell metacharacters
// never reaches a command line. Both are optional: a local run usually has no
// pull request yet.

import { execFile } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { findCommitViolations, findPullRequestTitleViolations } from "./commit-contract.mjs";

const execFileAsync = promisify(execFile);
const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

// Unit and record separators, which no commit message can contain, so a body
// with blank lines or quotes cannot break the parse.
const FIELD = String.fromCharCode(31);
const RECORD = String.fromCharCode(30);
const LOG_FORMAT = "%H%x1f%P%x1f%an%x1f%ae%x1f%s%x1f%b%x1e";

function readOption(name, fallback) {
  const index = process.argv.indexOf(`--${name}`);
  if (index < 0) {
    return fallback;
  }
  const value = process.argv[index + 1];
  if (!value || value.startsWith("--")) {
    throw new Error(`--${name} requires a value.`);
  }
  return value;
}

async function readCommits(base, head) {
  const { stdout } = await execFileAsync(
    "git",
    ["log", `--format=${LOG_FORMAT}`, `${base}..${head}`],
    { cwd: repositoryRoot, encoding: "utf8", maxBuffer: 32 * 1024 * 1024 },
  );

  return stdout
    .split(RECORD)
    .map((record) => record.replace(/^\r?\n/, ""))
    .filter((record) => record.trim() !== "")
    .map((record) => {
      const [sha, parents, author, email, subject, body = ""] = record.split(FIELD);
      return {
        sha,
        parents: parents.split(" ").filter(Boolean),
        author,
        email,
        subject,
        body,
      };
    });
}

const base = readOption("base", "origin/main");
const head = readOption("head", "HEAD");
const title = process.env.PR_TITLE ?? "";
const titleAuthor = process.env.PR_AUTHOR ?? "";
const titleBody = process.env.PR_BODY ?? "";

const commits = await readCommits(base, head);
const violations = [
  ...findCommitViolations(commits),
  ...(title ? findPullRequestTitleViolations(title, { author: titleAuthor, body: titleBody }) : []),
];

if (violations.length > 0) {
  console.error("Change contract violations:");
  for (const violation of violations) {
    console.error(`  ${violation}`);
  }
  console.error(
    "\nSee docs/CHANGE_CONTRACT.md. Amend or reword the branch; no published history is rewritten.",
  );
  process.exit(1);
}

const titleNote = title ? " and the pull request title" : "";
console.log(`Change contract satisfied across ${commits.length} commit(s)${titleNote}.`);
