import { describe, expect, it } from "vitest";
import {
  findRunnerPolicyViolations,
  stripYamlComment,
} from "../../scripts/workflow-runner-policy.mjs";

const check = (yaml) => findRunnerPolicyViolations(".github/workflows/fixture.yml", yaml);

describe("runner policy", () => {
  it("accepts the self-hosted label sets the workflows actually use", () => {
    expect(check("    runs-on: [self-hosted, Windows, ARM64, meowcal-ci]")).toEqual([]);
    expect(
      check(
        "    runs-on: >-\n" +
          "      ${{ inputs.architecture == 'arm64'\n" +
          '      && fromJSON(\'["self-hosted", "Windows", "meowcal-package-arm64"]\')\n' +
          '      || fromJSON(\'["self-hosted", "Windows", "meowcal-package-x64"]\') }}',
      ),
    ).toEqual([]);
  });

  it("accepts the Linux hosted runners the release jobs deliberately keep", () => {
    expect(check("    runs-on: ubuntu-latest")).toEqual([]);
    expect(check("    runs-on: ubuntu-24.04")).toEqual([]);
  });

  it("accepts the Stage 2 hosted Windows PR gates", () => {
    expect(check("    runs-on: windows-11-arm")).toEqual([]);
    expect(check("    runs-on: windows-2025")).toEqual([]);
  });

  it("rejects windows-latest, other Windows images, and macOS", () => {
    expect(check("    runs-on: windows-latest").length).toBeGreaterThan(0);
    expect(check("    runs-on: windows-2022").length).toBeGreaterThan(0);
    expect(check("    runs-on: macos-14").length).toBeGreaterThan(0);
  });

  it("rejects mixing a hosted Windows image into a self-hosted job", () => {
    expect(
      check("    runs-on: [self-hosted, Windows, ARM64, meowcal-ci, windows-11-arm]").length,
    ).toBeGreaterThan(0);
    expect(
      check("    runs-on: [self-hosted, Windows, meowcal-package-x64, windows-2025]").length,
    ).toBeGreaterThan(0);
  });

  it("rejects the bare self-hosted label", () => {
    expect(check("    runs-on: [self-hosted, Windows]")).toEqual([
      expect.stringContaining("bare self-hosted"),
    ]);
  });

  // A comment must never vouch for the code beside it. Both of these passed an
  // earlier version of this check, which matched labels against raw line text.
  it("does not let an inline comment satisfy the meowcal-* requirement", () => {
    expect(check("    runs-on: self-hosted  # meowcal-ci")).toEqual([
      expect.stringContaining("bare self-hosted"),
    ]);
  });

  it("rejects an indirect runner value even when a comment names valid labels", () => {
    const violations = check(
      "    # self-hosted meowcal-ci emergency override, see runbook\n" +
        "    runs-on: ${{ vars.EMERGENCY_RUNNER_LABEL }} # self-hosted meowcal-ci fallback",
    );
    expect(violations).toEqual([expect.stringContaining("neither an allowed")]);
  });

  it("rejects a hosted runner hidden among the lines of a block value", () => {
    expect(
      check("    runs-on:\n      - self-hosted\n      - meowcal-ci\n      - windows-latest").length,
    ).toBeGreaterThan(0);
  });

  it("does not flag a comment that merely names a hosted runner", () => {
    // The workflows explain the policy in prose, and prose is not a use.
    expect(
      check("    # Never windows-latest here.\n    runs-on: [self-hosted, meowcal-ci]"),
    ).toEqual([]);
  });
});

describe("stripYamlComment", () => {
  it("removes an end-of-line comment", () => {
    expect(stripYamlComment("runs-on: self-hosted # meowcal-ci")).toBe("runs-on: self-hosted");
  });

  it("keeps a hash inside a quoted scalar", () => {
    expect(stripYamlComment(`name: "a # b"`)).toBe(`name: "a # b"`);
    expect(stripYamlComment("name: 'a # b'")).toBe("name: 'a # b'");
  });

  it("keeps a hash that does not follow whitespace", () => {
    expect(stripYamlComment("url: https://example.test/x#fragment")).toBe(
      "url: https://example.test/x#fragment",
    );
  });

  it("removes a whole-line comment", () => {
    expect(stripYamlComment("# just a note")).toBe("");
  });
});
