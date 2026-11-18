import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { advisory, expires, matchesException } from "./npm-audit-policy.mjs";

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
