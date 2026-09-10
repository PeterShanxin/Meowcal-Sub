import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { foldRunsOnValue, splitWorkflowJobs } from "../../scripts/workflow-runner-policy.mjs";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const workflowDirectory = path.join(repositoryRoot, ".github/workflows");

const TRUSTED_ACTOR_IF = "(github.actor == 'PeterShanxin' || github.actor == 'ianmeowmeow')";
const NOT_DEPENDABOT = "github.actor != 'dependabot[bot]'";
const FORBIDDEN_SECRETS = [
  "TAURI_SIGNING_PRIVATE_KEY",
  "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
  "RELEASE_MIRROR_TOKEN",
];

function workflowNames() {
  return readdirSync(workflowDirectory)
    .filter((name) => /\.ya?ml$/.test(name))
    .sort();
}

function readWorkflow(name) {
  return readFileSync(path.join(workflowDirectory, name), "utf8");
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

describe("every job runs on GitHub-hosted infrastructure", () => {
  it("names no self-hosted runner anywhere in any workflow", () => {
    // The invariant the whole migration exists to hold: nothing in Actions may
    // wait on a machine somebody has to keep online.
    for (const name of workflowNames()) {
      expect(readWorkflow(name), name).not.toMatch(/self-hosted/);
    }
  });

  it("has no workflow that presents itself as real-hardware verification", () => {
    // A GitHub-hosted ARM64 VM is not a Snapdragon/Adreno device. Real device
    // validation is the manual Windows gate in CONTRIBUTING.md, not a CI job.
    // Prose that says so is welcome; a workflow *named* for hardware is not.
    expect(workflowNames()).not.toContain("hardware.yml");
    for (const name of workflowNames()) {
      const title = readWorkflow(name).match(/^name:\s*(.+)$/m)?.[1] ?? "";
      expect(title, name).not.toMatch(/hardware|device|snapdragon|adreno/i);
    }
  });
});

describe("hosted PR gate is the merge gate", () => {
  it("keeps required check names, pull_request, and contents: read on hosted Windows", () => {
    const contents = readWorkflow("test.yml");
    expect(contents).toMatch(/^permissions:\n {2}contents: read$/m);
    expect(contents).toMatch(/^ {2}pull-requests: read$/m);
    expect(contents).toMatch(/^ {2}pull_request:$/m);
    expect(contents).toMatch(/^ {2}push:$/m);

    const jobs = splitWorkflowJobs(contents);
    expect(jobs.map((job) => job.name)).toEqual([
      "scope",
      "lint_arm64",
      "lint_x64",
      "test_arm64",
      "test_x64",
      "frontend_windows",
      "lint",
      "test",
      "frontend",
    ]);
    const byName = Object.fromEntries(jobs.map((job) => [job.name, job]));

    const displayNames = jobs.map((job) => {
      const match = jobText(job).match(/^\s*name:\s*(.+)$/m);
      return match ? match[1].trim() : job.name;
    });
    expect(displayNames).toEqual([
      "Classify verification scope",
      "Lint & Format (ARM64)",
      "Lint & Format (x64)",
      "Tests (ARM64)",
      "Tests (x64)",
      "Frontend & Browser (Windows)",
      "Lint & Format",
      "Tests",
      "Frontend & Browser",
    ]);

    for (const name of ["lint_arm64", "lint_x64", "test_arm64", "test_x64", "frontend_windows"]) {
      const job = byName[name];
      expect(jobText(job), name).toContain("persist-credentials: false");
      expect(jobText(job), name).toContain("./scripts/verify.ps1");
      expect(jobText(job), name).not.toContain(TRUSTED_ACTOR_IF);
    }

    expect(jobRunsOn(byName.scope)).toBe("ubuntu-24.04");
    for (const name of ["lint", "test", "frontend"]) {
      const job = byName[name];
      expect(jobRunsOn(job), name).toBe("ubuntu-24.04");
      expect(jobText(job), name).toContain("if: ${{ always() }}");
      expect(jobText(job), name).toContain("Changed-file classification failed.");
    }
  });

  it("verifies each shipped architecture on its own hosted image", () => {
    // The gate covers both architectures because the crate compiles different
    // code for each. Emulating one of them on the other host would still pass
    // this check, so the runner image is asserted alongside the target.
    const byName = Object.fromEntries(
      splitWorkflowJobs(readWorkflow("test.yml")).map((job) => [job.name, job]),
    );
    const expected = {
      lint_arm64: ["windows-11-arm", "aarch64-pc-windows-msvc"],
      test_arm64: ["windows-11-arm", "aarch64-pc-windows-msvc"],
      lint_x64: ["windows-2025", "x86_64-pc-windows-msvc"],
      test_x64: ["windows-2025", "x86_64-pc-windows-msvc"],
      frontend_windows: ["windows-11-arm", "aarch64-pc-windows-msvc"],
    };
    for (const [name, [runner, target]] of Object.entries(expected)) {
      expect(jobRunsOn(byName[name]), name).toBe(runner);
      expect(jobText(byName[name]), name).toContain(target);
    }
  });

  it("gives every required wrapper the result of every job it stands for", () => {
    // A wrapper that stops naming one of its Windows jobs reports green while
    // that architecture is unverified, and the required check hides it.
    const byName = Object.fromEntries(
      splitWorkflowJobs(readWorkflow("test.yml")).map((job) => [job.name, job]),
    );
    const covered = {
      lint: ["lint_arm64", "lint_x64"],
      test: ["test_arm64", "test_x64"],
      frontend: ["frontend_windows"],
    };
    for (const [wrapper, heavy] of Object.entries(covered)) {
      const text = jobText(byName[wrapper]);
      expect(text, wrapper).toContain(`needs: [scope, ${heavy.join(", ")}]`);
      for (const name of heavy) {
        expect(text, `${wrapper} must resolve ${name}`).toContain(`needs.${name}.result`);
      }
    }
  });

  it("never interpolates signing or legacy-bridge secrets on the hosted gate", () => {
    expectNoForbiddenSecrets(readWorkflow("test.yml"), "test.yml");
    expectNoForbiddenSecrets(readWorkflow("change-contract.yml"), "change-contract.yml");
  });

  it("keeps Change Contract on hosted Ubuntu", () => {
    const jobs = splitWorkflowJobs(readWorkflow("change-contract.yml"));
    expect(jobs).toHaveLength(1);
    expect(jobRunsOn(jobs[0])).toBe("ubuntu-24.04");
  });
});

describe("packaging builds both architectures natively", () => {
  const contents = readWorkflow("package.yml");
  const job = splitWorkflowJobs(contents)[0];

  it("selects a native hosted image per architecture", () => {
    const runsOn = jobRunsOn(job);
    expect(runsOn).toContain("inputs.architecture == 'arm64'");
    expect(runsOn).toContain("windows-11-arm");
    expect(runsOn).toContain("windows-2025");
  });

  it("offers exactly x64 and arm64, and refuses anything else fail-closed", () => {
    expect(contents).toMatch(/options:\n {10}- x64\n {10}- arm64/);
    expect(contents).toContain("Unsupported package architecture");
    expect(contents).toContain("x86_64-pc-windows-msvc");
    expect(contents).toContain("aarch64-pc-windows-msvc");
  });

  it("refuses to package on a runner of the wrong architecture", () => {
    // An unrecognized input falls through to the x64 image. That must not be
    // able to upload a bundle built for the other architecture.
    expect(contents).toContain("RuntimeInformation]::OSArchitecture");
    expect(contents).toContain("expects a $expectedOsArchitecture runner");
  });

  it("installs its own toolchain rather than inheriting one from the image", () => {
    expect(contents).toContain("actions/setup-node@");
    expect(contents).toContain("actions/setup-dotnet@");
    expect(contents).toContain("rustup toolchain install stable");
    expect(contents).toContain("rustup target add");
    expect(contents).toContain("npm ci --ignore-scripts");
  });

  it("fails before the build when the signing key did not reach the job", () => {
    expect(contents).toContain("TAURI_SIGNING_PRIVATE_KEY is empty");
    expect(contents).toContain("if-no-files-found: error");
  });
});

describe("release and preflight share one asset contract", () => {
  it("both run the same asset verification script", () => {
    for (const name of ["release.yml", "release-preflight.yml"]) {
      expect(readWorkflow(name), name).toContain("./scripts/verify-release-assets.ps1");
      expect(readWorkflow(name), name).toContain("scripts/build-updater-manifest.mjs");
    }
  });

  it("requires one MSI, one NSIS setup, and one signature per architecture", () => {
    const script = readFileSync(
      path.join(repositoryRoot, "scripts/verify-release-assets.ps1"),
      "utf8",
    );
    expect(script).toContain('@("x64", "arm64")');
    expect(script).toContain('Pattern = "*.msi"');
    expect(script).toContain('Pattern = "*-setup.exe"');
    expect(script).toContain('Pattern = "*-setup.exe.sig"');
    expect(script).toContain("found $($found.Count)");
  });

  it("keeps checksum and manifest generation in the release path", () => {
    const contents = readWorkflow("release.yml");
    expect(contents).toContain("release-assets/SHA256SUMS.txt");
    expect(contents).toContain("release-assets/latest.json");
    expect(contents).toContain("--verify-tag");
    expect(contents).toContain("--draft");
  });

  it("never lets the preflight reserve a tag or create a release", () => {
    const contents = readWorkflow("release-preflight.yml");
    expect(contents).not.toMatch(/gh release create/);
    expect(contents).not.toMatch(/git tag/);
    expect(contents).not.toMatch(/git push/);
    expect(contents).not.toMatch(/contents: write/);
  });
});

describe("host trust is PeterShanxin and ianmeowmeow only", () => {
  it("gates every job that can reach a release credential on both trusted actors", () => {
    const expected = {
      "package.yml": ["package"],
      "release.yml": ["validate", "package-x64", "package-arm64", "draft-release"],
      "release-preflight.yml": ["package-x64", "package-arm64", "assets"],
      "publish-legacy-update-bridge.yml": ["publish"],
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

  it("does not add pull_request to packaging or release workflows", () => {
    for (const name of [
      "package.yml",
      "release.yml",
      "release-preflight.yml",
      "publish-legacy-update-bridge.yml",
    ]) {
      const contents = readWorkflow(name);
      expect(contents, name).not.toMatch(/^\s*pull_request:/m);
      expect(contents, name).toMatch(/^permissions:\n {2}contents: read$/m);
    }
  });

  it("keeps the legacy bridge manifest-only and pointed at canonical assets", () => {
    const contents = readWorkflow("publish-legacy-update-bridge.yml");
    expect(contents).toContain("--pattern latest.json");
    expect(contents).toContain("bridge-assets/latest.json");
    expect(contents).toContain("https://github.com/$env:GITHUB_REPOSITORY/releases/download/");
    expect(contents).not.toContain("export-showcase");
  });
});
