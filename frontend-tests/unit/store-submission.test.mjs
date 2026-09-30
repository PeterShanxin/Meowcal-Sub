import { describe, expect, it } from "vitest";
import { createHash } from "node:crypto";
import {
  assertApp,
  createStoreClient,
  safeFailure,
  stageDraft,
  updateSubmission,
} from "../../scripts/store-submission.mjs";
import {
  assertPackageChecksums,
  parseChecksums,
  prepareRelease,
  storeReleaseNotes,
  validateRelease,
} from "../../scripts/store-release-assets.mjs";

const release = { storeVersion: "1.0.1.0", notes: "Changes in 0.8.7" };
function published() {
  return {
    id: "1",
    status: "Published",
    pricing: { priceId: "Free", other: "preserved" },
    visibility: "Public",
    targetPublishMode: "Manual",
    notesForCertification: "Keep this",
    applicationPackages: [{ fileName: "old.msix", fileStatus: "Uploaded", version: "1.0.0.0" }],
    listings: {
      "en-us": {
        baseListing: {
          title: "Meowcal",
          description: "Original",
          images: [
            { imageType: "Screenshot", fileName: "screen.png", fileStatus: "Uploaded" },
            { imageType: "Icon", fileName: "old.png", fileStatus: "Uploaded" },
          ],
        },
      },
    },
  };
}
function app(pending = null) {
  return {
    id: "9NNK2X23VLWT",
    packageIdentityName: "ShanxinLi.MeowcalSub",
    publisherName: "CN=FCD37627-8F13-4157-933C-E729F9F08408",
    lastPublishedApplicationSubmission: { id: "1" },
    pendingApplicationSubmission: pending,
  };
}

describe("draft protection and metadata", () => {
  it("rejects a pre-existing draft before creating anything", async () => {
    const calls = [];
    const client = {
      authenticate: async () => calls.push("auth"),
      getApp: async () => app({ id: "2" }),
      create: async () => calls.push("create"),
    };
    await expect(
      stageDraft(client, release, { bytes: Buffer.alloc(1) }, { stage: true }),
    ).rejects.toThrow("existing_submission_protected");
    expect(calls).toEqual(["auth"]);
  });
  it("rejects an identity or version mismatch", () => {
    expect(() => assertApp({ ...app(), publisherName: "CN=Wrong" })).toThrow(
      "store_app_identity_mismatch",
    );
    expect(() =>
      updateSubmission(
        { ...published(), status: "PendingCommit" },
        { ...release, storeVersion: "1.0.0.0" },
      ),
    ).toThrow("store_version_not_increasing");
  });
  it("rejects overlong Store notes before a draft is created", async () => {
    const calls = [];
    const client = {
      authenticate: async () => {},
      getApp: async () => app(),
      getSubmission: async () => published(),
      create: async () => calls.push("create"),
    };
    await expect(
      stageDraft(client, { ...release, notes: "x".repeat(1501) }, {}, { stage: true }),
    ).rejects.toThrow("store_release_notes_invalid");
    expect(calls).toEqual([]);
  });
  it("replaces only intended packages, icon and release notes", () => {
    const result = updateSubmission({ ...published(), status: "PendingCommit" }, release);
    expect(result.applicationPackages.map((pkg) => pkg.fileStatus)).toEqual([
      "PendingDelete",
      "PendingUpload",
      "PendingUpload",
    ]);
    expect(result.applicationPackages.slice(1).map((pkg) => pkg.fileName)).toEqual([
      "MeowcalSub-1.0.1.0-x64-Release.msix",
      "MeowcalSub-1.0.1.0-arm64-Release.msix",
    ]);
    expect(result.listings["en-us"].baseListing.images.map((image) => image.fileStatus)).toEqual([
      "Uploaded",
      "PendingDelete",
      "PendingUpload",
    ]);
    expect(result.listings["en-us"].baseListing.releaseNotes).toBe(release.notes);
    expect(result.pricing).toEqual(published().pricing);
    expect(result.notesForCertification).toBe("Keep this");
    expect(result.visibility).toBe("Public");
  });
  it("never exposes a commit mode", async () => {
    const calls = [];
    await expect(
      stageDraft(
        { authenticate: () => calls.push("auth") },
        release,
        {},
        { stage: true, commit: true },
      ),
    ).rejects.toThrow("commit_forbidden");
    expect(calls).toEqual([]);
  });
  it("leaves a created draft uncommitted after a partial upload", async () => {
    const calls = [];
    let created = false;
    const client = {
      authenticate: async () => calls.push("auth"),
      getApp: async () => app(created ? { id: "2" } : null),
      getSubmission: async () => published(),
      create: async () => {
        created = true;
        calls.push("create");
        return { ...published(), id: "2", status: "PendingCommit", fileUploadUrl: "sas" };
      },
      update: async () => calls.push("update"),
      upload: async () => {
        calls.push("upload");
        throw Error("partial_upload_draft_retained");
      },
    };
    const ids = [];
    await expect(
      stageDraft(
        client,
        release,
        { bytes: Buffer.from("zip") },
        { stage: true, onCreated: (id) => ids.push(id) },
      ),
    ).rejects.toThrow("partial_upload_draft_retained");
    expect(calls).toEqual(["auth", "create", "update", "upload"]);
    expect(ids).toEqual(["2"]);
  });
  it("stages only the new draft after rechecking ownership and preserves listing metadata", async () => {
    const calls = [];
    let created = false;
    let updated;
    const client = {
      authenticate: async () => calls.push("auth"),
      getApp: async () => {
        calls.push("app");
        return app(created ? { id: "2" } : null);
      },
      getSubmission: async (id) => {
        calls.push(`submission:${id}`);
        if (id === "1") return published();
        return updated;
      },
      create: async () => {
        calls.push("create");
        created = true;
        return { ...published(), id: "2", status: "PendingCommit", fileUploadUrl: "sas" };
      },
      update: async (id, body) => {
        calls.push(`update:${id}`);
        updated = body;
      },
      upload: async () => calls.push("upload"),
    };
    const result = await stageDraft(
      client,
      release,
      { bytes: Buffer.from("zip") },
      { stage: true },
    );
    expect(result).toEqual({
      productId: "9NNK2X23VLWT",
      submissionId: "2",
      status: "PendingCommit",
      committed: false,
    });
    expect(calls).toEqual([
      "auth",
      "app",
      "submission:1",
      "app",
      "create",
      "app",
      "update:2",
      "upload",
      "submission:2",
    ]);
    expect(updated.listings["en-us"].baseListing.title).toBe("Meowcal");
    expect(updated.listings["en-us"].baseListing.description).toBe("Original");
    expect(updated.pricing.other).toBe("preserved");
  });
  it("stops if another submission appears during preparation", async () => {
    const calls = [];
    const client = {
      authenticate: async () => {},
      getApp: async () => {
        calls.push("app");
        return app(calls.length > 1 ? { id: "existing" } : null);
      },
      getSubmission: async () => published(),
      create: async () => calls.push("create"),
    };
    await expect(stageDraft(client, release, {}, { stage: true })).rejects.toThrow(
      "existing_submission_protected",
    );
    expect(calls).toEqual(["app", "app"]);
  });
});

describe("release and transport boundaries", () => {
  const tag = "v0.8.7";
  const sha = "a".repeat(40);
  const names = [
    "latest.json",
    "SHA256SUMS.txt",
    ...["x64", "arm64"].flatMap((arch) => [
      `Meowcal.Sub_0.8.7_${arch}-setup.exe`,
      `Meowcal.Sub_0.8.7_${arch}_en-US.msi`,
      `MeowcalSub-1.0.1.0-${arch}-Release.msix`,
    ]),
  ];
  const assets = names.map((name, index) => ({
    name,
    id: index + 1,
    size: 10,
    digest: `sha256:${"a".repeat(64)}`,
  }));
  const fixture = {
    tag_name: tag,
    target_commitish: "main",
    draft: false,
    prerelease: false,
    published_at: "2026-09-30T00:00:00Z",
    html_url: `https://github.com/PeterShanxin/Meowcal-Sub/releases/tag/${tag}`,
    assets,
  };
  it("derives bounded Store highlights from the exact release notes", () => {
    const notes = `# Meowcal Sub ${tag}\n## Fixes and improvements\n- First fix.\n- ${"Long but accurate detail. ".repeat(100)}\n## Validation\n- Test evidence.`;
    const summary = storeReleaseNotes(notes, tag);
    expect(summary).toContain("- First fix.");
    expect(summary).not.toContain("Test evidence");
    expect(summary.length).toBeLessThanOrEqual(1500);
    expect(summary).toContain(`/releases/tag/${tag}`);
  });
  it("requires exactly the canonical published assets, names and digests", () => {
    expect(validateRelease(fixture, tag, sha).storeVersion).toBe("1.0.1.0");
    expect(() => validateRelease({ ...fixture, assets: assets.slice(1) }, tag, sha)).toThrow(
      "release_assets_unexpected",
    );
    expect(() =>
      validateRelease({ ...fixture, assets: [...assets, { ...assets[0], id: 99 }] }, tag, sha),
    ).toThrow();
    expect(() =>
      validateRelease(
        { ...fixture, assets: assets.map((asset) => ({ ...asset, digest: null })) },
        tag,
        sha,
      ),
    ).toThrow("release_digest_missing");
    expect(() => validateRelease({ ...fixture, target_commitish: "other" }, tag, sha)).toThrow(
      "release_identity_invalid",
    );
  });
  it("rejects missing and mismatched release checksums", () => {
    expect(() => parseChecksums("")).toThrow("release_checksums_invalid");
    const records = assets.filter((asset) => /\.(msix|msi)$|-setup\.exe$/.test(asset.name));
    const files = new Map([
      [
        "SHA256SUMS.txt",
        Buffer.from(records.map((asset) => `${"b".repeat(64)}  ${asset.name}`).join("\n")),
      ],
    ]);
    expect(() => assertPackageChecksums(validateRelease(fixture, tag, sha), files)).toThrow(
      "release_checksums_mismatch",
    );
  });
  it("redacts HTTP responses, SAS URLs and credentials from failures", async () => {
    const secret = "private-secret-value";
    const credentials = {
      AZURE_AD_TENANT_ID: "a".repeat(36),
      AZURE_AD_APPLICATION_CLIENT_ID: "b".repeat(36),
      AZURE_AD_APPLICATION_SECRET: secret,
      SELLER_ID: "seller",
    };
    const client = createStoreClient(credentials, async () => {
      throw Error(`SAS and ${secret}`);
    });
    const error = await client.authenticate().catch((caught) => caught);
    expect(safeFailure(error)).toBe("authentication_failed");
    expect(safeFailure(error)).not.toContain(secret);
    expect(safeFailure(Error(`SAS and ${secret}`))).toBe("unexpected_failure_redacted");
  });
  it("detects a partial Azure upload and never logs the SAS URL", async () => {
    const calls = [];
    const credentials = {
      AZURE_AD_TENANT_ID: "a".repeat(36),
      AZURE_AD_APPLICATION_CLIENT_ID: "b".repeat(36),
      AZURE_AD_APPLICATION_SECRET: "private-secret-value",
      SELLER_ID: "seller",
    };
    const client = createStoreClient(credentials, async (url, options) => {
      calls.push(options.method);
      expect(url).toContain("blob.core.windows.net");
      return {
        ok: true,
        headers: new Map([
          ["content-length", "2"],
          ["content-md5", "digest"],
        ]),
      };
    });
    const sas = "https://storage.blob.core.windows.net/ingestion/123?sig=private-sas";
    const error = await client
      .upload(sas, Buffer.from("three"), "digest")
      .catch((caught) => caught);
    expect(calls).toEqual(["PUT", "HEAD"]);
    expect(safeFailure(error)).toBe("partial_upload_draft_retained");
    expect(safeFailure(error)).not.toContain("private-sas");
  });
  it("prepares only checksum-bound GitHub assets and tagged source for offline inspection", async () => {
    const fileBytes = new Map();
    const installerNames = names.filter((name) => /\.(msix|msi)$|-setup\.exe$/.test(name));
    for (const name of installerNames) fileBytes.set(name, Buffer.from(`asset:${name}`));
    fileBytes.set(
      "SHA256SUMS.txt",
      Buffer.from(
        installerNames
          .map(
            (name) => `${createHash("sha256").update(fileBytes.get(name)).digest("hex")}  ${name}`,
          )
          .join("\n"),
      ),
    );
    fileBytes.set(
      "latest.json",
      Buffer.from(
        JSON.stringify({
          version: "0.8.7",
          platforms: {
            "windows-x86_64": {
              url: "https://github.com/PeterShanxin/Meowcal-Sub/releases/download/v0.8.7/Meowcal.Sub_0.8.7_x64-setup.exe",
              signature: "sig",
            },
            "windows-aarch64": {
              url: "https://github.com/PeterShanxin/Meowcal-Sub/releases/download/v0.8.7/Meowcal.Sub_0.8.7_arm64-setup.exe",
              signature: "sig",
            },
          },
        }),
      ),
    );
    const releaseAssets = names.map((name, index) => ({
      name,
      id: index + 1,
      size: fileBytes.get(name).length,
      digest: `sha256:${createHash("sha256").update(fileBytes.get(name)).digest("hex")}`,
    }));
    const logo = Buffer.alloc(32);
    Buffer.from("89504e470d0a1a0a", "hex").copy(logo);
    logo.writeUInt32BE(300, 16);
    logo.writeUInt32BE(300, 20);
    const sources = new Map([
      ["src-tauri/tauri.conf.json", Buffer.from(JSON.stringify({ version: "0.8.7" }))],
      [
        "docs/releases/v0.8.7.md",
        Buffer.from("# Meowcal Sub v0.8.7\n## Fixes and improvements\n- Fixes"),
      ],
      ["docs/assets/store-listing-logo-300.png", logo],
      ...["Square150x150Logo.png", "Square44x44Logo.png", "StoreLogo.png"].map((name) => [
        `src-tauri/icons/${name}`,
        Buffer.from(name),
      ]),
    ]);
    const requested = [];
    const client = {
      json: async (url) => {
        requested.push(url);
        if (url.startsWith("/git/ref/")) return { object: { type: "commit", sha } };
        return {
          ...fixture,
          body: "# Meowcal Sub v0.8.7\n## Fixes and improvements\n- Fixes",
          assets: releaseAssets,
        };
      },
      source: async (_commit, name) => sources.get(name),
      asset: async (asset) => fileBytes.get(asset.name),
    };
    const { mkdtemp, readdir } = await import("node:fs/promises");
    const { tmpdir } = await import("node:os");
    const { join } = await import("node:path");
    const directory = await mkdtemp(join(tmpdir(), "store-mock-"));
    let inspected = false;
    const result = await prepareRelease(client, tag, sha, directory, (_dir, version) => {
      inspected = true;
      expect(version).toBe("1.0.1.0");
    });
    expect(result.storeVersion).toBe("1.0.1.0");
    expect(inspected).toBe(true);
    expect(requested).toEqual([`/git/ref/tags/${tag}`, `/releases/tags/${tag}`]);
    expect(await readdir(directory)).toContain("store-listing-logo-300.png");
  });
});
