# Showcase assets

Place public-safe images here. The export step copies only files named in
`EXPORT_ALLOWLIST.json`.

| File | Purpose |
| --- | --- |
| `hero.png` | README hero — product screenshot or composed preview (1280×720 recommended) |
| `icon.png` | App icon for social cards and compact layouts |
| `architecture.svg` | High-level pipeline diagram (checked in) |

If `hero.png` is missing at export time, the exporter falls back to `icon.png`.
Copy `src-tauri/icons/icon.png` to `showcase/assets/icon.png` when refreshing art.
