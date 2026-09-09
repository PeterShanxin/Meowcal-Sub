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
  "ubuntu-22.04",
  "windows-11-arm",
  "windows-2025",
]);

// A token that names a runner rather than, say, an architecture input value.
// `windows-11-arm` matches; the `'arm64'` in a `runs-on` expression's condition
// does not, which is what lets the expression form below be checked at all.
const RUNNER_SHAPED = /^(?:self-hosted|ubuntu|windows|macos)[\w.-]*$/i;

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

  for (let next = index + 1; next < lines.length; next += 1) {
    const line = stripYamlComment(lines[next]);
    if (line.trim() === "") {
      continue;
    }
    if (/^\s*/.exec(line)[0].length <= indentWidth) {
      break;
    }
    collected.push(line.trim().replace(/^-\s*/, ""));
  }

  return collected.join(" ");
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
 * The runner labels a `runs-on` value can select, or null when it selects
 * something this file cannot see through - a repository variable, a matrix
 * key, anything else indirect.
 *
 * An expression is read through its quoted literals, keeping only the
 * runner-shaped ones. That is what lets the packaging workflow choose its image
 * from the architecture input and still be checked: the `'arm64'` it compares
 * against is not a runner name, and both branches are.
 */
export function runnerCandidates(value) {
  if (value.includes("${{")) {
    const literals = [...value.matchAll(/'([^']*)'|"([^"]*)"/g)]
      .map((match) => match[1] ?? match[2])
      .filter((literal) => RUNNER_SHAPED.test(literal));
    return literals.length > 0 ? literals : null;
  }

  const candidates = value
    .replace(/^\[|\]$/g, "")
    .split(",")
    .map((entry) => entry.replaceAll('"', "").replaceAll("'", "").trim())
    .filter((entry) => entry !== "");
  return candidates.length > 0 ? candidates : null;
}

function findRunsOnViolations(relativePath, lines) {
  const violations = [];

  for (let index = 0; index < lines.length; index += 1) {
    const value = foldRunsOnValue(lines, index);
    if (value === null) {
      continue;
    }

    const candidates = runnerCandidates(value);
    if (candidates === null) {
      violations.push(
        `${relativePath}:${index + 1}: runs-on '${value}' does not name a runner ` +
          `directly. An indirect value such as a repository variable or a matrix ` +
          `key can resolve to any runner, including a self-hosted one.`,
      );
      continue;
    }

    for (const candidate of candidates) {
      if (ALLOWED_RUNNERS.has(candidate)) {
        continue;
      }
      violations.push(
        `${relativePath}:${index + 1}: runs-on '${value}' selects '${candidate}', ` +
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
