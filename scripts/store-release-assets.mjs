import { createHash } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { requireCondition, SubmissionError } from "./store-submission.mjs";

const REPO = "PeterShanxin/Meowcal-Sub";
const API = `https://api.github.com/repos/${REPO}`;
const REQUIRED = (version) => [
  "latest.json",
  "SHA256SUMS.txt",
  ...["x64", "arm64"].flatMap((arch) => [
    `Meowcal.Sub_${version}_${arch}-setup.exe`,
    `Meowcal.Sub_${version}_${arch}_en-US.msi`,
  ]),
];

export function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

export function parseChecksums(contents) {
  const found = new Map();
  for (const line of contents.trim().split(/\r?\n/)) {
    const match = /^([a-f\d]{64})\s+\*?([A-Za-z\d._-]+)$/i.exec(line);
    requireCondition(match && !found.has(match[2]), "release_checksums_invalid");
    found.set(match[2], match[1].toLowerCase());
  }
  requireCondition(found.size === 6, "release_checksums_invalid");
  return found;
}

export function validateRelease(release, tag, commit) {
  requireCondition(
    /^v\d+\.\d+\.\d+$/.test(tag) && /^[a-f\d]{40}$/i.test(commit),
    "release_input_invalid",
  );
  requireCondition(
    release.tag_name === tag &&
      ["main", commit].includes(release.target_commitish) &&
      !release.draft &&
      !release.prerelease &&
      release.published_at &&
      release.html_url === `https://github.com/${REPO}/releases/tag/${tag}`,
    "release_identity_invalid",
  );
  const version = tag.slice(1);
  const names = REQUIRED(version);
  const store = release.assets.filter((asset) =>
    /^MeowcalSub-.*-(x64|arm64)-Release\.msix$/.test(asset.name),
  );
  requireCondition(store.length === 2, "store_assets_missing");
  const match = /^MeowcalSub-(\d+\.\d+\.\d+\.0)-x64-Release\.msix$/.exec(
    store.find((a) => a.name.endsWith("-x64-Release.msix"))?.name ?? "",
  );
  requireCondition(
    match && store.some((a) => a.name === `MeowcalSub-${match[1]}-arm64-Release.msix`),
    "store_assets_mismatch",
  );
  names.push(...store.map((asset) => asset.name));
  requireCondition(
    release.assets.length === names.length &&
      names.every((name) => release.assets.filter((asset) => asset.name === name).length === 1),
    "release_assets_unexpected",
  );
  for (const asset of release.assets) {
    requireCondition(
      Number.isSafeInteger(asset.id) &&
        asset.size > 0 &&
        /^sha256:[a-f\d]{64}$/i.test(asset.digest ?? ""),
      "release_digest_missing",
    );
  }
  return {
    version,
    storeVersion: match[1],
    assets: new Map(release.assets.map((asset) => [asset.name, asset])),
  };
}

export function assertSource(source, release, tag) {
  requireCondition(
    source.version === release.version &&
      source.notes.trim() === release.body.trim() &&
      release.body.includes(`# Meowcal Sub ${tag}`),
    "release_notes_mismatch",
  );
  const png = source.logo;
  requireCondition(
    png.length > 24 &&
      png.subarray(0, 8).equals(Buffer.from("89504e470d0a1a0a", "hex")) &&
      png.readUInt32BE(16) === 300 &&
      png.readUInt32BE(20) === 300,
    "listing_icon_invalid",
  );
}

export function assertPackageChecksums(release, files) {
  const sums = parseChecksums(files.get("SHA256SUMS.txt").toString("utf8"));
  const expectedNames = [...release.assets.keys()].filter(
    (name) => name.endsWith(".msix") || name.endsWith(".msi") || name.endsWith("-setup.exe"),
  );
  requireCondition(
    expectedNames.length === sums.size &&
      expectedNames.every(
        (name) => sums.get(name) === release.assets.get(name).digest.slice(7).toLowerCase(),
      ),
    "release_checksums_mismatch",
  );
  for (const [name, bytes] of files) {
    requireCondition(
      release.assets.get(name)?.digest.toLowerCase() === `sha256:${sha256(bytes)}` &&
        release.assets.get(name).size === bytes.length,
      "release_asset_corrupt",
    );
  }
  const manifest = JSON.parse(files.get("latest.json").toString("utf8"));
  requireCondition(
    manifest.version === release.version &&
      ["windows-x86_64", "windows-aarch64"].every((platform, index) => {
        const arch = index === 0 ? "x64" : "arm64";
        return (
          manifest.platforms?.[platform]?.url ===
            `https://github.com/${REPO}/releases/download/v${release.version}/Meowcal.Sub_${release.version}_${arch}-setup.exe` &&
          typeof manifest.platforms?.[platform]?.signature === "string" &&
          manifest.platforms[platform].signature.length > 0
        );
      }),
    "release_updater_invalid",
  );
}

export function createGitHubClient(token, fetchImpl = fetch) {
  async function request(url, accept = "application/vnd.github+json") {
    try {
      const response = await fetchImpl(url, {
        headers: { Accept: accept, Authorization: `Bearer ${token}` },
        signal: AbortSignal.timeout(120000),
      });
      requireCondition(response.ok, "github_release_unavailable");
      return response;
    } catch {
      throw new SubmissionError("github_release_unavailable");
    }
  }
  return {
    async json(suffix) {
      return (await request(`${API}${suffix}`)).json();
    },
    async source(commit, name) {
      const response = await this.json(`/contents/${name}?ref=${commit}`);
      requireCondition(
        response.encoding === "base64" && response.type === "file",
        "release_source_invalid",
      );
      return Buffer.from(response.content.replace(/\s/g, ""), "base64");
    },
    async asset(asset) {
      const response = await request(
        `${API}/releases/assets/${asset.id}`,
        "application/octet-stream",
      );
      return Buffer.from(await response.arrayBuffer());
    },
  };
}

export async function prepareRelease(client, tag, commit, directory, inspector) {
  const ref = await client.json(`/git/ref/tags/${tag}`);
  requireCondition(
    ref.object?.type === "commit" && ref.object.sha === commit,
    "release_tag_mismatch",
  );
  const raw = await client.json(`/releases/tags/${tag}`);
  const release = validateRelease(raw, tag, commit);
  const source = {
    version: JSON.parse((await client.source(commit, "src-tauri/tauri.conf.json")).toString("utf8"))
      .version,
    notes: (await client.source(commit, `docs/releases/${tag}.md`)).toString("utf8"),
    logo: await client.source(commit, "docs/assets/store-listing-logo-300.png"),
    icons: await Promise.all(
      ["Square150x150Logo.png", "Square44x44Logo.png", "StoreLogo.png"].map((name) =>
        client.source(commit, `src-tauri/icons/${name}`),
      ),
    ),
  };
  assertSource(source, { ...release, body: raw.body }, tag);
  const files = new Map();
  const downloaded = [
    "latest.json",
    "SHA256SUMS.txt",
    ...["x64", "arm64"].map((arch) => `MeowcalSub-${release.storeVersion}-${arch}-Release.msix`),
  ];
  await mkdir(directory, { recursive: true });
  for (const name of downloaded) files.set(name, await client.asset(release.assets.get(name)));
  assertPackageChecksums(release, files);
  for (const [name, bytes] of files)
    await writeFile(path.join(directory, name), bytes, { flag: "wx" });
  await writeFile(path.join(directory, "store-listing-logo-300.png"), source.logo, { flag: "wx" });
  for (const [index, name] of [
    "Square150x150Logo.png",
    "Square44x44Logo.png",
    "StoreLogo.png",
  ].entries()) {
    await writeFile(path.join(directory, name), source.icons[index], { flag: "wx" });
  }
  await inspector(directory, release.storeVersion);
  return { ...release, notes: source.notes, commit };
}
