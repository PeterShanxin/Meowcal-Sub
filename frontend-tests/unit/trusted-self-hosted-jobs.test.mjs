import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { isSelfHostedRunsOn, splitWorkflowJobs } from "../../scripts/action-cache-plan.mjs";
import { foldRunsOnValue } from "../../scripts/workflow-runner-policy.mjs";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

const TRUSTED_ACTOR_IF = "(github.actor == 'PeterShanxin' || github.actor == 'ianmeowmeow')";
const TRUSTED_PR_AUTHOR_IF =
  "(github.event.pull_request.user.login == 'PeterShanxin' || github.event.pull_request.user.login == 'ianmeowmeow')";
const NOT_DEPENDABOT = "github.actor != 'dependabot[bot]'";
const SAME_REPO = "github.event.pull_request.head.repo.full_name == github.repository";

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

function isReusableCaller(job) {
  return job.lines.some((line) => /^\s*uses:\s*\.\//.test(line));
}

function expectTrustedActorIf(job, label) {
  const text = jobText(job).replace(/\s+/g, " ");
  expect(text, `${label} missing trusted-actor if`).toContain(TRUSTED_ACTOR_IF);
  expect(text, `${label} missing Dependabot exclusion`).toContain(NOT_DEPENDABOT);
}

function expectFailClosedStep(job, label) {
  const text = jobText(job);
  expect(text, `${label} missing fail-closed step`).toContain("Require a trusted actor");
  expect(text, `${label} fail-closed missing PeterShanxin`).toContain("PeterShanxin");
  expect(text, `${label} fail-closed missing ianmeowmeow`).toContain("ianmeowmeow");
  expect(text, `${label} fail-closed missing actor refusal`).toContain(
    "Privileged job refused actor",
  );
}

describe("self-hosted CI jobs stay off untrusted pull requests", () => {
  it("gives every test.yml job the trusted-actor if, fork check, and a credential-free checkout", () => {
    const contents = readWorkflow("test.yml");
    expect(contents).toMatch(/^permissions:\n {2}contents: read$/m);
    expect(contents).toMatch(/^ {2}pull_request:$/m);

    const jobs = splitWorkflowJobs(contents);

    expect(jobs.map((job) => job.name)).toEqual(["lint", "test", "frontend"]);

    for (const job of jobs) {
      const text = jobText(job).replace(/\s+/g, " ");
      expectTrustedActorIf(job, job.name);
      expectFailClosedStep(job, job.name);
      expect(text, job.name).toContain("github.event_name == 'push'");
      expect(text, job.name).toContain(SAME_REPO);
      expect(text, job.name).toContain(TRUSTED_PR_AUTHOR_IF);
      expect(text, job.name).toContain("Privileged job refused fork head");
      expect(jobText(job), job.name).toContain("persist-credentials: false");
      expect(jobText(job), job.name).toContain("clean: false");
    }
  });

  it("requires the trusted actor on push, not only on pull_request", () => {
    const contents = readWorkflow("test.yml");
    for (const job of splitWorkflowJobs(contents)) {
      const text = jobText(job).replace(/\s+/g, " ");
      expect(text, job.name).toMatch(
        /\(github\.actor == 'PeterShanxin' \|\| github\.actor == 'ianmeowmeow'\) && github\.actor != 'dependabot\[bot\]' && \(github\.event_name == 'push'/,
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

describe("host trust is PeterShanxin and ianmeowmeow only", () => {
  it("gates every self-hosted job on both trusted actors", () => {
    const selfHosted = [];
    for (const name of ["test.yml", "package.yml", "release.yml", "publish-update.yml"]) {
      for (const job of splitWorkflowJobs(readWorkflow(name))) {
        const runsOn = jobRunsOn(job);
        if (isSelfHostedRunsOn(runsOn)) {
          selfHosted.push(`${name}:${job.name}`);
          expectTrustedActorIf(job, `${name}:${job.name}`);
          expectFailClosedStep(job, `${name}:${job.name}`);
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

  it("gates packaging, release, and publish-update jobs on both trusted actors", () => {
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
        expectTrustedActorIf(job, `${name}:${job.name}`);
        if (!isReusableCaller(job)) {
          expectFailClosedStep(job, `${name}:${job.name}`);
        }
      }
    }
  });
});
