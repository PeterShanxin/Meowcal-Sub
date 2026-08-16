// Resolves every relative Markdown link in every tracked document. The rules
// live in doc-links.mjs.
//
//   node scripts/check-doc-links.mjs
//
// Only tracked files are read, so an ignored scratch note or a stale local
// worktree cannot fail the gate for everyone.

import { access, readFile } from "node:fs/promises";
import { execFile } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { collectLinkTargets } from "./doc-links.mjs";

const execFileAsync = promisify(execFile);
const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

async function trackedMarkdownFiles() {
  const { stdout } = await execFileAsync("git", ["ls-files", "-z", "*.md"], {
    cwd: repositoryRoot,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  });
  return stdout.split(String.fromCharCode(0)).filter((entry) => entry !== "");
}

async function exists(absolutePath) {
  try {
    await access(absolutePath);
    return true;
  } catch {
    return false;
  }
}

const documents = await trackedMarkdownFiles();
const broken = [];
let linkCount = 0;

for (const document of documents) {
  const contents = await readFile(path.join(repositoryRoot, document), "utf8");
  const documentDirectory = path.posix.dirname(document);

  for (const target of collectLinkTargets(contents)) {
    linkCount += 1;
    const resolved = target.startsWith("/")
      ? path.join(repositoryRoot, target.slice(1))
      : path.join(repositoryRoot, documentDirectory, target);

    if (!(await exists(resolved))) {
      broken.push(`${document}: '${target}' does not exist.`);
    }
  }
}

if (broken.length > 0) {
  console.error("Broken documentation links:");
  for (const entry of broken) {
    console.error(`  ${entry}`);
  }
  process.exit(1);
}

console.log(`Resolved ${linkCount} relative link(s) across ${documents.length} document(s).`);
