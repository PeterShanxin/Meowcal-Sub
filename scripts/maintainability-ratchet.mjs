export function findRatchetRegressions(current, previous) {
  const violations = [];

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
    if (typeof currentMinimum === "number" && currentMinimum < previousMinimum) {
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
