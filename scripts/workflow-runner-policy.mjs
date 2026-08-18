// Runner policy rules for .github/workflows, kept separate from the CLI in
// check-workflow-runners.mjs so they can be tested against crafted workflows.
//
// The failure this prevents is quiet: someone adds a job, reaches for
// `windows-latest` out of habit or as a fallback for when the self-hosted host
// is offline, and the repository resumes spending paid Windows minutes with
// nothing visibly broken. Queuing is the designed behavior when no runner is
// online, so there is no legitimate hosted-Windows fallback to allow.

// Hosted runners that bill above the Linux rate. macOS is included because it is
// the same mistake at ten times the multiplier.
const FORBIDDEN_RUNNER = /\b(windows-latest|windows-\d{4}|windows-11-arm|macos-[\w.-]+)\b/g;

// Linux hosted runners stay allowed: three release jobs use them deliberately,
// to keep RELEASE_MIRROR_TOKEN and release-write permission off a long-lived
// host that also executes contributor pull request code.
const ALLOWED_HOSTED = new Set(["ubuntu-latest", "ubuntu-24.04", "ubuntu-22.04"]);

/**
 * Removes a YAML end-of-line comment.
 *
 * This is load-bearing rather than cosmetic. Matching runner labels against raw
 * line text lets a comment vouch for the code beside it: `runs-on: self-hosted
 * # meowcal-ci` would satisfy a naive "does the line mention meowcal-" test
 * while actually selecting any self-hosted runner, and
 * `runs-on: ${{ vars.RUNNER }} # self-hosted meowcal-ci` would pass while
 * resolving to whatever that variable holds. It also stops a comment that merely
 * *names* `windows-latest`, as several of these workflows do when explaining the
 * policy, from being read as a use of it.
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

function findForbiddenRunners(relativePath, lines) {
  const violations = [];

  for (const [index, text] of lines.entries()) {
    for (const match of stripYamlComment(text).matchAll(FORBIDDEN_RUNNER)) {
      violations.push(
        `${relativePath}:${index + 1}: '${match[0]}' is a paid hosted runner. ` +
          `Use a meowcal-* self-hosted label; jobs queue when no runner is online.`,
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
    collected.push(line.trim());
  }

  return collected.join(" ");
}

/**
 * The complete `runs-on:` value at `index`, folded onto one line, or null when
 * that line is not a `runs-on:`.
 *
 * Exported because a second reader needs the same answer: action-cache-plan.mjs
 * decides which jobs execute on the owner's own hosts, and two parsers
 * disagreeing about that would surface as a job quietly missing the action
 * archive cache rather than as a failure.
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

function findRunsOnViolations(relativePath, lines) {
  const violations = [];

  for (let index = 0; index < lines.length; index += 1) {
    const value = foldRunsOnValue(lines, index);
    if (value === null) {
      continue;
    }

    const bare = value
      .replace(/^\[|\]$/g, "")
      .replaceAll('"', "")
      .replaceAll("'", "")
      .trim();
    const isAllowedHosted = ALLOWED_HOSTED.has(bare);
    const isSelfHosted = /self-hosted/.test(value);

    if (!isAllowedHosted && !isSelfHosted) {
      violations.push(
        `${relativePath}:${index + 1}: runs-on '${value}' is neither an allowed ` +
          `Linux hosted runner nor a self-hosted label set. An indirect value such as ` +
          `a repository variable can resolve to a paid runner and is not allowed.`,
      );
    }

    if (isSelfHosted && !/meowcal-/.test(value)) {
      violations.push(
        `${relativePath}:${index + 1}: runs-on '${value}' uses the bare self-hosted ` +
          `label. Name a meowcal-* label so the job cannot land on an unintended runner.`,
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
