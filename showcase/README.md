# Public showcase export boundary

Only files listed in `EXPORT_ALLOWLIST.json` may leave the private
`Meowcal-Sub` repository and land in the public `Meowcal-Sub-releases`
repository.

The export step is **allowlist-first**: `scripts/export-showcase-bundle.mjs`
copies named paths into a disposable `showcase-out/` directory. Nothing else
from the private tree is included.

## When the public repo updates

The public showcase README and assets refresh when `Publish Update` runs after
a release is published — the same manual gate that mirrors installers.

## What you maintain

| File | Update when |
| --- | --- |
| `showcase/showcase.json` | Product positioning, features, or compatibility changes |
| `showcase/benchmarks.json` | New verified performance evidence worth showing publicly |
| `showcase/assets/*` | New screenshots or hero art (keep filenames stable) |
| `docs/releases/v*.md` | Every release (already required by the release workflow) |

Version numbers, download links, and release notes body are injected at export
time from `src-tauri/tauri.conf.json` and the matching release notes file.
