#!/usr/bin/env node
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';

function parseArgs(argv) {
  const args = { version: null, output: 'showcase-out/README.md' };
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--version' && argv[i + 1]) args.version = argv[++i];
    else if (argv[i] === '--output' && argv[i + 1]) args.output = argv[++i];
  }
  return args;
}

function readJson(path) { return JSON.parse(readFileSync(path, 'utf8')); }
function readVersionFromTauri() { return readJson('src-tauri/tauri.conf.json').version; }
function bulletList(items) { return items.map((item) => `- ${item}`).join('\n'); }
function joinPhrases(items) { return items.join('; '); }

function renderFeaturesTable(features) {
  const rows = features.map((f) => `| **${f.title}** | ${f.description} |`);
  return ['| Feature | Description |', '| --- | --- |', ...rows].join('\n');
}

function renderBenchmarks(benchmarks) {
  if (!benchmarks.entries?.length) return '_No public benchmark summary published yet._';
  const parts = [benchmarks.disclaimer, ''];
  for (const entry of benchmarks.entries) {
    parts.push(`### ${entry.title}`);
    parts.push(`**Environment:** ${entry.environment}`);
    if (entry.metrics?.length) {
      parts.push('', '| Metric | Value |', '| --- | --- |');
      for (const m of entry.metrics) parts.push(`| ${m.name} | ${m.value} |`);
    }
    if (entry.context) parts.push('', entry.context);
    if (entry.comparison) parts.push('', entry.comparison);
    parts.push('');
  }
  return parts.join('\n').trim();
}

function main() {
  const args = parseArgs(process.argv);
  const showcase = readJson('showcase/showcase.json');
  const benchmarks = readJson('showcase/benchmarks.json');
  const template = readFileSync('showcase/README.template.md', 'utf8');
  const version = args.version ?? readVersionFromTauri();
  const mirror = 'PeterShanxin/Meowcal-Sub-releases';
  const tag = `v${version}`;
  const replacements = {
    '{{PRODUCT_NAME}}': showcase.product.name,
    '{{TAGLINE}}': showcase.product.tagline,
    '{{VERSION}}': version,
    '{{STATUS}}': showcase.product.status,
    '{{DOWNLOAD_URL}}': `https://github.com/${mirror}/releases/latest`,
    '{{RELEASES_URL}}': `https://github.com/${mirror}/releases`,
    '{{RELEASE_NOTES_URL}}': `https://github.com/${mirror}/releases/tag/${tag}`,
    '{{FEATURES_TABLE}}': renderFeaturesTable(showcase.features),
    '{{ENGINEERING_LIST}}': bulletList(showcase.engineering),
    '{{BENCHMARKS_SECTION}}': renderBenchmarks(benchmarks),
    '{{PRIVACY_LOCAL}}': joinPhrases(showcase.privacy.local),
    '{{PRIVACY_NETWORK}}': joinPhrases(showcase.privacy.network),
    '{{PRIVACY_NOT_SENT}}': joinPhrases(showcase.privacy.notSent),
    '{{REQUIREMENTS_OS}}': showcase.requirements.os,
    '{{REQUIREMENTS_DISK}}': showcase.requirements.disk,
    '{{REQUIREMENTS_NOTES}}': showcase.requirements.notes.map((n) => `- ${n}`).join('\n'),
    '{{LICENSE_SUMMARY}}': showcase.license.summary,
  };
  let output = template;
  for (const [key, value] of Object.entries(replacements)) output = output.split(key).join(value);
  mkdirSync(dirname(args.output), { recursive: true });
  writeFileSync(args.output, output, 'utf8');
  console.log(`Wrote ${args.output} for ${tag}`);
}

main();
