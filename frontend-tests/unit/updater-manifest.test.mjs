import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import {
  buildUpdaterManifest,
  githubAssetName,
  selectUpdaterArtifacts,
  UPDATER_PLATFORMS,
} from "../../scripts/build-updater-manifest.mjs";

const X64 = "Meowcal Sub_0.6.6_x64-setup.exe";
const ARM64 = "Meowcal Sub_0.6.6_arm64-setup.exe";

function releaseAssets(extra = []) {
  return [
    X64,
    `${X64}.sig`,
    ARM64,
    `${ARM64}.sig`,
    "Meowcal Sub_0.6.6_x64_en-US.msi",
    "Meowcal Sub_0.6.6_arm64_en-US.msi",
    "SHA256SUMS.txt",
    ...extra,
  ];
}

describe("updater artifact selection", () => {
  // Offering an ARM64 machine the x64 installer produces an install that runs
  // once and then does not, so the mapping gets asserted rather than trusted.
  it("maps each architecture to the platform key that machine asks for", () => {
    const selected = selectUpdaterArtifacts(releaseAssets());

    expect(selected[UPDATER_PLATFORMS.x64].installer).toBe(X64);
    expect(selected[UPDATER_PLATFORMS.arm64].installer).toBe(ARM64);
    expect(selected["windows-x86_64"].installer).toContain("x64");
    expect(selected["windows-aarch64"].installer).toContain("arm64");
  });

  it("ignores the MSI bundles, which are not the updater's install path", () => {
    const selected = selectUpdaterArtifacts(releaseAssets());

    for (const entry of Object.values(selected)) {
      expect(entry.installer.endsWith("-setup.exe")).toBe(true);
    }
  });

  it("refuses a release that would leave one architecture unable to update", () => {
    const withoutArm = releaseAssets().filter((name) => !name.includes("arm64"));

    expect(() => selectUpdaterArtifacts(withoutArm)).toThrow(/arm64 installer/);
  });

  it("refuses an unsigned installer instead of shipping a manifest that fails later", () => {
    const unsigned = releaseAssets().filter((name) => name !== `${ARM64}.sig`);

    expect(() => selectUpdaterArtifacts(unsigned)).toThrow(/TAURI_SIGNING_PRIVATE_KEY/);
  });

  it("refuses two installers for the same architecture", () => {
    const duplicated = releaseAssets(["Meowcal Sub_0.6.5_x64-setup.exe"]);

    expect(() => selectUpdaterArtifacts(duplicated)).toThrow(/Two x64 installers/);
  });

  it("refuses an installer whose architecture cannot be read from its name", () => {
    const nameless = releaseAssets(["Meowcal Sub_0.6.6-setup.exe"]);

    expect(() => selectUpdaterArtifacts(nameless)).toThrow(/no recognisable architecture/);
  });
});

describe("manifest assembly", () => {
  // GitHub stores `Meowcal Sub_...` as `Meowcal.Sub_...`; a URL built from the
  // local file name 404s for every user who asks for an update.
  it("builds download URLs from the name GitHub actually serves", () => {
    expect(githubAssetName(X64)).toBe("Meowcal.Sub_0.6.6_x64-setup.exe");

    const manifest = buildUpdaterManifest({
      artifacts: selectUpdaterArtifacts(releaseAssets()),
      signatures: { [`${X64}.sig`]: "sig-x64\n", [`${ARM64}.sig`]: "sig-arm64\n" },
      version: "0.6.6",
      notes: "Release notes",
      pubDate: "2026-08-04T00:00:00Z",
      baseUrl: "https://github.com/PeterShanxin/Meowcal-Sub/releases/download/v0.6.6",
    });

    expect(manifest.platforms["windows-x86_64"].url).toBe(
      "https://github.com/PeterShanxin/Meowcal-Sub/releases/download/v0.6.6/Meowcal.Sub_0.6.6_x64-setup.exe",
    );
    expect(manifest.platforms["windows-x86_64"].url).not.toContain(" ");
  });

  it("carries the exact signature text the updater verifies against", () => {
    const manifest = buildUpdaterManifest({
      artifacts: selectUpdaterArtifacts(releaseAssets()),
      signatures: { [`${X64}.sig`]: "  sig-x64\n", [`${ARM64}.sig`]: "sig-arm64" },
      version: "0.6.6",
      notes: "",
      pubDate: "2026-08-04T00:00:00Z",
      baseUrl: "https://example.invalid/download",
    });

    expect(manifest.platforms["windows-x86_64"].signature).toBe("sig-x64");
    expect(manifest.platforms["windows-aarch64"].signature).toBe("sig-arm64");
  });

  it("keeps the required top-level fields the updater validates first", () => {
    const manifest = buildUpdaterManifest({
      artifacts: selectUpdaterArtifacts(releaseAssets()),
      signatures: { [`${X64}.sig`]: "a", [`${ARM64}.sig`]: "b" },
      version: "0.6.6",
      notes: "Notes",
      pubDate: "2026-08-04T00:00:00Z",
      baseUrl: "https://example.invalid/download",
    });

    expect(manifest.version).toBe("0.6.6");
    expect(manifest.pub_date).toBe("2026-08-04T00:00:00Z");
    expect(Object.keys(manifest.platforms).sort()).toEqual(["windows-aarch64", "windows-x86_64"]);
  });

  it("refuses an empty signature rather than publishing an unverifiable update", () => {
    expect(() =>
      buildUpdaterManifest({
        artifacts: selectUpdaterArtifacts(releaseAssets()),
        signatures: { [`${X64}.sig`]: "   ", [`${ARM64}.sig`]: "b" },
        version: "0.6.6",
        notes: "",
        pubDate: "2026-08-04T00:00:00Z",
        baseUrl: "https://example.invalid/download",
      }),
    ).toThrow(/empty/);
  });
});

describe("canonical update channel", () => {
  it("keeps future clients and release manifests on the main repository", () => {
    const config = JSON.parse(
      readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
    );
    expect(config.plugins.updater.endpoints).toEqual([
      "https://github.com/PeterShanxin/Meowcal-Sub/releases/latest/download/latest.json",
    ]);
    expect(config.bundle.homepage).toBe("https://github.com/PeterShanxin/Meowcal-Sub");

    const releaseWorkflow = readFileSync(
      new URL("../../.github/workflows/release.yml", import.meta.url),
      "utf8",
    );
    expect(releaseWorkflow).toContain("--repo $env:GITHUB_REPOSITORY");
    expect(releaseWorkflow).not.toContain("UPDATE_MIRROR_REPO");
  });
});
