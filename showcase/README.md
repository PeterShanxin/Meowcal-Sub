# Public showcase export boundary

Only files listed in `EXPORT_ALLOWLIST.json` may leave the private
`Meowcal-Sub` repository and land in the public `Meowcal-Sub-releases`
repository.

The export step is **allowlist-first**: `scripts/export-showcase-bundle.mjs`
copies named paths into a disposable `showcase-out/` directory. After
generation, output is validated against an explicit positive allowlist
(`output.outputPaths` in `EXPORT_ALLOWLIST.json`). Unexpected files, symlinks,
and path traversal are rejected.

## When the public repo updates

The public showcase README and assets refresh when `Publish Update` runs after
a release is published — the same manual gate that mirrors installers.

## What you maintain

| File | Update when |
| --- | --- |
| `showcase/showcase.json` | Product positioning, features, or compatibility changes |
| `showcase/benchmarks.json` | New verified performance evidence (stays private; rendered into README) |
| `showcase/assets/*` | New screenshots, logo, or hero art (keep filenames stable) |
| `docs/releases/v*.md` | Every release (already required by the release workflow) |

Version numbers, download links, and release notes body are injected at export
time from `src-tauri/tauri.conf.json` and the matching release notes file.

Raw `showcase.json` and `benchmarks.json` are **not** published to the public
repository.
