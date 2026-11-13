// Builds the product site into a directory that GitHub Pages can serve as is.
//
//   node scripts/site/build.mjs [--out <directory>]
//
// The default output is .local/site, which Git ignores.

import { access, cp, lstat, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import sharp from "sharp";
import { IMAGES, IMAGE_WIDTHS, imageFile, readSub1Release, renderSite } from "./render.mjs";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const { values } = parseArgs({ options: { out: { type: "string", default: ".local/site" } } });
const outputRoot = path.resolve(repositoryRoot, values.out);

// Only these generated directories belong to the build. Never clean a caller's
// source directory or follow a junction to data outside this checkout.
const allowedOutputs = [".local/site", "_site"].map((dir) => path.resolve(repositoryRoot, dir));
if (!allowedOutputs.includes(outputRoot)) {
  throw new Error("Choose a designated build output: .local/site or _site.");
}
for (
  let directory = outputRoot;
  directory !== repositoryRoot;
  directory = path.dirname(directory)
) {
  const entry = await lstat(directory).catch((error) => {
    if (error.code !== "ENOENT") throw error;
    return null;
  });
  if (entry && (!entry.isDirectory() || entry.isSymbolicLink())) {
    throw new Error(`Build output must use real directories, not links: ${directory}`);
  }
}

const STATIC_FILES = [
  ["site/styles.css", "styles.css"],
  // Proves ownership of the site to Google Search Console. Removing it ends the verification.
  ["site/google4bfbcf53cdd665f4.html", "google4bfbcf53cdd665f4.html"],
  ["site/assets/sub2-icon.svg", "assets/sub2-icon.svg"],
  ["site/assets/sub2-social-preview.png", "assets/sub2-social-preview.png"],
  ["docs/assets/logo.svg", "assets/favicon.svg"],
  ["docs/assets/mark.svg", "assets/mark.svg"],
  ["docs/assets/social-preview.png", "assets/social-preview.png"],
];

const source = (file) => path.join(repositoryRoot, file);
const output = (file) => path.join(outputRoot, file);

await rm(outputRoot, { recursive: true, force: true });
await mkdir(output("assets"), { recursive: true });

const release = readSub1Release(await readFile(source("README.md"), "utf8"));
const documents = renderSite(release);
for (const document of documents) {
  await mkdir(path.dirname(output(document.path)), { recursive: true });
  await writeFile(output(document.path), document.contents, "utf8");
}

for (const [from, to] of STATIC_FILES) {
  await cp(source(from), output(to));
}

for (const [id, { sources, crop }] of Object.entries(IMAGES)) {
  for (const [locale, file] of Object.entries(sources)) {
    for (const width of IMAGE_WIDTHS) {
      const picture = crop ? sharp(source(file)).extract(crop) : sharp(source(file));
      await picture
        .resize({ width })
        .webp({ quality: 82 })
        .toFile(output(imageFile(id, locale, width)));
    }
  }
}

// A renamed asset or route must fail the build, not ship a broken page.
const missing = [];
for (const document of documents.filter(({ path: file }) => file.endsWith(".html"))) {
  const references = [
    ...document.contents.matchAll(/(?:href|src)="([^"]+)"|([^\s",]+\.webp) \d+w/g),
  ]
    .map((match) => match[1] ?? match[2])
    .filter((reference) => !/^(?:[a-z][a-z0-9+.-]*:|#)/i.test(reference));
  for (const reference of new Set(references)) {
    const target = path.join(path.dirname(output(document.path)), reference.split("#")[0]);
    const file = reference.endsWith("/") ? path.join(target, "index.html") : target;
    await access(file).catch(() => missing.push(`${document.path}: ${reference}`));
  }
}
if (missing.length > 0) {
  console.error(`Unresolved site references:\n  ${missing.join("\n  ")}`);
  process.exit(1);
}

console.log(
  `Built ${documents.length} documents for Meowcal Sub v${release.version} in ${path.relative(repositoryRoot, outputRoot)}.`,
);
