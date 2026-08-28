#!/usr/bin/env node
/**
 * Allowlist-first export of public-safe showcase files.
 * SECURITY: Only paths in showcase/EXPORT_ALLOWLIST.json may be copied.
 */
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';

function parseArgs(argv) {
  const args = { version: null, outDir: 'showcase-out' };
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--version' && argv[i + 1]) args.version = argv[++i];
    else if (argv[i] === '--out-dir' && argv[i + 1]) args.outDir = argv[++i];
  }
  return args;
}

function readJson(path) { return JSON.parse(readFileSync(path, 'utf8')); }

function ensureIconFallback() {
  const iconDest = 'showcase/assets/icon.png';
  if (existsSync(iconDest)) return;
  const source = 'src-tauri/icons/icon.png';
  if (!existsSync(source)) throw new Error(`Missing ${iconDest} and fallback ${source}`);
  mkdirSync(dirname(iconDest), { recursive: true });
  copyFileSync(source, iconDest);
}

function ensureHeroFallback() {
  const hero = 'showcase/assets/hero.png';
  if (existsSync(hero)) return;
  ensureIconFallback();
  copyFileSync('showcase/assets/icon.png', hero);
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
  ensureHeroFallback();
  if (existsSync(args.outDir)) rmSync(args.outDir, { recursive: true, force: true });
  mkdirSync(args.outDir, { recursive: true });
  const renderArgs = ['scripts/render-showcase-readme.mjs', '--output', join(args.outDir, 'README.md')];
  if (args.version) renderArgs.push('--version', args.version);
  const render = spawnSync(process.execPath, renderArgs, { stdio: 'inherit' });
  if (render.status !== 0) process.exit(render.status ?? 1);
  copyAllowlisted(args.outDir, allowlist);
  copyFileSync('showcase/showcase.json', join(args.outDir, 'showcase.json'));
  copyFileSync('showcase/benchmarks.json', join(args.outDir, 'benchmarks.json'));
  console.log(`Showcase bundle ready in ${args.outDir}`);
}

main();
