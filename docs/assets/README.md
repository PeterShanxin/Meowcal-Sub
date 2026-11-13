# Brand assets

The mark is a pair of rounded subtitle cards: the outlined front card represents
recognized text, and the lavender card represents its translation. Use the full
name **Meowcal Sub** in English and **Meowcal Sub · 妙影** in Simplified Chinese.
Keep the existing neutral dark UI; lavender is a small brand accent.

- `logo.svg` is the canonical application-icon source.
- `mark.svg` is the transparent standalone mark.
- `wordmark.svg` and `wordmark.zh-CN.svg` are the English and Chinese lockups on
  dark backgrounds. The text remains editable; use Segoe UI and Microsoft YaHei.
- `logo.png`, `src/assets/meowcal-icon.png`, and the packaged icons come from
  `logo.svg`. Run `node scripts/generate-brand-assets.mjs` at the repository root
  after `npm ci`. This uses the repository's pinned Tauri CLI. The social preview
  uses Chrome; set `MEOWCAL_BROWSER_CHANNEL=msedge` to use Edge.
- `hero.png` and `hero.zh-CN.png` place real application screenshots beside the
  brand copy. Run `node scripts/generate-readme-hero.mjs` after updating
  `screenshot-home.png` and `screenshot-home.zh-CN.png`. The generator composes
  the images without recreating or changing the application controls.

Keep source screenshots at their captured aspect ratio. Capture the current
native development app in each language, using non-private example inputs.
Record the commit and capture conditions with the PR's native evidence. A
browser screenshot can document the frontend, but cannot prove native behavior.
The README identifies unreleased interface screenshots as development previews.

Do not use a cat outline, whiskers, ears, or a face as the application mark.
The offset cards, rounded corners, and restrained lavender accent carry its
friendly character. Keep adequate clear space; do not place other text within
half a card's height of the mark.

The [name and logo usage terms](../../TRADEMARKS.md) apply to these assets.
