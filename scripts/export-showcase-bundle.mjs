#!/usr/bin/env node
/**
 * Allowlist-first export of public-safe showcase files.
 * SECURITY: Only paths in showcase/EXPORT_ALLOWLIST.json may be copied.
 * README is rendered after assets are copied so image tags are never emitted
 * for files that are not in the public bundle.
 */
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { validateShowcaseOutput } from './validate-showcase-output.mjs';

function parseArgs(argv) {
  const args = { version: null, outDir: 'showcase-out' };
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--version' && argv[i + 1]) args.version = argv[++i];
    else if (argv[i] === '--out-dir' && argv[i + 1]) args.outDir = argv[++i];
  }
  return args;
}

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function ensureAsset(dest, ...sources) {
  if (existsSync(dest)) return;
  for (const src of sources) {
    if (existsSync(src)) {
      mkdirSync(dirname(dest), { recursive: true });
      copyFileSync(src, dest);
      return;
    }
  }
}

function prepareAssets() {
  ensureAsset('showcase/assets/logo.png', 'showcase/assets/icon.png', 'src-tauri/icons/icon.png');
  ensureAsset('showcase/assets/icon.png', 'src-tauri/icons/icon.png');
}

function copyAllowlisted(outDir, allowlist) {
  const assetDir = join(outDir, 'assets');
  mkdirSync(assetDir, { recursive: true });
  for (const rel of allowlist.paths) {
    if (!rel.startsWith('showcase/assets/')) continue;
    if (!existsSync(rel)) continue;
    const name = rel.split('/').pop();
    copyFileSync(rel, join(assetDir, name));
  }
}

function main() {
  const args = parseArgs(process.argv);
  const allowlist = readJson('showcase/EXPORT_ALLOWLIST.json');
  prepareAssets();

  if (existsSync(args.outDir)) rmSync(args.outDir, { recursive: true, force: true });
  mkdirSync(args.outDir, { recursive: true });
  copyAllowlisted(args.outDir, allowlist);

  const renderArgs = [
    'scripts/render-showcase-readme.mjs',
    '--output',
    join(args.outDir, 'README.md'),
    '--assets-dir',
    args.outDir,
  ];
  if (args.version) renderArgs.push('--version', args.version);
  const render = spawnSync(process.execPath, renderArgs, { stdio: 'inherit' });
  if (render.status !== 0) process.exit(render.status ?? 1);

  validateShowcaseOutput(args.outDir, allowlist.output);
  console.log(`Showcase bundle ready in ${args.outDir}`);
}

main();
