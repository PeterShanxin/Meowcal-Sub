import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { splitWorkflowJobs } from "../../scripts/action-cache-plan.mjs";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

const OWNER_SELF_HOSTED_CLAUSES = [
  "github.actor != 'dependabot[bot]'",
  "github.event_name == 'push'",
  "github.event.pull_request.head.repo.full_name == github.repository",
  "github.event.pull_request.user.login == 'PeterShanxin'",
  "github.actor == 'PeterShanxin'",
];

describe("self-hosted CI jobs stay off untrusted pull requests", () => {
  it("gives every test.yml job the owner-only if and a credential-free checkout", () => {
    const contents = readFileSync(path.join(repositoryRoot, ".github/workflows/test.yml"), "utf8");
    expect(contents).toMatch(/^permissions:\n {2}contents: read$/m);
    expect(contents).toMatch(/^ {2}pull_request:$/m);

    const jobs = splitWorkflowJobs(contents);

    expect(jobs.map((job) => job.name)).toEqual(["lint", "test", "frontend"]);

    for (const job of jobs) {
      const text = job.lines.join("\n");
      for (const clause of OWNER_SELF_HOSTED_CLAUSES) {
        expect(text, `${job.name} missing ${clause}`).toContain(clause);
      }
      expect(text, job.name).toContain("persist-credentials: false");
      expect(text, job.name).toContain("clean: false");
    }
  });

  it("does not treat same-repo write access as enough to run on meowcal-ci", () => {
    const contents = readFileSync(path.join(repositoryRoot, ".github/workflows/test.yml"), "utf8");
    for (const job of splitWorkflowJobs(contents)) {
      const text = job.lines.join("\n").replace(/\s+/g, " ");
      expect(text, job.name).not.toMatch(
        /if: github\.event_name == 'push' \|\| github\.event\.pull_request\.head\.repo\.full_name == github\.repository\s*$/,
      );
      expect(text, job.name).toContain("github.event.pull_request.user.login == 'PeterShanxin'");
      expect(text, job.name).toContain("github.actor == 'PeterShanxin'");
      expect(text, job.name).toMatch(
        /github\.event\.pull_request\.user\.login == 'PeterShanxin' && github\.actor == 'PeterShanxin'/,
      );
    }
  });

  it("does not add pull_request to maintainer-only packaging and release workflows", () => {
    for (const name of ["package.yml", "release.yml", "publish-update.yml"]) {
      const contents = readFileSync(path.join(repositoryRoot, ".github/workflows", name), "utf8");
      expect(contents, name).not.toMatch(/^\s*pull_request:/m);
    }
  });
});
