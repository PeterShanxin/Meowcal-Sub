/**
 * How far a coverage floor may fall in one scope-widening change.
 *
 * A widening that costs more than this is not an adjustment to the same claim,
 * it is a different claim - split it, or argue it explicitly rather than letting
 * the ratchet wave it through. Without a cap, adding one module would license
 * dropping every floor to zero, which is a downward ratchet in name only.
 */
export const MAX_COVERAGE_FLOOR_DROP = 15;

/**
 * Did the measured coverage scope grow, in the sense that matters?
 *
 * Only a superset counts, ignoring modules whose files are gone. A scope that
 * swapped one module for another is not bigger, and a floor lowered against it
 * would be a floor lowered for free.
 */
function coverageScopeGrew(current = [], previous = [], stillPresent) {
  const currentSet = new Set(current);
  const retained = previous.filter(stillPresent);
  return retained.every((module) => currentSet.has(module)) && currentSet.size > retained.length;
}

/**
 * @param {object} current baseline being proposed
 * @param {object} previous baseline it is compared against
 * @param {{ fileExists?: (relativePath: string) => boolean }} [options]
 *   `fileExists` lets the caller say which scoped paths still exist. A module
 *   whose file was deleted has to be able to leave the scope, or a legitimate
 *   removal has no passing baseline update at all. Defaults to "everything still
 *   exists", which keeps this function pure for its unit tests.
 */
export function findRatchetRegressions(current, previous, { fileExists = () => true } = {}) {
  const violations = [];

  // The scope is the denominator of every coverage number. Dropping a module
  // raises the percentage without a line of new test code, which is the cheapest
  // way there is to make a coverage claim mean less than it says - so a module
  // may only leave when its file does.
  const currentScope = new Set(current.frontendCoverageScope ?? []);
  for (const module of previous.frontendCoverageScope ?? []) {
    if (!currentScope.has(module) && fileExists(module)) {
      violations.push(`frontendCoverageScope no longer measures ${module}, which still exists`);
    }
  }

  // Widening the scope pulls in code the old floors never described, so the
  // percentage can legitimately fall while the claim gets stronger. That is the
  // only circumstance in which a floor may drop, and it is verifiable from the
  // manifest rather than from a promise in a pull request.
  const scopeGrew = coverageScopeGrew(
    current.frontendCoverageScope,
    previous.frontendCoverageScope,
    fileExists,
  );

  if (current.newProductionFileMaxLines > previous.newProductionFileMaxLines) {
    violations.push(
      `newProductionFileMaxLines increased from ${previous.newProductionFileMaxLines} to ${current.newProductionFileMaxLines}`,
    );
  }
  if (current.eslintMaxWarnings > previous.eslintMaxWarnings) {
    violations.push(
      `eslintMaxWarnings increased from ${previous.eslintMaxWarnings} to ${current.eslintMaxWarnings}`,
    );
  }

  for (const [metric, previousMinimum] of Object.entries(previous.frontendCoverageMinimum)) {
    const currentMinimum = current.frontendCoverageMinimum[metric];
    if (typeof currentMinimum !== "number" || currentMinimum >= previousMinimum) {
      continue;
    }
    if (!scopeGrew) {
      violations.push(
        `frontendCoverageMinimum.${metric} decreased from ${previousMinimum} to ${currentMinimum}`,
      );
    } else if (previousMinimum - currentMinimum > MAX_COVERAGE_FLOOR_DROP) {
      violations.push(
        `frontendCoverageMinimum.${metric} fell ${previousMinimum - currentMinimum} points ` +
          `(${previousMinimum} to ${currentMinimum}); a scope widening may lower a floor by at ` +
          `most ${MAX_COVERAGE_FLOOR_DROP}`,
      );
    }
  }

  for (const [relativePath, previousCeiling] of Object.entries(previous.legacyFileMaxLines)) {
    const currentCeiling = current.legacyFileMaxLines[relativePath];
    if (typeof currentCeiling === "number" && currentCeiling > previousCeiling) {
      violations.push(
        `${relativePath}: legacy ceiling increased from ${previousCeiling} to ${currentCeiling}`,
      );
    }
  }

  for (const relativePath of Object.keys(current.legacyFileMaxLines)) {
    if (!Object.hasOwn(previous.legacyFileMaxLines, relativePath)) {
      violations.push(`${relativePath}: new legacy exceptions are not allowed`);
    }
  }

  return violations;
}
