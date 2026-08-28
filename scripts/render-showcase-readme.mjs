#!/usr/bin/env node
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';

const MIRROR = 'PeterShanxin/Meowcal-Sub-releases';

function parseArgs(argv) {
  const args = { version: null, output: 'showcase-out/README.md' };
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--version' && argv[i + 1]) args.version = argv[++i];
    else if (argv[i] === '--output' && argv[i + 1]) args.output = argv[++i];
  }
  return args;
}

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function readVersionFromTauri() {
  return readJson('src-tauri/tauri.conf.json').version;
}

function isPrerelease(version) {
  return /(?:^|[.-])(alpha|beta|rc)(?:[.-]|$)/i.test(version);
}

function releaseLinks(version) {
  const tag = version.startsWith('v') ? version : `v${version}`;
  const base = `https://github.com/${MIRROR}`;
  const prerelease = isPrerelease(version);
  return {
    download: prerelease ? `${base}/releases/tag/${tag}` : `${base}/releases/latest`,
    releases: `${base}/releases`,
    releaseNotes: `${base}/releases/tag/${tag}`,
    versionBadge: prerelease
      ? `https://img.shields.io/github/v/release/${MIRROR}?include_prereleases&label=${encodeURIComponent(tag)}&color=orange`
      : `https://img.shields.io/github/v/release/${MIRROR}?label=latest`,
  };
}

function bulletList(items) {
  return items.map((item) => `- ${item}`).join('\n');
}

function joinPhrases(items) {
  return items.join('; ');
}

function renderFeaturesTable(features) {
  const rows = features.map((f) => `| **${f.title}** | ${f.description} |`);
  return ['| Feature | Description |', '| --- | --- |', ...rows].join('\n');
}

function renderBadges(links) {
  return [
    `[![Release](${links.versionBadge})](${links.releases})`,
    '![Windows 11](https://img.shields.io/badge/platform-Windows%2011-0078D6?logo=windows)',
    '![Architectures](https://img.shields.io/badge/arch-x64%20%7C%20ARM64-blue)',
    '![Local AI](https://img.shields.io/badge/AI-on--device-22c55e)',
    '![Tauri](https://img.shields.io/badge/stack-Tauri%20%2B%20Rust-ffc131)',
  ].join('\n');
}

function renderBenchmarks(benchmarks) {
  if (!benchmarks.entries?.length) {
    return '_No public benchmark summary published yet._';
  }

  const summary =
    'On Windows ARM64, local translation reached **~660 ms median latency** in our development evaluation, with a hardware-gated GPU path and automatic CPU fallback.';

  const lines = [
    summary,
    '',
    '<details>',
    '<summary>Technical benchmark details</summary>',
    '',
    benchmarks.disclaimer,
    '',
  ];

  for (const entry of benchmarks.entries) {
    lines.push(`### ${entry.title}`, '', `**Environment:** ${entry.environment}`, '');
    if (entry.metrics?.length) {
      lines.push('| Metric | Value |', '| --- | --- |');
      for (const m of entry.metrics) lines.push(`| ${m.name} | ${m.value} |`);
      lines.push('');
    }
    if (entry.context) lines.push(entry.context, '');
    if (entry.comparison) lines.push(entry.comparison, '');
  }

  lines.push('</details>');
  return lines.join('\n').trim();
}

function main() {
  const args = parseArgs(process.argv);
  const showcase = readJson('showcase/showcase.json');
  const benchmarks = readJson('showcase/benchmarks.json');
  const template = readFileSync('showcase/README.template.md', 'utf8');
  const version = args.version ?? readVersionFromTauri();
  const links = releaseLinks(version);

  const replacements = {
    '{{PRODUCT_NAME}}': showcase.product.name,
    '{{TAGLINE}}': showcase.product.tagline,
    '{{VERSION}}': version,
    '{{STATUS}}': showcase.product.status,
    '{{BADGES_ROW}}': renderBadges(links),
    '{{DOWNLOAD_URL}}': links.download,
    '{{RELEASES_URL}}': links.releases,
    '{{RELEASE_NOTES_URL}}': links.releaseNotes,
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
  for (const [key, value] of Object.entries(replacements)) {
    output = output.split(key).join(value);
  }

  mkdirSync(dirname(args.output), { recursive: true });
  writeFileSync(args.output, output, 'utf8');
  console.log(`Wrote ${args.output} for v${version}`);
}

main();
