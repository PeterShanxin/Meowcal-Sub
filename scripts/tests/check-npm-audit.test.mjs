import assert from "node:assert/strict";
import { test } from "node:test";
import { matchesException } from "../npm-audit-policy.mjs";

function fixture() {
  const cause = {
    name: "picomatch",
    dependency: "picomatch",
    severity: "high",
    url: "https://github.com/advisories/GHSA-fc5q-39xc-vjpf",
  };
  const finding = (name, via) => ({
    name,
    severity: "high",
    isDirect: false,
    nodes: ["node_modules/" + name],
    via,
  });
  const vulnerabilities = {
    picomatch: finding("picomatch", [cause]),
    fdir: finding("fdir", ["picomatch"]),
  };
  const report = {
    auditReportVersion: 2,
    vulnerabilities,
    metadata: { vulnerabilities: { critical: 0, high: 2, total: 2 } },
  };
  const packages = {
    "node_modules/picomatch": { version: "4.0.7", dev: true },
    "node_modules/fdir": { dev: true },
  };
  return { report, packages, now: Date.parse("2026-12-02T23:59:59Z") };
}

test("accepts the tracked advisory in the known development graph", () => {
  const { report, packages, now } = fixture();
  assert.equal(matchesException(report, packages, now), true);
});

for (const [scenario, change] of Object.entries({
  "the expiry instant": (f) => (f.now = Date.parse("2026-12-03T00:00:00Z")),
  "registry error": (f) => (f.report.error = { code: "E503" }),
  "unknown report version": (f) => f.report.auditReportVersion++,
  "inconsistent counts": (f) => f.report.metadata.vulnerabilities.high++,
  "a second advisory": (f) =>
    f.report.vulnerabilities.picomatch.via.push({ url: "https://example.invalid/another" }),
  "a different advisory at the same severity": (f) =>
    (f.report.vulnerabilities.picomatch.via[0].url += "-other"),
  "a second advisory on a parent": (f) =>
    f.report.vulnerabilities.fdir.via.push({ severity: "high" }),
  "critical severity": (f) => (f.report.vulnerabilities.picomatch.severity = "critical"),
  "a production path": (f) => delete f.packages["node_modules/picomatch"].dev,
  "a production parent": (f) => delete f.packages["node_modules/fdir"].dev,
  "a nested installation": (f) =>
    f.report.vulnerabilities.picomatch.nodes.push("node_modules/a/node_modules/picomatch"),
  "a direct dependency": (f) => (f.report.vulnerabilities.picomatch.isDirect = true),
  "a different locked version": (f) => (f.packages["node_modules/picomatch"].version = "4.0.8"),
  "a new dependency edge": (f) => (f.report.vulnerabilities.fdir.via = ["vitest"]),
  "an unknown affected package": (f) => {
    f.report.vulnerabilities.other = {
      ...f.report.vulnerabilities.fdir,
      name: "other",
      nodes: ["node_modules/other"],
    };
    delete f.report.vulnerabilities.fdir;
    f.packages["node_modules/other"] = { dev: true };
  },
})) {
  test("blocks " + scenario, () => {
    const input = fixture();
    change(input);
    assert.equal(matchesException(input.report, input.packages, input.now), false);
  });
}
