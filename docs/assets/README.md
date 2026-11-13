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
- `hero.svg` and `hero.zh-CN.svg` pair the English and Chinese frontend with
  matching brand copy. Run `node scripts/generate-readme-hero.mjs` to regenerate
  both heroes and `screenshot-home.png` / `screenshot-home.zh-CN.png`. It serves
  the current app components and styles through Vite, with an in-memory ready
  state and the production window title. It does not redraw the UI or edit pixels.
  Home is captured at the configured 680 × 500 viewport with 3× pixel density;
  each 1600 × 720 hero has a 2× PNG export (3200 × 1440) for sharing. The READMEs
  load the smaller SVG: vector background, logo and copy, with a self-contained
  lossless WebP screenshot. The generator verifies that screenshot compression
  preserves every pixel. SVG text positions come from the browser layout;
  `.local/hero*-svg.png` previews verify rendering in an image element.
  Use Windows with Segoe UI and Microsoft YaHei; set
  `MEOWCAL_BROWSER_CHANNEL=msedge` to use Edge.

Keep source screenshots at their captured aspect ratio. These browser-rendered
images document the frontend, not native behavior. Native validation still needs
separate Windows app evidence with its commit and capture conditions recorded
in the PR. Mark future unreleased interface previews explicitly in the README.

Do not use a cat outline, whiskers, ears, or a face as the application mark.
The offset cards, rounded corners, and restrained lavender accent carry its
friendly character. Keep adequate clear space; do not place other text within
half a card's height of the mark.

The [name and logo usage terms](../../TRADEMARKS.md) apply to these assets.
