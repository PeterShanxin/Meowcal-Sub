import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { isSelfHostedRunsOn, splitWorkflowJobs } from "../../scripts/action-cache-plan.mjs";
import { foldRunsOnValue } from "../../scripts/workflow-runner-policy.mjs";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

const TRUSTED_ACTOR_IF = "(github.actor == 'PeterShanxin' || github.actor == 'ianmeowmeow')";
const NOT_DEPENDABOT = "github.actor != 'dependabot[bot]'";
const SIGNING_OR_MIRROR_SECRET =
  /\$\{\{\s*secrets\.(TAURI_SIGNING_PRIVATE_KEY|TAURI_SIGNING_PRIVATE_KEY_PASSWORD|RELEASE_MIRROR_TOKEN)/;

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

describe("public PR CI runs on hosted Windows, not meowcal-ci", () => {
  it("keeps contents: read and a pull_request trigger on test.yml", () => {
    const contents = readWorkflow("test.yml");
    expect(contents).toMatch(/^permissions:\n {2}contents: read$/m);
    expect(contents).toMatch(/^ {2}pull_request:$/m);
    expect(contents).toMatch(/^ {2}workflow_dispatch:$/m);
    expect(contents).not.toMatch(SIGNING_OR_MIRROR_SECRET);
  });

  it("gives hosted PR jobs persist-credentials: false and no host trust gate", () => {
    const hosted = splitWorkflowJobs(readWorkflow("test.yml")).filter(
      (job) => !isSelfHostedRunsOn(jobRunsOn(job)),
    );

    expect(hosted.map((job) => job.name).sort()).toEqual([
      "frontend",
      "lint",
      "test",
      "verify-x64",
    ]);

    for (const job of hosted) {
      const runsOn = jobRunsOn(job);
      const text = jobText(job);
      expect(text, job.name).toContain("persist-credentials: false");
      expect(text, job.name).not.toContain("clean: false");
      expect(text, job.name).not.toContain("Require a trusted actor");
      expect(text.replace(/\s+/g, " "), job.name).toContain(
        "github.event_name == 'pull_request' || github.event_name == 'push'",
      );
      expect(["windows-11-arm", "windows-2025"], job.name).toContain(runsOn);
    }

    expect(
      hosted.filter((job) => jobRunsOn(job) === "windows-11-arm").map((job) => job.name),
    ).toEqual(["lint", "test", "frontend"]);
    expect(
      hosted.filter((job) => jobRunsOn(job) === "windows-2025").map((job) => job.name),
    ).toEqual(["verify-x64"]);
  });

  it("checks out Change Contract without persisted credentials", () => {
    expect(readWorkflow("change-contract.yml")).toContain("persist-credentials: false");
    expect(readWorkflow("change-contract.yml")).toContain("runs-on: ubuntu-24.04");
  });

  it("installs a toolchain on hosted jobs instead of assuming a host image", () => {
    for (const job of splitWorkflowJobs(readWorkflow("test.yml"))) {
      if (isSelfHostedRunsOn(jobRunsOn(job))) {
        continue;
      }
      const text = jobText(job);
      expect(text, job.name).toContain("actions/setup-node@v4");
      expect(text, job.name).toContain("dtolnay/rust-toolchain@stable");
      expect(text, job.name).toContain("node-version-file: .node-version");
    }
  });

  it("does not add pull_request to maintainer-only packaging and release workflows", () => {
    for (const name of ["package.yml", "release.yml", "publish-update.yml"]) {
      const contents = readWorkflow(name);
      expect(contents, name).not.toMatch(/^\s*pull_request:/m);
    }
  });
});

describe("self-hosted hardware jobs stay off pull requests", () => {
  it("limits meowcal-ci jobs to trusted-admin workflow_dispatch", () => {
    const hardware = splitWorkflowJobs(readWorkflow("test.yml")).filter((job) =>
      isSelfHostedRunsOn(jobRunsOn(job)),
    );

    expect(hardware.map((job) => job.name)).toEqual([
      "hardware-lint",
      "hardware-test",
      "hardware-frontend",
    ]);

    for (const job of hardware) {
      const text = jobText(job).replace(/\s+/g, " ");
      expectTrustedActorIf(job, job.name);
      expectFailClosedStep(job, job.name);
      expect(text, job.name).toContain("github.event_name == 'workflow_dispatch'");
      expect(text, job.name).not.toContain("github.event_name == 'push'");
      expect(text, job.name).not.toContain("github.event_name == 'pull_request'");
      expect(jobText(job), job.name).toContain("persist-credentials: false");
      expect(jobText(job), job.name).toContain("clean: false");
      expect(jobText(job), job.name).toContain("Privileged job refused fork head");
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
      "test.yml:hardware-frontend",
      "test.yml:hardware-lint",
      "test.yml:hardware-test",
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
