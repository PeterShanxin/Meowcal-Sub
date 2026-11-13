function headings(text) {
  return [...text.matchAll(/^(#{2,6}) /gm)].map((match) => match[1].length);
}

function destinations(text) {
  const markdown = [...text.matchAll(/\]\(([^)\s]+)\)/g)].map((match) => match[1]);
  const html = [...text.matchAll(/(?:href|src)="([^"]+)"/g)].map((match) => match[1]);
  return [
    ...new Set(
      [...markdown, ...html]
        .filter((target) => !target.startsWith("#"))
        .map((target) => target.split("#")[0].replaceAll(".zh-CN", "")),
    ),
  ].sort();
}

function codeBlocks(text) {
  return [...text.matchAll(/^```[^\n]*\n([\s\S]*?)^```/gm)].map((match) =>
    match[1].replaceAll("\r", "").trim(),
  );
}

/** Structural parity catches omissions; human review still owns translation meaning. */
export function compareBilingualDocs(english, chinese) {
  const failures = [];
  for (const [label, extract] of [
    ["section structure", headings],
    ["links and release downloads", destinations],
    ["example commands", codeBlocks],
  ]) {
    if (JSON.stringify(extract(english)) !== JSON.stringify(extract(chinese))) {
      failures.push(`${label} differ between English and Simplified Chinese`);
    }
  }
  if (/妙幕|妙译/.test(chinese)) failures.push("Chinese brand must be 妙影");
  return failures;
}
