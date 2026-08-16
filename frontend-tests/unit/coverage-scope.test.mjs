// Keeps the coverage claim honest about what it measures.
//
// The failure this prevents is the comfortable one: a module gains tests, its
// percentage looks fine, and nobody notices it was never in the measured set -
// so the headline number stays high precisely because the risky code is outside
// it. Here the two lists are compared directly, so exercising a module without
// measuring it fails the suite (#35).
//
// It also works the other way: a file listed in the scope that no test touches
// is reported, because with this provider it would silently contribute nothing.

import { existsSync, readFileSync, readdirSync } from "node:fs";
import { describe, expect, it } from "vitest";

const repositoryRoot = new URL("../../", import.meta.url);
const unitTestDirectory = new URL("./", import.meta.url);

const baseline = JSON.parse(
  readFileSync(new URL("config/maintainability-baseline.json", repositoryRoot), "utf8"),
);
const coverageScope = baseline.frontendCoverageScope;

// Modules a test names, whether by static import, dynamic import, or
// createRequire. The extension is optional because TypeScript imports are
// written without one; `resolveModule` puts it back. Only executable sources
// count: several tests read HTML and CSS as text to assert on markup, and there
// is nothing to instrument in those.
const MODULE_REFERENCE = /["'`](?:\.\.\/)+((?:src|scripts)\/[\w./-]+?)(?:\.(?:m?js|ts))?["'`]/g;
const EXTENSIONS = [".ts", ".js", ".mjs"];

// Named by a test but not executed by one, with the reason. Every entry is a
// claim a reviewer can check, which is the point: the way to defeat this check
// is to add a line here, and that line is visible in the diff.
const NOT_MEASURABLE = new Map([
  // Types only. It compiles to nothing, so instrumenting it would report an
  // empty file rather than an uncovered one.
  ["src/ui/contracts.ts", "types only"],
  // Read as source text by entry-graph.test.mjs and shell-stylesheets.test.mjs,
  // which assert on what the entries wire up rather than running them. They pull
  // in Lit and the DOM, so the node unit environment cannot execute them; the
  // browser smoke and the manual Windows gate cover them instead.
  ["src/entries/main.ts", "asserted as text; needs a DOM to run"],
  ["src/entries/wizard.ts", "asserted as text; needs a DOM to run"],
  ["src/ui/meowcal-titlebar.ts", "asserted as text; needs a DOM to run"],
]);

/**
 * The repository file a reference names, or null when nothing is there.
 *
 * A reference written with an extension is taken as it stands; one without gets
 * the extensions a bundler would try. Anything that resolves to no file is not a
 * module reference at all - a fixture path, or a string that happens to look
 * like one.
 */
function resolveModule(reference) {
  const candidates = /\.(m?js|ts)$/.test(reference)
    ? [reference]
    : EXTENSIONS.map((extension) => `${reference}${extension}`);

  return candidates.find((candidate) => existsSync(new URL(candidate, repositoryRoot))) ?? null;
}

function unitTestFiles(directory = unitTestDirectory) {
  // Recursive, because vitest's include is recursive: a test in a subdirectory
  // runs, and a scanner that only reads the top level would let the module it
  // exercises stay outside the measured scope unnoticed.
  const files = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const child = new URL(entry.name + (entry.isDirectory() ? "/" : ""), directory);
    if (entry.isDirectory()) {
      files.push(...unitTestFiles(child));
    } else if (/\.test\.(mjs|ts)$/.test(entry.name)) {
      files.push(child);
    }
  }
  return files;
}

function directReferences(source) {
  const found = new Set();
  for (const match of source.matchAll(MODULE_REFERENCE)) {
    const resolved = resolveModule(match[1]);
    if (resolved) {
      found.add(resolved);
    }
  }
  return found;
}

// Relative imports inside a production module. A module a test never names is
// still exercised when a module under test imports it, and its uncovered lines
// belong in the denominator just the same.
const RELATIVE_IMPORT =
  /(?:from|import|require)\s*\(?\s*["'`](\.[\w./-]+?)(?:\.(?:m?js|ts))?["'`]/g;

function transitiveImports(module, seen) {
  const source = readFileSync(new URL(module, repositoryRoot), "utf8");
  const directory = module.slice(0, module.lastIndexOf("/") + 1);

  for (const match of source.matchAll(RELATIVE_IMPORT)) {
    const joined = new URL(match[1], new URL(directory, repositoryRoot));
    const relative = decodeURIComponent(joined.href.slice(repositoryRoot.href.length));
    const resolved = resolveModule(relative);
    if (resolved && !seen.has(resolved)) {
      seen.add(resolved);
      transitiveImports(resolved, seen);
    }
  }
}

function referencedModules() {
  const referenced = new Set();

  for (const file of unitTestFiles()) {
    for (const module of directReferences(readFileSync(file, "utf8"))) {
      referenced.add(module);
    }
  }

  // Follow what those modules import, so a module reached only through another
  // is measured too.
  for (const module of [...referenced]) {
    if (!NOT_MEASURABLE.has(module)) {
      transitiveImports(module, referenced);
    }
  }

  return referenced;
}

describe("frontend coverage scope", () => {
  it("measures every module the unit suite exercises", () => {
    const missing = [...referencedModules()]
      .filter((module) => !NOT_MEASURABLE.has(module))
      .filter((module) => !coverageScope.includes(module))
      .sort();

    expect(
      missing,
      "these modules are exercised by unit tests but are outside the measured " +
        "coverage scope in the baseline manifest; add them there rather than leaving " +
        "the percentage flattering",
    ).toEqual([]);
  });

  it("does not claim scope over files that exist only in the list", () => {
    const referenced = referencedModules();
    const unexercised = coverageScope.filter((module) => !referenced.has(module)).sort();

    expect(
      unexercised,
      "these files are in the measured scope but no unit test names them; either " +
        "test them or drop them from the scope",
    ).toEqual([]);
  });

  it("names every listed file with a repository-relative path", () => {
    for (const module of coverageScope) {
      expect(module, `${module} should be repository-relative`).toMatch(/^(?:src|scripts)\//);
      expect(module.includes("\\"), `${module} should use forward slashes`).toBe(false);
    }
  });
});

// Guards the guard: if the pattern stops matching the way tests actually name
// modules, both assertions above pass vacuously.
describe("the scope check can see module references", () => {
  it("finds the modules this repository's tests name", () => {
    const referenced = referencedModules();
    expect(referenced.size).toBeGreaterThan(15);
    expect(referenced).toContain("src/scripts/overlay-geometry.js");
    expect(referenced).toContain("src/ui/home-state.ts");
  });

  it("matches static, dynamic, and createRequire forms", () => {
    const samples = [
      'import { x } from "../../src/ui/home-state";',
      'await import("../../src/scripts/overlay-liveness.js");',
      'require("../../src/scripts/overlay-geometry.js")',
      'const p = "../../../scripts/doc-links.mjs";',
    ];
    const found = samples.flatMap((sample) =>
      [...sample.matchAll(MODULE_REFERENCE)].map((match) => match[1]),
    );

    // Captured without the extension, which `resolveModule` restores.
    expect(found).toEqual([
      "src/ui/home-state",
      "src/scripts/overlay-liveness",
      "src/scripts/overlay-geometry",
      "scripts/doc-links",
    ]);
  });

  it("follows imports through a module under test", () => {
    // app-controller.ts imports these; no unit test names update-controller
    // directly through a path, so a scan of test sources alone would leave it
    // out of the denominator.
    const seen = new Set(["src/ui/app-controller.ts"]);
    transitiveImports("src/ui/app-controller.ts", seen);

    expect(seen).toContain("src/ui/update-controller.ts");
    expect(seen).toContain("src/ui/languages.ts");
  });

  it("reads the same repository the suite runs against", () => {
    expect(readFileSync(new URL("package.json", repositoryRoot), "utf8")).toContain(
      '"name": "meowcal-sub"',
    );
  });
});
