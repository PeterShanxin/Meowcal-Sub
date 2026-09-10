import { describe, expect, it } from "vitest";
import {
  findRunnerPolicyViolations,
  runnerSelection,
  splitWorkflowJobs,
  stripYamlComment,
} from "../../scripts/workflow-runner-policy.mjs";

const check = (yaml) => findRunnerPolicyViolations(".github/workflows/fixture.yml", yaml);

describe("runner policy", () => {
  it("accepts the hosted runners the workflows actually use", () => {
    expect(check("    runs-on: ubuntu-latest")).toEqual([]);
    expect(check("    runs-on: ubuntu-24.04")).toEqual([]);
    expect(check("    runs-on: windows-11-arm")).toEqual([]);
    expect(check("    runs-on: windows-2025")).toEqual([]);
  });

  it("accepts the packaging expression that picks an image per architecture", () => {
    expect(
      check(
        "    runs-on: >-\n" +
          "      ${{ inputs.architecture == 'arm64'\n" +
          "      && 'windows-11-arm'\n" +
          "      || 'windows-2025' }}",
      ),
    ).toEqual([]);
  });

  it("rejects every self-hosted form", () => {
    expect(check("    runs-on: self-hosted").length).toBeGreaterThan(0);
    expect(check("    runs-on: [self-hosted, Windows, ARM64, meowcal-ci]").length).toBeGreaterThan(
      0,
    );
    expect(
      check(
        "    runs-on: >-\n" +
          "      ${{ inputs.architecture == 'arm64'\n" +
          '      && fromJSON(\'["self-hosted", "meowcal-package-arm64"]\')\n' +
          "      || 'windows-2025' }}",
      ).length,
    ).toBeGreaterThan(0);
  });

  it("rejects hosted images the packaging contract was never proven on", () => {
    expect(check("    runs-on: windows-latest").length).toBeGreaterThan(0);
    expect(check("    runs-on: windows-2022").length).toBeGreaterThan(0);
    expect(check("    runs-on: macos-14").length).toBeGreaterThan(0);
  });

  // A comment must never vouch for the code beside it. Both of these passed an
  // earlier version of this check, which matched labels against raw line text.
  it("does not let an inline comment satisfy the runner requirement", () => {
    expect(check("    runs-on: self-hosted  # windows-2025").length).toBeGreaterThan(0);
  });

  // One allowed literal must not vouch for an indirect operand beside it. The
  // forbidden name never appears in the workflow text in these - it lives in
  // the variable or the matrix - so the line scan cannot catch them either.
  it("rejects an expression whose other branch is indirect", () => {
    expect(
      check(
        "    runs-on: ${{ github.event_name == 'push' && 'ubuntu-latest' || vars.PACKAGE_RUNNER }}",
      ),
    ).toEqual([expect.stringContaining("does not name a runner")]);
    expect(
      check("    runs-on: ${{ inputs.architecture == 'arm64' && 'windows-11-arm' || matrix.os }}"),
    ).toEqual([expect.stringContaining("does not name a runner")]);
  });

  // GitHub substitutes the expression into the surrounding scalar, so reading
  // only the expression body reports an allowed label for a job that asks for
  // one nobody approved.
  it("rejects an expression embedded in a larger value", () => {
    expect(check("    runs-on: prefix-${{ 'windows-2025' }}")).toEqual([
      expect.stringContaining("does not name a runner"),
    ]);
    expect(check("    runs-on: ${{ 'windows-2025' }}-extra")).toEqual([
      expect.stringContaining("does not name a runner"),
    ]);
  });

  // A runner must carry every label in a list, and each hosted image carries
  // one, so this job would queue forever - the exact failure the hosted-only
  // policy exists to prevent.
  it("rejects a list of several hosted labels, however it is written", () => {
    expect(check("    runs-on: [windows-2025, ubuntu-latest]")).toEqual([
      expect.stringContaining("lists 2 labels"),
    ]);
    expect(check("    runs-on:\n      - windows-2025\n      - ubuntu-latest")).toEqual([
      expect.stringContaining("lists 2 labels"),
    ]);
    expect(check("    runs-on: [windows-2025]")).toEqual([]);
  });

  it("rejects an indirect runner value even when a comment names valid labels", () => {
    const violations = check(
      "    # windows-2025 emergency override, see runbook\n" +
        "    runs-on: ${{ vars.EMERGENCY_RUNNER_LABEL }} # windows-2025 fallback",
    );
    expect(violations).toEqual([expect.stringContaining("does not name a runner")]);
  });

  it("rejects a forbidden runner hidden among the lines of a block value", () => {
    expect(
      check("    runs-on:\n      - windows-2025\n      - windows-latest").length,
    ).toBeGreaterThan(0);
  });

  it("rejects a forbidden runner reached through a matrix entry", () => {
    // `runs-on: ${{ matrix.os }}` is already indirect; this is the value it
    // would have resolved to.
    expect(check("    strategy:\n      matrix:\n        os: [windows-latest]").length).toBe(1);
  });

  it("does not flag a comment that merely names a runner", () => {
    // The workflows explain the policy in prose, and prose is not a use.
    expect(check("    # Never windows-latest here.\n    runs-on: windows-2025")).toEqual([]);
  });
});

describe("runnerSelection", () => {
  it("takes the value position of each alternative, not every literal", () => {
    // The `'arm64'` is a condition operand, so it is not a candidate.
    expect(
      runnerSelection(
        "${{ inputs.architecture == 'arm64' && 'windows-11-arm' || 'windows-2025' }}",
      ),
    ).toEqual({ kind: "alternatives", labels: ["windows-11-arm", "windows-2025"] });
  });

  it("reports an expression whose value position is not a literal as indirect", () => {
    expect(runnerSelection("${{ vars.RUNNER }}")).toBeNull();
    expect(
      runnerSelection("${{ github.event_name == 'push' && 'ubuntu-latest' || vars.RUNNER }}"),
    ).toBeNull();
    expect(
      runnerSelection(
        "${{ inputs.a == 'arm64' && fromJSON('[\"self-hosted\"]') || 'windows-2025' }}",
      ),
    ).toBeNull();
  });

  it("requires the expression to be the whole value", () => {
    // `prefix-${{ 'windows-2025' }}` resolves to `prefix-windows-2025`, which
    // is not the label the expression names.
    expect(runnerSelection("prefix-${{ 'windows-2025' }}")).toBeNull();
    expect(runnerSelection("${{ 'windows-2025' }}-extra")).toBeNull();
    expect(runnerSelection("${{ 'ubuntu-latest' }}")).toEqual({
      kind: "alternatives",
      labels: ["ubuntu-latest"],
    });
  });

  it("reports a label list as a conjunction, not as alternatives", () => {
    expect(runnerSelection("[self-hosted, Windows, meowcal-ci]")).toEqual({
      kind: "labels",
      labels: ["self-hosted", "Windows", "meowcal-ci"],
    });
    expect(runnerSelection("windows-2025")).toEqual({ kind: "labels", labels: ["windows-2025"] });
  });
});

describe("splitWorkflowJobs", () => {
  it("returns each job with the lines beneath it", () => {
    const jobs = splitWorkflowJobs(
      [
        "jobs:",
        "  build:",
        "    runs-on: ubuntu-latest",
        "  ship:",
        "    runs-on: ubuntu-24.04",
      ].join("\n"),
    );
    expect(jobs.map((job) => job.name)).toEqual(["build", "ship"]);
    expect(jobs[0].lines).toEqual(["    runs-on: ubuntu-latest"]);
  });

  it("returns nothing for a workflow with no jobs block", () => {
    expect(splitWorkflowJobs("name: nothing\non:\n  workflow_dispatch:\n")).toEqual([]);
  });
});

describe("stripYamlComment", () => {
  it("removes an end-of-line comment", () => {
    expect(stripYamlComment("runs-on: windows-2025 # x64")).toBe("runs-on: windows-2025");
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
