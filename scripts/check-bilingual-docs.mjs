import { access, readFile } from "node:fs/promises";
import path from "node:path";
import { compareBilingualDocs } from "./bilingual-docs.mjs";

const pairs = [
  ["README.md", "README.zh-CN.md"],
  ["docs/USAGE.md", "docs/USAGE.zh-CN.md"],
];
let failed = false;
for (const [english, chinese] of pairs) {
  const [en, zh] = await Promise.all([readFile(english, "utf8"), readFile(chinese, "utf8")]);
  const problems = compareBilingualDocs(en, zh);
  if (!en.includes(chinese.split("/").at(-1)) || !zh.includes(english.split("/").at(-1))) {
    problems.push("missing reciprocal language link");
  }
  for (const [file, text] of [
    [english, en],
    [chinese, zh],
  ]) {
    for (const match of text.matchAll(/<img\b[^>]*\bsrc="([^"]+)"/g)) {
      if (/^https?:\/\//.test(match[1])) continue;
      try {
        await access(path.resolve(path.dirname(file), match[1]));
      } catch {
        problems.push(`${file}: missing image ${match[1]}`);
      }
    }
  }
  for (const problem of problems) console.error(`${english} / ${chinese}: ${problem}`);
  failed ||= problems.length > 0;
}
if (failed) process.exitCode = 1;
else
  console.log(
    "Bilingual user documentation structure, links, commands, and language entries match.",
  );
