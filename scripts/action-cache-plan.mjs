// Derives which action archives the self-hosted runners need on disk, so a job
// never has to reach codeload.github.com to start.
//
// The runner deletes `_work\_actions` at the start of every job
// (ActionManager.PrepareActionsAsync), so its own per-action `.completed`
// watermark cannot survive into the next job and every job re-downloads
// `actions/checkout`. That download happens during job *initialization*, before
// any step exists, so nothing in a workflow can retry it: one bad response from
// codeload fails the job with `Caught exception from JobExtension
// Initialization` and names the action, which reads like a broken workflow.
// See issue #132 and docs/SELF_HOSTED_RUNNERS.md.
//
// The list is derived from the workflows rather than written down, so adding a
// step that uses a new action extends the cache with no second edit. Only jobs
// that actually run on our hosts are collected: the cache is a property of the
// machine, and a hosted Linux job cannot read it.

import { foldRunsOnValue, stripYamlComment } from "./workflow-runner-policy.mjs";

const SHA_PATTERN = /^[0-9a-f]{40}$/;
const NAME_WITH_OWNER_PATTERN = /^[^/\s]+\/[^/\s]+$/;

/**
 * Splits a workflow into its jobs, so a `uses:` can be attributed to the runner
 * that would execute it.
 *
 * Line-based on purpose, matching workflow-runner-policy.mjs: these files are
 * checked in and read by humans, and a YAML dependency for two shallow queries
 * would be a larger surface than the queries. A job header is a key with no
 * value at the first indent inside `jobs:`; every job-level key (`runs-on:`,
 * `name:`) carries a value, and the ones that do not (`steps:`, `permissions:`)
 * are indented deeper.
 */
export function splitWorkflowJobs(contents) {
  const lines = contents.split(/\r?\n/);
  const jobsIndex = lines.findIndex((line) => /^(\s*)jobs:\s*$/.test(stripYamlComment(line)));
  if (jobsIndex === -1) {
    return [];
  }

  const jobsIndent = /^\s*/.exec(lines[jobsIndex])[0].length;
  const jobs = [];
  let current = null;

  for (let index = jobsIndex + 1; index < lines.length; index += 1) {
    const line = stripYamlComment(lines[index]);
    if (line.trim() === "") {
      continue;
    }

    const indent = /^\s*/.exec(line)[0].length;
    if (indent <= jobsIndent) {
      break;
    }

    const header = /^\s*([A-Za-z_][\w.-]*):\s*$/.exec(line);
    if (header && (current === null || indent === current.indent)) {
      current = { name: header[1], indent, lines: [] };
      jobs.push(current);
      continue;
    }

    if (current) {
      current.lines.push(line);
    }
  }

  return jobs.map((job) => ({ name: job.name, lines: job.lines }));
}

/** True when a folded `runs-on:` value selects one of the owner's own hosts. */
export function isSelfHostedRunsOn(value) {
  return typeof value === "string" && /self-hosted/.test(value);
}

/**
 * The `uses:` values in one job's lines, in file order.
 *
 * Comments are stripped first: a commented-out step must not seed the cache,
 * and `# uses: actions/foo@v1` in an explanation is prose, not a dependency.
 */
export function readJobActionUses(lines) {
  const uses = [];
  for (const line of lines) {
    const match = /^\s*(?:-\s+)?uses:\s*(\S.*)$/.exec(stripYamlComment(line));
    if (match) {
      uses.push(match[1].trim());
    }
  }
  return uses;
}

/**
 * Splits `owner/repo[/sub/path]@ref` into its parts.
 *
 * Returns null for anything the archive cache cannot hold: a local workflow
 * reference (`./.github/...`), a container action (`docker://...`), or a value
 * with no ref at all.
 */
export function parseActionUses(uses) {
  const value = String(uses ?? "")
    .trim()
    .replace(/^["']|["']$/g, "");
  if (value === "" || value.startsWith("./") || value.startsWith(".\\")) {
    return null;
  }
  if (value.startsWith("docker://")) {
    return null;
  }

  const separator = value.lastIndexOf("@");
  if (separator <= 0 || separator === value.length - 1) {
    return null;
  }

  const reference = value.slice(0, separator);
  const ref = value.slice(separator + 1);
  const segments = reference.split("/");
  if (segments.length < 2 || segments.some((segment) => segment === "")) {
    return null;
  }

  const [owner, repository, ...rest] = segments;
  return {
    nameWithOwner: `${owner}/${repository}`,
    ref,
    subPath: rest.join("/"),
  };
}

/**
 * Every `owner/repo@ref` a self-hosted job would download, sorted and deduped.
 *
 * `workflows` is `[{ path, contents }]`. Two jobs using the same action produce
 * one entry, because the runner resolves both to the same archive.
 */
export function collectCacheableActionUses(workflows) {
  const found = new Set();

  for (const workflow of workflows) {
    for (const job of splitWorkflowJobs(workflow.contents)) {
      const runsOnIndex = job.lines.findIndex((line) => /^\s*runs-on:/.test(line));
      if (runsOnIndex === -1) {
        continue;
      }
      if (!isSelfHostedRunsOn(foldRunsOnValue(job.lines, runsOnIndex))) {
        continue;
      }

      for (const uses of readJobActionUses(job.lines)) {
        const parsed = parseActionUses(uses);
        if (parsed) {
          found.add(`${parsed.nameWithOwner}@${parsed.ref}`);
        }
      }
    }
  }

  return [...found].sort();
}

function assertResolvedName(nameWithOwner) {
  if (!NAME_WITH_OWNER_PATTERN.test(String(nameWithOwner ?? ""))) {
    throw new Error(`Not an owner/repo action name: '${nameWithOwner}'.`);
  }
}

function assertSha(sha) {
  if (!SHA_PATTERN.test(String(sha ?? ""))) {
    throw new Error(`Not a resolved 40-character commit SHA: '${sha}'.`);
  }
}

/**
 * Where the runner looks for a cached archive, relative to the cache root.
 *
 * `ActionManager` builds this itself as
 * `<cache>\<owner>_<repo>\<ResolvedSha>.zip` on Windows (the same code emits
 * `.tar.gz` on Linux), so this is a reimplementation of a path the runner owns.
 * Two consequences worth stating: the name is the **resolved SHA**, never the
 * ref, so a moved tag misses the cache and re-downloads rather than serving
 * stale bytes under a name that no longer means them; and it is the name the
 * runner **resolved**, so a renamed repository must be filed under its current
 * `full_name` even though the old name still answers.
 */
export function archiveCacheRelativePath(resolvedNameWithOwner, sha) {
  assertResolvedName(resolvedNameWithOwner);
  assertSha(sha);
  return `${resolvedNameWithOwner.replace(/[\\/]/g, "_")}\\${sha}.zip`;
}

/** The archive URL the runner would otherwise fetch for this action. */
export function archiveZipUrl(resolvedNameWithOwner, sha) {
  assertResolvedName(resolvedNameWithOwner);
  assertSha(sha);
  return `https://codeload.github.com/${resolvedNameWithOwner}/zip/${sha}`;
}

/**
 * Turns workflow references plus their resolutions into the archives to place.
 *
 * `resolutions` maps `owner/repo@ref` to `{ nameWithOwner, sha }` as answered by
 * the GitHub API. A reference with no resolution throws rather than being
 * skipped: silently caching four of five actions leaves the fifth reaching
 * codeload on every job, which is the defect this exists to remove.
 */
export function buildActionCachePlan(refs, resolutions) {
  const entries = new Map();

  for (const ref of refs) {
    const resolution = resolutions?.[ref];
    if (!resolution) {
      throw new Error(`No resolution supplied for '${ref}'.`);
    }

    const { nameWithOwner, sha } = resolution;
    const relativePath = archiveCacheRelativePath(nameWithOwner, sha);
    if (entries.has(relativePath)) {
      continue;
    }
    entries.set(relativePath, {
      ref,
      nameWithOwner,
      sha,
      relativePath,
      url: archiveZipUrl(nameWithOwner, sha),
    });
  }

  return [...entries.values()].sort((left, right) =>
    left.relativePath.localeCompare(right.relativePath),
  );
}
