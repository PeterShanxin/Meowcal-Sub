#!/usr/bin/env node
/**
 * Validates a generated showcase bundle against an explicit output allowlist.
 * SECURITY: Reject unexpected files, symlinks, and path traversal.
 */
import { lstatSync, readdirSync, readFileSync } from 'node:fs';
import { join, posix, relative, resolve } from 'node:path';

function normalizeRel(p) {
  return posix.normalize(p.replace(/\\/g, '/')).replace(/^\.\//, '');
}

function listFilesRecursive(root) {
  const absRoot = resolve(root);
  const results = [];

  function walk(dir) {
    for (const name of readdirSync(dir)) {
      const abs = join(dir, name);
      const st = lstatSync(abs);
      if (st.isSymbolicLink()) {
        throw new Error(`Symlink not allowed in output: ${relative(absRoot, abs)}`);
      }
      if (st.isDirectory()) {
        walk(abs);
      } else if (st.isFile()) {
        const rel = normalizeRel(relative(absRoot, abs));
        if (rel.includes('..') || posix.isAbsolute(rel)) {
          throw new Error(`Path traversal rejected: ${rel}`);
        }
        if (!resolve(abs).startsWith(absRoot)) {
          throw new Error(`Output escaped root: ${rel}`);
        }
        results.push(rel);
      }
    }
  }

  walk(absRoot);
  return results.sort();
}

function isAllowed(rel, allowed) {
  if (allowed.has(rel)) return true;
  for (const pattern of allowed) {
    if (pattern.endsWith('/**')) {
      const prefix = pattern.slice(0, -3);
      if (rel === prefix || rel.startsWith(`${prefix}/`)) return true;
    }
  }
  return false;
}

export function validateShowcaseOutput(outDir, outputConfig) {
  const allowed = new Set(outputConfig.outputPaths.map(normalizeRel));
  const found = listFilesRecursive(outDir);
  const errors = [];

  for (const rel of found) {
    if (!isAllowed(rel, allowed)) errors.push(`Unexpected output file: ${rel}`);
  }
  for (const required of outputConfig.requiredPaths ?? []) {
    const norm = normalizeRel(required);
    if (!found.includes(norm)) errors.push(`Missing required output file: ${norm}`);
  }

  if (errors.length) {
    throw new Error(
      `Showcase output validation failed:\n${errors.map((e) => `  - ${e}`).join('\n')}`,
    );
  }
  console.log(`Validated ${found.length} output file(s) in ${outDir}`);
}

function main() {
  const outDir = process.argv[2] ?? 'showcase-out';
  const config = JSON.parse(readFileSync('showcase/EXPORT_ALLOWLIST.json', 'utf8'));
  validateShowcaseOutput(outDir, config.output);
}

if (import.meta.url === new URL(`file://${process.argv[1]}`).href) main();
