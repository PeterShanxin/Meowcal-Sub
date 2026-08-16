/**
 * Did the measured coverage scope grow, in the sense that matters?
 *
 * Only a superset counts. A scope that swapped one module for another is not
 * bigger, and a floor lowered against it would be a floor lowered for free.
 */
function coverageScopeGrew(current = [], previous = []) {
  const currentSet = new Set(current);
  const previousSet = new Set(previous);
  return previous.every((module) => currentSet.has(module)) && currentSet.size > previousSet.size;
}

export function findRatchetRegressions(current, previous) {
  const violations = [];

  // The scope is the denominator of every coverage number. Dropping a module
  // raises the percentage without a line of new test code, which is the cheapest
  // way there is to make a coverage claim mean less than it says.
  const currentScope = new Set(current.frontendCoverageScope ?? []);
  for (const module of previous.frontendCoverageScope ?? []) {
    if (!currentScope.has(module)) {
      violations.push(`frontendCoverageScope no longer measures ${module}`);
    }
  }

  // Widening the scope pulls in code the old floors never described, so the
  // percentage can legitimately fall while the claim gets stronger. That is the
  // only circumstance in which a floor may drop, and it is verifiable from the
  // manifest rather than from a promise in a pull request.
  const scopeGrew = coverageScopeGrew(
    current.frontendCoverageScope,
    previous.frontendCoverageScope,
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
    if (typeof currentMinimum === "number" && currentMinimum < previousMinimum && !scopeGrew) {
      violations.push(
        `frontendCoverageMinimum.${metric} decreased from ${previousMinimum} to ${currentMinimum}`,
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
