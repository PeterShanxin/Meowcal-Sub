import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { isSelfHostedRunsOn, splitWorkflowJobs } from "../../scripts/action-cache-plan.mjs";
import { foldRunsOnValue } from "../../scripts/workflow-runner-policy.mjs";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

const OWNER_ACTOR = "github.actor == 'PeterShanxin'";
const NOT_DEPENDABOT = "github.actor != 'dependabot[bot]'";

function readWorkflow(name) {
  return readFileSync(path.join(repositoryRoot, ".github/workflows", name), "utf8");
}

function jobText(job) {
  return job.lines.join("\n");
}

function jobRunsOn(job) {
  for (let index = 0; index < job.lines.length; index += 1) {
    const value = foldRunsOnValue(job.lines, index);
    if (value !== null) {
      return value;
    }
  }
  return null;
}

function expectOwnerActorGate(job, label) {
  const text = jobText(job);
  expect(text, `${label} missing owner actor`).toContain(OWNER_ACTOR);
  expect(text, `${label} missing Dependabot exclusion`).toContain(NOT_DEPENDABOT);
}

describe("self-hosted CI jobs stay off untrusted pull requests", () => {
  it("gives every test.yml job the owner-only if and a credential-free checkout", () => {
    const contents = readWorkflow("test.yml");
    expect(contents).toMatch(/^permissions:\n {2}contents: read$/m);
    expect(contents).toMatch(/^ {2}pull_request:$/m);

    const jobs = splitWorkflowJobs(contents);

    expect(jobs.map((job) => job.name)).toEqual(["lint", "test", "frontend"]);

    for (const job of jobs) {
      const text = jobText(job);
      expectOwnerActorGate(job, job.name);
      expect(text, job.name).toContain("github.event_name == 'push'");
      expect(text, job.name).toContain(
        "github.event.pull_request.head.repo.full_name == github.repository",
      );
      expect(text, job.name).toContain("github.event.pull_request.user.login == 'PeterShanxin'");
      expect(text, job.name).toContain("persist-credentials: false");
      expect(text, job.name).toContain("clean: false");
    }
  });

  it("requires the owner actor on push, not only on pull_request", () => {
    const contents = readWorkflow("test.yml");
    for (const job of splitWorkflowJobs(contents)) {
      const text = jobText(job).replace(/\s+/g, " ");
      expect(text, job.name).toMatch(
        /github\.actor == 'PeterShanxin' && github\.actor != 'dependabot\[bot\]' && \(github\.event_name == 'push'/,
      );
    }
  });

  it("does not add pull_request to maintainer-only packaging and release workflows", () => {
    for (const name of ["package.yml", "release.yml", "publish-update.yml"]) {
      const contents = readWorkflow(name);
      expect(contents, name).not.toMatch(/^\s*pull_request:/m);
    }
  });
});

describe("write access is not enough to run privileged jobs", () => {
  it("gates every self-hosted job on the owner actor", () => {
    const selfHosted = [];
    for (const name of ["test.yml", "package.yml", "release.yml", "publish-update.yml"]) {
      for (const job of splitWorkflowJobs(readWorkflow(name))) {
        const runsOn = jobRunsOn(job);
        if (isSelfHostedRunsOn(runsOn)) {
          selfHosted.push(`${name}:${job.name}`);
          expectOwnerActorGate(job, `${name}:${job.name}`);
        }
      }
    }
    expect(selfHosted.sort()).toEqual([
      "package.yml:package",
      "test.yml:frontend",
      "test.yml:lint",
      "test.yml:test",
    ]);
  });

  it("gates packaging, release, and publish-update jobs on the owner actor", () => {
    const expected = {
      "package.yml": ["package"],
      "release.yml": ["validate", "package-x64", "package-arm64", "draft-release"],
      "publish-update.yml": ["publish"],
    };
    for (const [name, jobNames] of Object.entries(expected)) {
      const jobs = splitWorkflowJobs(readWorkflow(name));
      expect(
        jobs.map((job) => job.name),
        name,
      ).toEqual(jobNames);
      for (const job of jobs) {
        expectOwnerActorGate(job, `${name}:${job.name}`);
      }
    }
  });
});
