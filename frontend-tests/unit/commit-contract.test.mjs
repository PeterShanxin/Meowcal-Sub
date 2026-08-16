import { describe, expect, it } from "vitest";
import {
  COMMIT_TYPES,
  MAX_HEADER_LENGTH,
  findCommitViolations,
  findHeaderViolation,
  findPullRequestTitleViolations,
} from "../../scripts/commit-contract.mjs";

function commit(overrides = {}) {
  return {
    sha: "0123456789abcdef",
    parents: ["fedcba9876543210"],
    author: "PeterShanxin",
    email: "shanxin@example.invalid",
    subject: "fix: keep the overlay visible after a stop",
    body: "",
    ...overrides,
  };
}

describe("commit header grammar", () => {
  it("accepts every allowed type", () => {
    for (const type of COMMIT_TYPES) {
      expect(findHeaderViolation(`${type}: do the thing`)).toBeNull();
    }
  });

  it("accepts an optional scope and an issue reference", () => {
    expect(findHeaderViolation("refactor(overlay): give the timers an owner (#34)")).toBeNull();
  });

  it("rejects an unknown type", () => {
    expect(findHeaderViolation("wip: half a change")).toMatch(/not an allowed type/);
  });

  it("rejects a missing type", () => {
    expect(findHeaderViolation("keep the overlay visible")).toMatch(/does not match/);
  });

  it("rejects a missing space after the colon", () => {
    expect(findHeaderViolation("fix:keep the overlay visible")).toMatch(/does not match/);
  });

  it("rejects an uppercase or punctuated scope", () => {
    expect(findHeaderViolation("fix(Overlay): keep it visible")).toMatch(/must be lowercase/);
  });

  it("rejects a trailing period", () => {
    expect(findHeaderViolation("fix: keep the overlay visible.")).toMatch(/ends with a period/);
  });

  it("rejects an over-long subject", () => {
    const header = `fix: ${"x".repeat(MAX_HEADER_LENGTH)}`;
    expect(findHeaderViolation(header)).toMatch(/limit is 100/);
  });

  it("rejects an empty subject", () => {
    expect(findHeaderViolation("fix:")).toMatch(/does not match/);
  });

  it("rejects surrounding whitespace", () => {
    expect(findHeaderViolation("fix: keep the overlay visible ")).toMatch(/whitespace/);
  });

  it("requires a footer for a breaking change", () => {
    expect(findHeaderViolation("feat!: drop the legacy endpoint")).toMatch(/BREAKING CHANGE/);
    expect(
      findHeaderViolation("feat!: drop the legacy endpoint", {
        body: "BREAKING CHANGE: the developer-mode endpoint is gone.",
      }),
    ).toBeNull();
  });
});

describe("commits a pull request adds", () => {
  it("passes a conforming branch", () => {
    expect(
      findCommitViolations([commit(), commit({ subject: "docs: record the contract" })]),
    ).toEqual([]);
  });

  it("names the offending commit", () => {
    const violations = findCommitViolations([commit({ subject: "wip" })]);
    expect(violations).toHaveLength(1);
    expect(violations[0]).toContain("01234567");
    expect(violations[0]).toContain("wip");
  });

  it("exempts merge commits", () => {
    const merge = commit({
      parents: ["aaaa", "bbbb"],
      subject: "Merge pull request #120 from PeterShanxin/refactor/34-overlay",
    });
    expect(findCommitViolations([merge])).toEqual([]);
  });

  it("exempts a generated revert subject", () => {
    expect(
      findCommitViolations([commit({ subject: 'Revert "fix: keep the overlay visible"' })]),
    ).toEqual([]);
  });

  it("exempts dependency-bot commits", () => {
    const bot = commit({
      author: "dependabot[bot]",
      email: "49699333+dependabot[bot]@users.noreply.github.com",
      subject: "Bump vite from 8.2.0 to 8.2.1",
    });
    expect(findCommitViolations([bot])).toEqual([]);
  });
});

describe("pull request titles", () => {
  it("accepts a conforming title", () => {
    expect(findPullRequestTitleViolations("chore: define the change contract (#37)")).toEqual([]);
  });

  it("rejects a non-conforming title", () => {
    const violations = findPullRequestTitleViolations("Update stuff");
    expect(violations).toHaveLength(1);
    expect(violations[0]).toContain("Update stuff");
  });

  it("exempts a bot-authored pull request", () => {
    expect(findPullRequestTitleViolations("Bump vite", { author: "dependabot[bot]" })).toEqual([]);
  });
});
