import { readFile, readdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import {
  checkSource,
  makeCatalog,
  readOverlayMessages,
  validateCatalog,
} from "./i18n-coverage.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const read = (name) => readFile(path.join(root, name), "utf8");
const catalog = makeCatalog(
  JSON.parse(await read("src/i18n/en-zh.json")),
  readOverlayMessages(await read("src/i18n/overlay-messages.ts")),
);
const problems = validateCatalog(catalog);
const files = (await readdir(path.join(root, "src"), { recursive: true }))
  .map((name) => `src/${name.replaceAll("\\", "/")}`)
  .filter((name) => /\.(?:html|ts|js)$/.test(name) && !name.startsWith("src/i18n/"));
for (const name of files) problems.push(...checkSource(name, await read(name), catalog));
if (problems.length) {
  console.error(
    `Translation coverage failed:\n${problems.map((entry) => `- ${entry}`).join("\n")}`,
  );
  process.exitCode = 1;
} else
  console.log(
    `Checked ${Object.keys(catalog).length} bilingual messages and ${files.length} UI source files.`,
  );
