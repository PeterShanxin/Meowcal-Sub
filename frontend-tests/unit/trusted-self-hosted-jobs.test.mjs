import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { splitWorkflowJobs } from "../../scripts/action-cache-plan.mjs";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

const TRUSTED_SELF_HOSTED_IF =
  "github.event_name == 'push' || github.event.pull_request.head.repo.full_name == github.repository";

describe("self-hosted CI jobs stay off untrusted pull requests", () => {
  it("gives every test.yml job the trusted-repo if and a credential-free checkout", () => {
    const contents = readFileSync(path.join(repositoryRoot, ".github/workflows/test.yml"), "utf8");
    expect(contents).toMatch(/^permissions:\n {2}contents: read$/m);

    const jobs = splitWorkflowJobs(contents);

    expect(jobs.map((job) => job.name)).toEqual(["lint", "test", "frontend"]);

    for (const job of jobs) {
      const text = job.lines.join("\n");
      expect(text, job.name).toContain(`if: ${TRUSTED_SELF_HOSTED_IF}`);
      expect(text, job.name).toContain("persist-credentials: false");
      expect(text, job.name).toContain("clean: false");
    }
  });

  it("does not add pull_request to maintainer-only packaging and release workflows", () => {
    for (const name of ["package.yml", "release.yml", "publish-update.yml"]) {
      const contents = readFileSync(path.join(repositoryRoot, ".github/workflows", name), "utf8");
      expect(contents, name).not.toMatch(/^\s*pull_request:/m);
    }
  });
});
