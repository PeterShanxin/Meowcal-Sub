# App icon source

Every icon in this directory is generated from the layered subtitle-card mark
in [`docs/assets/logo.svg`](../../docs/assets/logo.svg). The same source supplies
[`src/assets/meowcal-icon.png`](../../src/assets/meowcal-icon.png), the README logo,
and the [300 px Store listing logo](../../docs/assets/store-listing-logo-300.png).

From the repository root, after `npm ci`:

```sh
node scripts/generate-brand-assets.mjs
```

The script uses the pinned Tauri CLI. It regenerates the existing PNG sizes,
ICO, ICNS, and retained mobile assets together so none keeps an obsolete mark.
The temporary 300 px and 1024 px renders stay under ignored `.local/brand-assets`.

Windows embeds `icon.ico`, `32x32.png`, `128x128.png`, and `128x128@2x.png` for
the application, taskbar, tray, and installer. Store packaging uses
`Square44x44Logo.png`, `Square150x150Logo.png`, and `StoreLogo.png`.
See [Store branding](../../docs/STORE_DISTRIBUTION.md#store-branding) for the
separate manual Partner Center listing update.

English branding is **Meowcal Sub**; Chinese branding is **Meowcal Sub · 妙影**.
Package identifiers and executable names remain stable across interface languages.
