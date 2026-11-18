import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const advisory = "https://github.com/advisories/GHSA-fc5q-39xc-vjpf";
const expires = "2026-12-03T00:00:00Z";

// Only the development-tool edges in the reviewed npm audit v2 report.
const knownCauses = new Map(
  Object.entries({
    "@typescript-eslint/eslint-plugin": [
      "@typescript-eslint/parser",
      "@typescript-eslint/type-utils",
      "@typescript-eslint/utils",
    ],
    "@typescript-eslint/parser": ["@typescript-eslint/typescript-estree"],
    "@typescript-eslint/type-utils": [
      "@typescript-eslint/typescript-estree",
      "@typescript-eslint/utils",
    ],
    "@typescript-eslint/typescript-estree": ["tinyglobby"],
    "@typescript-eslint/utils": ["@typescript-eslint/typescript-estree"],
    "@vitest/coverage-v8": ["vitest"],
    fdir: ["picomatch"],
    tinyglobby: ["fdir", "picomatch"],
    "typescript-eslint": [
      "@typescript-eslint/eslint-plugin",
      "@typescript-eslint/parser",
      "@typescript-eslint/typescript-estree",
      "@typescript-eslint/utils",
    ],
    vite: ["picomatch", "tinyglobby"],
    vitest: ["@vitest/coverage-v8", "picomatch", "tinyglobby"],
  }),
);

export function matchesException(report, packages, now) {
  if (report?.error || report?.auditReportVersion !== 2 || !(now < Date.parse(expires)))
    return false;
  const findings = report.vulnerabilities;
  const counts = report.metadata?.vulnerabilities;
  if (!findings || !counts || counts.critical !== 0 || !Object.hasOwn(findings, "picomatch"))
    return false;
  const entries = Object.entries(findings);
  const high = entries.filter(([, item]) => item?.severity === "high");
  if (
    entries.length !== counts.total ||
    high.length !== counts.high ||
    !high.length ||
    !entries.every(([, item]) => ["info", "low", "moderate", "high"].includes(item?.severity))
  )
    return false;

  const root = findings.picomatch;
  if (
    root.severity !== "high" ||
    root.isDirect !== false ||
    packages["node_modules/picomatch"]?.version !== "4.0.7"
  )
    return false;
  return high.every(([name, item]) => {
    const location = "node_modules/" + name;
    if (
      item.name !== name ||
      item.nodes?.length !== 1 ||
      item.nodes[0] !== location ||
      packages[location]?.dev !== true ||
      !Array.isArray(item.via) ||
      !item.via.length
    )
      return false;
    if (name === "picomatch") {
      const cause = item.via[0];
      return (
        item.via.length === 1 &&
        cause?.url === advisory &&
        cause.name === name &&
        cause.dependency === name &&
        cause.severity === "high"
      );
    }
    const expected = knownCauses.get(name);
    return (
      expected?.length === item.via.length &&
      expected.every((cause) => item.via.includes(cause) && findings[cause]?.severity === "high")
    );
  });
}

if (import.meta.main) {
  const result = spawnSync(
    process.execPath,
    [
      process.env.npm_execpath,
      "audit",
      "--package-lock-only",
      "--include=dev",
      "--include=peer",
      "--include=optional",
      "--audit-level=high",
      "--json",
    ],
    { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] },
  );
  if (result.error) throw result.error;
  const report = JSON.parse(result.stdout);
  if (
    result.status === 0 &&
    report.auditReportVersion === 2 &&
    !report.error &&
    report.metadata?.vulnerabilities?.high === 0 &&
    report.metadata?.vulnerabilities?.critical === 0
  ) {
    console.log("npm audit passed at the high threshold.");
  } else if (
    result.status === 1 &&
    matchesException(
      report,
      JSON.parse(readFileSync("package-lock.json", "utf8")).packages,
      Date.now(),
    )
  ) {
    console.warn(
      `Known development dependency finding: ${advisory} (picomatch 4.0.7). Exception expires ${expires}.`,
    );
  } else {
    console.error(result.stdout);
    process.exitCode = result.status || 1;
  }
}
