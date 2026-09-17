import { describe, expect, it } from "vitest";
import {
  groupUpdatesAreCompatible,
  isBreakingUpdate,
} from "../../scripts/dependabot-update-risk.mjs";

const cargoGroup = (count) =>
  `chore(deps): bump the cargo-minor-patch group across 1 directory with ${count} updates`;

describe("isBreakingUpdate", () => {
  it.each([
    ["1.2.3", "1.3.0", false],
    ["0.8.22", "0.8.23", false],
    ["1.0.0-rc.1", "1.0.0", false],
    ["0.41.0", "0.42.0", true],
    ["0.0.3", "0.0.4", true],
    ["1.9.0", "2.0.0", true],
    ["abc123", "def456", true],
  ])("%s -> %s is breaking: %s", (from, to, breaking) => {
    expect(isBreakingUpdate(from, to)).toBe(breaking);
  });
});

describe("groupUpdatesAreCompatible", () => {
  it("accepts a group whose every counted update is compatible", () => {
    const body = [
      "Bumps the cargo-minor-patch group with 2 updates in the /src-tauri directory:",
      "Updates `plist` from 1.10.0 to 1.10.1",
      "Updates `crossbeam-utils` from 0.8.22 to 0.8.23",
    ].join("\r\n");
    expect(groupUpdatesAreCompatible(cargoGroup(2), body)).toBe(true);
  });

  it("rejects a group that hides a breaking 0.x minor update (#191)", () => {
    const body = [
      "Updates `plist` from 1.10.0 to 1.10.1",
      "Updates `quick-xml` from 0.41.0 to 0.42.0",
    ].join("\n");
    expect(groupUpdatesAreCompatible(cargoGroup(2), body)).toBe(false);
  });

  it("rejects a truncated body that lists fewer updates than the title counts (#202)", () => {
    const body = "Updates `eslint` from 10.9.1 to 10.10.0";
    expect(groupUpdatesAreCompatible(cargoGroup(24), body)).toBe(false);
  });

  it("rejects an empty body", () => {
    expect(groupUpdatesAreCompatible(cargoGroup(1), "")).toBe(false);
  });

  it("expects one update when the title names a single dependency", () => {
    const title = "chore(deps): bump plist from 1.10.0 to 1.10.1 in the cargo-minor-patch group";
    expect(groupUpdatesAreCompatible(title, "Updates `plist` from 1.10.0 to 1.10.1")).toBe(true);
  });
});
