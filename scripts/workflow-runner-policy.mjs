// Runner policy rules for .github/workflows, kept separate from the CLI in
// check-workflow-runners.mjs so they can be tested against crafted workflows.
//
// Every job runs on a GitHub-hosted runner. No workflow may reach for a
// self-hosted runner: that would put a release, or a required check, behind a
// physical machine somebody has to keep online, and would place the update
// signing key on a long-lived host.
//
// The other failure this prevents is quiet: someone reaches for
// `windows-latest` or a dated `windows-20xx` image out of habit and gets an
// image the packaging contract was never proven on, or picks macOS, which this
// repository ships nothing for.

// Every runner label a job may name. Packaging is native per architecture:
// `windows-2025` emits x64 and `windows-11-arm` emits ARM64. The Ubuntu images
// carry release administration, the Change Contract, and the required-check
// wrappers.
const ALLOWED_RUNNERS = new Set([
  "ubuntu-latest",
  "ubuntu-24.04",
  "windows-11-arm",
  "windows-2025",
]);

// The same names anywhere else in a workflow: a matrix entry, a container
// image, a reusable-workflow input. `runs-on: ${{ matrix.os }}` is already
// rejected as indirect, so this is defence in depth against a value that
// reaches a runner by some route this file does not model.
const FORBIDDEN_ANYWHERE =
  /\b(?:self-hosted|windows-latest|windows-2019|windows-2022|macos-[\w.-]+)\b/g;

/**
 * Removes a YAML end-of-line comment.
 *
 * This is load-bearing rather than cosmetic. Matching runner labels against raw
 * line text lets a comment vouch for the code beside it, and would also let a
 * comment that merely *names* `windows-latest`, as these workflows do when
 * explaining the policy, be read as a use of it.
 *
 * A `#` opens a comment only at the start of a line or after whitespace, and
 * never inside a quoted scalar.
 */
export function stripYamlComment(line) {
  let quote = null;

  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];

    if (quote) {
      if (character === quote) {
        quote = null;
      }
      continue;
    }

    if (character === '"' || character === "'") {
      quote = character;
      continue;
    }

    if (character === "#" && (index === 0 || /\s/.test(line[index - 1]))) {
      return line.slice(0, index).trimEnd();
    }
  }

  return line;
}

/**
 * The jobs of a workflow, each as the block of lines under its key.
 *
 * Line-based on purpose, matching the rest of this file: these workflows use
 * expression syntax and folded scalars that a general YAML loader would
 * normalize away, and the checks here are about the text a reviewer reads.
 *
 * Exported for the workflow contract tests, which assert per-job invariants -
 * runner, trusted actor, credential handling - that only make sense per job.
 */
export function splitWorkflowJobs(contents) {
  const lines = contents.split(/\r?\n/);
  const jobsIndex = lines.findIndex((line) => /^(\s*)jobs:\s*$/.test(stripYamlComment(line)));
  if (jobsIndex === -1) {
    return [];
  }

  const jobsIndent = /^\s*/.exec(lines[jobsIndex])[0].length;
  const jobs = [];
  let current = null;

  for (let index = jobsIndex + 1; index < lines.length; index += 1) {
    const line = stripYamlComment(lines[index]);
    if (line.trim() === "") {
      continue;
    }

    const indent = /^\s*/.exec(line)[0].length;
    if (indent <= jobsIndent) {
      break;
    }

    const header = /^\s*([A-Za-z_][\w.-]*):\s*$/.exec(line);
    if (header && (current === null || indent === current.indent)) {
      current = { name: header[1], indent, lines: [] };
      jobs.push(current);
      continue;
    }

    if (current) {
      current.lines.push(line);
    }
  }

  return jobs.map((job) => ({ name: job.name, lines: job.lines }));
}

function findForbiddenRunners(relativePath, lines) {
  const violations = [];

  for (const [index, text] of lines.entries()) {
    for (const match of stripYamlComment(text).matchAll(FORBIDDEN_ANYWHERE)) {
      violations.push(
        `${relativePath}:${index + 1}: '${match[0]}' is not an allowed runner. ` +
          `Every job runs GitHub-hosted: ${[...ALLOWED_RUNNERS].join(", ")}.`,
      );
    }
  }

  return violations;
}

function collectRunsOnValue(lines, index, indentWidth) {
  // A block scalar (`>-`, `|`) or a bare list continues on the following
  // more-indented lines. Gather them so the check sees the whole value, minus
  // any comments among them.
  const collected = [];
  let everyLineIsListItem = true;

  for (let next = index + 1; next < lines.length; next += 1) {
    const line = stripYamlComment(lines[next]);
    if (line.trim() === "") {
      continue;
    }
    if (/^\s*/.exec(line)[0].length <= indentWidth) {
      break;
    }
    if (!/^\s*-\s+/.test(line)) {
      everyLineIsListItem = false;
    }
    collected.push(line.trim().replace(/^-\s*/, ""));
  }

  // A block list is the same value as its flow form, so join it the way the
  // flow form is written. Joining list entries with a space instead would hide
  // a second label inside what then reads as one unrecognised name.
  return collected.join(everyLineIsListItem ? ", " : " ");
}

/**
 * The complete `runs-on:` value at `index`, folded onto one line, or null when
 * that line is not a `runs-on:`.
 */
export function foldRunsOnValue(lines, index) {
  const match = /^(\s*)runs-on:\s*(.*)$/.exec(stripYamlComment(lines[index] ?? ""));
  if (!match) {
    return null;
  }

  const [, indent, inlineValue] = match;
  const value = inlineValue.trim();
  if (value === "" || value === ">-" || value === ">" || value === "|") {
    return collectRunsOnValue(lines, index, indent.length);
  }
  return value;
}

/**
 * Split `text` on a top-level `separator`, ignoring occurrences inside quotes
 * or parentheses.
 */
function splitTopLevel(text, separator) {
  const parts = [];
  let depth = 0;
  let quote = null;
  let start = 0;

  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];

    if (quote) {
      if (character === quote) {
        quote = null;
      }
      continue;
    }
    if (character === '"' || character === "'") {
      quote = character;
      continue;
    }
    if (character === "(") {
      depth += 1;
      continue;
    }
    if (character === ")") {
      depth -= 1;
      continue;
    }
    if (depth === 0 && text.startsWith(separator, index)) {
      parts.push(text.slice(start, index));
      index += separator.length - 1;
      start = index + 1;
    }
  }

  parts.push(text.slice(start));
  return parts;
}

/** The text of a single-quoted or double-quoted scalar, or null. */
function quotedLiteral(text) {
  const match = /^'([^']*)'$/.exec(text.trim()) ?? /^"([^"]*)"$/.exec(text.trim());
  return match ? match[1] : null;
}

/**
 * What a `runs-on` value selects, or null when it selects something this file
 * cannot see through - a repository variable, a matrix key, anything indirect.
 *
 * `kind` distinguishes the two ways a value names more than one label, because
 * GitHub reads them oppositely. An expression picks exactly one of its
 * branches, so several are fine as long as each is allowed. A label list is a
 * conjunction: the runner must carry *every* label in it.
 *
 * An expression is read by its *value* positions rather than by every literal
 * in it, which is what lets the packaging workflow pick its image from the
 * architecture input and still be checked. GitHub's idiom is
 * `condition && valueA || valueB`, so each `||` alternative contributes the
 * last of its `&&` terms, and every one of those must be a quoted literal.
 * Reading literals anywhere in the expression instead would let one allowed
 * name vouch for an indirect operand beside it: `'ubuntu-latest' ||
 * vars.PACKAGE_RUNNER` would pass while resolving to whatever that variable
 * holds. Only the condition may name inputs, because it selects between values
 * rather than being one.
 *
 * The expression must also be the entire value. `prefix-${{ 'windows-2025' }}`
 * resolves to `prefix-windows-2025`, so reading only the expression body would
 * report an allowed label for a job that asks for one nobody approved.
 */
export function runnerSelection(value) {
  const trimmed = value.trim();

  if (trimmed.includes("${{")) {
    if (!trimmed.startsWith("${{") || !trimmed.endsWith("}}")) {
      return null;
    }
    const body = trimmed.slice(3, -2);
    if (body.includes("${{")) {
      return null;
    }

    const labels = [];
    for (const alternative of splitTopLevel(body, "||")) {
      const terms = splitTopLevel(alternative, "&&");
      const literal = quotedLiteral(terms[terms.length - 1]);
      if (literal === null) {
        return null;
      }
      labels.push(literal);
    }
    return labels.length > 0 ? { kind: "alternatives", labels } : null;
  }

  const labels = trimmed
    .replace(/^\[|\]$/g, "")
    .split(",")
    .map((entry) => entry.replaceAll('"', "").replaceAll("'", "").trim())
    .filter((entry) => entry !== "");
  return labels.length > 0 ? { kind: "labels", labels } : null;
}

function findRunsOnViolations(relativePath, lines) {
  const violations = [];

  for (let index = 0; index < lines.length; index += 1) {
    const value = foldRunsOnValue(lines, index);
    if (value === null) {
      continue;
    }

    const selection = runnerSelection(value);
    if (selection === null) {
      violations.push(
        `${relativePath}:${index + 1}: runs-on '${value}' does not name a runner ` +
          `directly. An indirect value such as a repository variable, a matrix ` +
          `key, or an expression inside a larger string can resolve to any ` +
          `runner, including a self-hosted one.`,
      );
      continue;
    }

    // A hosted image carries one label, so a job asking for two waits for a
    // runner carrying both and never gets one. That queues forever, which is
    // the failure this policy exists to prevent.
    if (selection.kind === "labels" && selection.labels.length !== 1) {
      violations.push(
        `${relativePath}:${index + 1}: runs-on '${value}' lists ` +
          `${selection.labels.length} labels. GitHub requires a runner carrying ` +
          `every label in the list, so name exactly one hosted runner.`,
      );
    }

    for (const label of selection.labels) {
      if (ALLOWED_RUNNERS.has(label)) {
        continue;
      }
      violations.push(
        `${relativePath}:${index + 1}: runs-on '${value}' selects '${label}', ` +
          `which is not an allowed GitHub-hosted runner ` +
          `(${[...ALLOWED_RUNNERS].join(", ")}).`,
      );
    }
  }

  return violations;
}

export function findRunnerPolicyViolations(relativePath, contents) {
  const lines = contents.split(/\r?\n/);
  return [
    ...findForbiddenRunners(relativePath, lines),
    ...findRunsOnViolations(relativePath, lines),
  ];
}
