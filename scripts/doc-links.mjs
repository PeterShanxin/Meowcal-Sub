// Finds Markdown links that point at a file this repository does not have,
// kept separate from the CLI in check-doc-links.mjs so the rules can be tested
// against crafted documents.
//
// The failure this prevents is quiet: a document is moved or renamed, every
// other document keeps its old link, and the guidance still reads as if it
// resolves. Cross-document links are how the contract is navigated here - the
// ADR index, the document-class list, CONTRIBUTING's read order - so a dead one
// is a governance defect, not a typo.

// Inline links and reference definitions. Bare autolinks (<https://...>) are
// external by definition and are not collected.
const INLINE_LINK = /!?\[[^\]]*\]\(\s*([^)\s]+)(?:\s+"[^"]*")?\s*\)/g;
const REFERENCE_DEFINITION = /^\s{0,3}\[[^\]]+\]:\s*(\S+)/gm;

const FENCE = /^\s{0,3}(```+|~~~+)/;

/**
 * Removes fenced code blocks.
 *
 * Examples in this repository's documentation contain link-shaped text -
 * templates, sample commit bodies - that must not be resolved as real links.
 * Lines are replaced rather than deleted so reported line numbers stay true.
 */
export function stripFencedBlocks(contents) {
  const lines = contents.split(/\r?\n/);
  let fence = null;

  return lines
    .map((line) => {
      const match = FENCE.exec(line);
      if (fence) {
        if (match && line.trim().startsWith(fence)) {
          fence = null;
        }
        return "";
      }
      if (match) {
        fence = match[1];
        return "";
      }
      return line;
    })
    .join("\n");
}

function isExternal(target) {
  return /^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(target);
}

/**
 * Collects the repository-relative file paths a document links to.
 *
 * `[text](#anchor)` is same-document and has no file part, so it is skipped.
 * A `path#anchor` link contributes its path: the file must exist. Anchors
 * themselves are not resolved - that would mean parsing every heading of every
 * target, and a wrong anchor still lands the reader on the right document.
 */
export function collectLinkTargets(contents) {
  const body = stripFencedBlocks(contents);
  const targets = [];

  for (const pattern of [INLINE_LINK, REFERENCE_DEFINITION]) {
    pattern.lastIndex = 0;
    for (const match of body.matchAll(pattern)) {
      let target = match[1].trim();
      if (target.startsWith("<") && target.endsWith(">")) {
        target = target.slice(1, -1);
      }
      if (target === "" || target.startsWith("#") || isExternal(target)) {
        continue;
      }
      const [pathPart] = target.split("#");
      if (pathPart === "") {
        continue;
      }
      targets.push(decodeURIComponent(pathPart));
    }
  }

  return targets;
}
