import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  archiveCacheRelativePath,
  archiveZipUrl,
  buildActionCachePlan,
  collectCacheableActionUses,
  isSelfHostedRunsOn,
  parseActionUses,
  readJobActionUses,
  splitWorkflowJobs,
} from "../../scripts/action-cache-plan.mjs";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const workflowDirectory = path.join(repositoryRoot, ".github", "workflows");

function readRealWorkflows() {
  return readdirSync(workflowDirectory)
    .filter((name) => /\.ya?ml$/.test(name))
    .sort()
    .map((name) => ({
      path: path.posix.join(".github/workflows", name),
      contents: readFileSync(path.join(workflowDirectory, name), "utf8"),
    }));
}

describe("splitWorkflowJobs", () => {
  it("attributes each key to the job that owns it", () => {
    const jobs = splitWorkflowJobs(
      ["name: CI", "jobs:", "  lint:", "    runs-on: a", "  test:", "    runs-on: b"].join("\n"),
    );

    expect(jobs.map((job) => job.name)).toEqual(["lint", "test"]);
    expect(jobs[0].lines.join("\n")).toContain("runs-on: a");
    expect(jobs[0].lines.join("\n")).not.toContain("runs-on: b");
  });

  it("does not mistake a deeper valueless key for a job", () => {
    const jobs = splitWorkflowJobs(
      ["jobs:", "  build:", "    permissions:", "      contents: read", "    runs-on: a"].join(
        "\n",
      ),
    );

    expect(jobs.map((job) => job.name)).toEqual(["build"]);
  });

  it("stops at the end of the jobs block", () => {
    const jobs = splitWorkflowJobs(
      ["jobs:", "  build:", "    runs-on: a", "concurrency:", "  group: x"].join("\n"),
    );

    expect(jobs.map((job) => job.name)).toEqual(["build"]);
    expect(jobs[0].lines.join("\n")).not.toContain("group: x");
  });

  it("returns nothing for a file with no jobs block", () => {
    expect(splitWorkflowJobs("name: nothing\n")).toEqual([]);
  });
});

describe("isSelfHostedRunsOn", () => {
  it("recognises the label list form", () => {
    expect(isSelfHostedRunsOn("[self-hosted, Windows, ARM64, meowcal-ci]")).toBe(true);
  });

  it("rejects a hosted runner", () => {
    expect(isSelfHostedRunsOn("ubuntu-latest")).toBe(false);
    expect(isSelfHostedRunsOn("windows-11-arm")).toBe(false);
    expect(isSelfHostedRunsOn("windows-2025")).toBe(false);
  });
});

describe("readJobActionUses", () => {
  it("reads both the list-item and mapping forms", () => {
    expect(
      readJobActionUses(["      - uses: actions/checkout@v4", "        uses: actions/foo@v1"]),
    ).toEqual(["actions/checkout@v4", "actions/foo@v1"]);
  });

  it("ignores a commented-out step", () => {
    // A commented step is prose. Seeding the cache from it would download an
    // archive nothing uses and, worse, make the prune list disagree with reality.
    expect(readJobActionUses(["      # - uses: actions/retired@v1"])).toEqual([]);
  });
});

describe("parseActionUses", () => {
  it("splits owner, repository and ref", () => {
    expect(parseActionUses("actions/checkout@v4")).toEqual({
      nameWithOwner: "actions/checkout",
      ref: "v4",
      subPath: "",
    });
  });

  it("keeps the sub-path separate from the repository", () => {
    // The archive is the whole repository; the sub-path selects an action
    // inside it and must not become part of the cached name.
    expect(parseActionUses("owner/repo/tools/lint@main")).toEqual({
      nameWithOwner: "owner/repo",
      ref: "main",
      subPath: "tools/lint",
    });
  });

  it("returns null for references the archive cache cannot hold", () => {
    expect(parseActionUses("./.github/workflows/package.yml")).toBeNull();
    expect(parseActionUses("docker://alpine:3")).toBeNull();
    expect(parseActionUses("actions/checkout")).toBeNull();
    expect(parseActionUses("actions/checkout@")).toBeNull();
    expect(parseActionUses("owner//repo@v1")).toBeNull();
    expect(parseActionUses("")).toBeNull();
  });
});

describe("collectCacheableActionUses", () => {
  const workflow = [
    "jobs:",
    "  hosted:",
    "    runs-on: ubuntu-latest",
    "    steps:",
    "      - uses: actions/checkout@v4",
    "      - uses: actions/hosted-only@v9",
    "  ours:",
    "    runs-on: [self-hosted, Windows, meowcal-ci]",
    "    steps:",
    "      - uses: actions/checkout@v4",
    "      - uses: actions/upload-artifact@v4",
    "  folded:",
    "    runs-on: >-",
    "      ${{ inputs.architecture == 'arm64'",
    '      && fromJSON(\'["self-hosted", "meowcal-package-arm64"]\')',
    '      || fromJSON(\'["self-hosted", "meowcal-package-x64"]\') }}',
    "    steps:",
    "      - uses: actions/checkout@v4",
    "      - uses: actions/setup-node@v4",
  ].join("\n");

  it("collects only what a job on our own hosts would download", () => {
    // The cache is a property of the machine. A hosted Linux job cannot read
    // it, so caching its actions would be pure download with no cache hit.
    expect(collectCacheableActionUses([{ path: "w.yml", contents: workflow }])).toEqual([
      "actions/checkout@v4",
      "actions/setup-node@v4",
      "actions/upload-artifact@v4",
    ]);
  });

  it("sees through a folded runs-on that resolves to a self-hosted label", () => {
    expect(collectCacheableActionUses([{ path: "w.yml", contents: workflow }])).toContain(
      "actions/setup-node@v4",
    );
  });

  it("counts an action used by two jobs once", () => {
    const refs = collectCacheableActionUses([{ path: "w.yml", contents: workflow }]);
    expect(refs.filter((ref) => ref === "actions/checkout@v4")).toHaveLength(1);
  });
});

describe("this repository's own workflows", () => {
  it("derives the archives the self-hosted runners need", () => {
    // Pinned so that adding a step which uses a new action is a visible change
    // here rather than a silent extra download on every job.
    expect(collectCacheableActionUses(readRealWorkflows())).toEqual([
      "actions/checkout@v4",
      "actions/upload-artifact@v4",
    ]);
  });
});

describe("archiveCacheRelativePath", () => {
  const sha = "11d5960a326750d5838078e36cf38b85af677262";

  it("matches the layout ActionManager builds on Windows", () => {
    expect(archiveCacheRelativePath("actions/checkout", sha)).toBe(`actions_checkout\\${sha}.zip`);
  });

  it("refuses a ref where the resolved SHA belongs", () => {
    // Naming the file by the ref would serve stale bytes under a name that no
    // longer means them the moment a tag moves.
    expect(() => archiveCacheRelativePath("actions/checkout", "v4")).toThrow(/resolved/);
  });

  it("refuses a name that is not owner/repo", () => {
    expect(() => archiveCacheRelativePath("checkout", sha)).toThrow(/owner\/repo/);
  });
});

describe("archiveZipUrl", () => {
  it("is the codeload archive the runner would otherwise fetch", () => {
    const sha = "11d5960a326750d5838078e36cf38b85af677262";
    expect(archiveZipUrl("actions/checkout", sha)).toBe(
      `https://codeload.github.com/actions/checkout/zip/${sha}`,
    );
  });
});

describe("buildActionCachePlan", () => {
  const checkout = "11d5960a326750d5838078e36cf38b85af677262";
  const upload = "3d3c42e5aac5ba805825da76410c181273ba90b1";

  it("pairs every reference with the file to place", () => {
    const plan = buildActionCachePlan(["actions/checkout@v4"], {
      "actions/checkout@v4": { nameWithOwner: "actions/checkout", sha: checkout },
    });

    expect(plan).toEqual([
      {
        ref: "actions/checkout@v4",
        nameWithOwner: "actions/checkout",
        sha: checkout,
        relativePath: `actions_checkout\\${checkout}.zip`,
        url: `https://codeload.github.com/actions/checkout/zip/${checkout}`,
      },
    ]);
  });

  it("files a renamed repository under the name the API resolved", () => {
    // The runner writes the archive under ResolvedNameWithOwner, so caching it
    // under the workflow's spelling would be a permanent miss.
    const plan = buildActionCachePlan(["old-owner/checkout@v4"], {
      "old-owner/checkout@v4": { nameWithOwner: "actions/checkout", sha: checkout },
    });

    expect(plan[0].relativePath).toBe(`actions_checkout\\${checkout}.zip`);
  });

  it("collapses two refs that resolve to the same commit", () => {
    const plan = buildActionCachePlan(["actions/checkout@v4", "actions/checkout@v4.2.2"], {
      "actions/checkout@v4": { nameWithOwner: "actions/checkout", sha: checkout },
      "actions/checkout@v4.2.2": { nameWithOwner: "actions/checkout", sha: checkout },
    });

    expect(plan).toHaveLength(1);
  });

  it("keeps distinct commits apart", () => {
    const plan = buildActionCachePlan(["actions/checkout@v4", "actions/upload-artifact@v4"], {
      "actions/checkout@v4": { nameWithOwner: "actions/checkout", sha: checkout },
      "actions/upload-artifact@v4": { nameWithOwner: "actions/upload-artifact", sha: upload },
    });

    expect(plan.map((entry) => entry.relativePath)).toEqual([
      `actions_checkout\\${checkout}.zip`,
      `actions_upload-artifact\\${upload}.zip`,
    ]);
  });

  it("refuses to skip a reference it cannot resolve", () => {
    // Caching four of five actions leaves the fifth reaching codeload on every
    // job, which is the defect this exists to remove.
    expect(() => buildActionCachePlan(["actions/checkout@v4"], {})).toThrow(/No resolution/);
  });
});
