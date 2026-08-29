import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { isSelfHostedRunsOn, splitWorkflowJobs } from "../../scripts/action-cache-plan.mjs";
import { foldRunsOnValue } from "../../scripts/workflow-runner-policy.mjs";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

const TRUSTED_ACTOR_IF = "(github.actor == 'PeterShanxin' || github.actor == 'ianmeowmeow')";
const NOT_DEPENDABOT = "github.actor != 'dependabot[bot]'";
const FORBIDDEN_SECRETS = [
  "TAURI_SIGNING_PRIVATE_KEY",
  "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
  "RELEASE_MIRROR_TOKEN",
];

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

function expectNoForbiddenSecrets(contents, label) {
  for (const name of FORBIDDEN_SECRETS) {
    expect(contents, `${label} must not interpolate ${name}`).not.toMatch(
      new RegExp(`secrets\\.${name}`),
    );
  }
}

describe("hosted PR gate is the merge gate", () => {
  it("keeps required check names, pull_request, and contents: read on windows-11-arm", () => {
    const contents = readWorkflow("test.yml");
    expect(contents).toMatch(/^permissions:\n {2}contents: read$/m);
    expect(contents).toMatch(/^ {2}pull_request:$/m);
    expect(contents).toMatch(/^ {2}push:$/m);

    const jobs = splitWorkflowJobs(contents);
    expect(jobs.map((job) => job.name)).toEqual(["lint", "test", "frontend"]);

    const displayNames = jobs.map((job) => {
      const match = jobText(job).match(/^\s*name:\s*(.+)$/m);
      return match ? match[1].trim() : job.name;
    });
    expect(displayNames).toEqual(["Lint & Format", "Tests", "Frontend & Browser"]);

    for (const job of jobs) {
      expect(jobRunsOn(job), job.name).toBe("windows-11-arm");
      expect(jobText(job), job.name).toContain("persist-credentials: false");
      expect(jobText(job), job.name).toContain("./scripts/verify.ps1");
      expect(jobText(job), job.name).not.toContain("self-hosted");
      expect(jobText(job), job.name).not.toContain(TRUSTED_ACTOR_IF);
    }
  });

  it("never interpolates signing or mirror secrets on the hosted gate", () => {
    expectNoForbiddenSecrets(readWorkflow("test.yml"), "test.yml");
  });

  it("keeps Change Contract on hosted Ubuntu", () => {
    const jobs = splitWorkflowJobs(readWorkflow("change-contract.yml"));
    expect(jobs).toHaveLength(1);
    expect(jobRunsOn(jobs[0])).toBe("ubuntu-24.04");
    expect(isSelfHostedRunsOn(jobRunsOn(jobs[0]))).toBe(false);
  });

  it("documents hosted Windows as the explicit Stage 2 PR gate", () => {
    const runnerDoc = readFileSync(
      path.join(repositoryRoot, "docs/SELF_HOSTED_RUNNERS.md"),
      "utf8",
    );
    const contributing = readFileSync(path.join(repositoryRoot, "CONTRIBUTING.md"), "utf8");
    expect(runnerDoc).toMatch(/windows-11-arm/);
    expect(runnerDoc).toMatch(/Stage 2/);
    expect(runnerDoc).toMatch(/merge gate/);
    expect(runnerDoc).not.toMatch(/No workflow names a GitHub-hosted/);
    expect(contributing).toMatch(/windows-11-arm/);
    expect(contributing).not.toMatch(/Windows CI for them is not provided/);
  });
});

describe("self-hosted hardware CI is maintainer dispatch only", () => {
  it("does not trigger hardware.yml from pull_request", () => {
    const contents = readWorkflow("hardware.yml");
    expect(contents).toMatch(/^permissions:\n {2}contents: read$/m);
    expect(contents).toMatch(/^ {2}workflow_dispatch:\s*$/m);
    expect(contents).not.toMatch(/^\s*pull_request:/m);
    expect(contents).not.toMatch(/^\s*push:/m);
    expectNoForbiddenSecrets(contents, "hardware.yml");
  });

  it("gives every hardware.yml job the trusted-actor if and a credential-free checkout", () => {
    const jobs = splitWorkflowJobs(readWorkflow("hardware.yml"));
    expect(jobs.map((job) => job.name)).toEqual(["lint", "test", "frontend"]);

    for (const job of jobs) {
      expectTrustedActorIf(job, job.name);
      expectFailClosedStep(job, job.name);
      expect(jobText(job), job.name).toContain("Privileged job refused fork head");
      expect(jobText(job), job.name).toContain("persist-credentials: false");
      expect(jobText(job), job.name).toContain("clean: false");
      expect(isSelfHostedRunsOn(jobRunsOn(job)), job.name).toBe(true);
    }
  });

  it("does not add pull_request to maintainer-only packaging and release workflows", () => {
    for (const name of ["hardware.yml", "package.yml", "release.yml", "publish-update.yml"]) {
      const contents = readWorkflow(name);
      expect(contents, name).not.toMatch(/^\s*pull_request:/m);
    }
  });
});

describe("host trust is PeterShanxin and ianmeowmeow only", () => {
  it("gates every self-hosted job on both trusted actors", () => {
    const selfHosted = [];
    for (const name of [
      "test.yml",
      "hardware.yml",
      "package.yml",
      "release.yml",
      "publish-update.yml",
    ]) {
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
      "hardware.yml:frontend",
      "hardware.yml:lint",
      "hardware.yml:test",
      "package.yml:package",
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
