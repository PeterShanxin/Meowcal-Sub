// Whether a Dependabot group pull request may skip maintainer review.
//
// Dependabot names groups by semver position, but Cargo and npm treat a 0.x
// minor bump (0.41 -> 0.42) as breaking and Dependabot still files it under a
// minor-patch group. It also truncates long bodies. A group is compatible only
// when its body lists every update the title counts and none of them breaks.

const UPDATE_LINE = /^Updates `[^`]+` from (\S+) to (\S+)$/;
const SEMVER = /^(\d+)\.(\d+)\.(\d+)/;

export function isBreakingUpdate(from, to) {
  const before = SEMVER.exec(from);
  const after = SEMVER.exec(to);
  if (!before || !after) {
    return true;
  }
  const [major, minor, patch] = before.slice(1).map(Number);
  const [nextMajor, nextMinor, nextPatch] = after.slice(1).map(Number);
  if (major !== nextMajor) {
    return true;
  }
  if (major === 0 && minor !== nextMinor) {
    return true;
  }
  return major === 0 && minor === 0 && patch !== nextPatch;
}

export function groupUpdatesAreCompatible(title, body) {
  const expected = Number(/with (\d+) updates?/.exec(title)?.[1] ?? 1);
  const updates = body
    .split(/\r?\n/)
    .map((line) => UPDATE_LINE.exec(line))
    .filter(Boolean);
  return (
    updates.length === expected && updates.every(([, from, to]) => !isBreakingUpdate(from, to))
  );
}
