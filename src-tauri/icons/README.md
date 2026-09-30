# App icon source

`icon.ico`, `32x32.png`, `128x128.png`, and `128x128@2x.png` — the icons
Windows embeds for the taskbar, tray, and installer — and the three MSIX logos
(`Square44x44Logo.png`, `Square150x150Logo.png`, `StoreLogo.png`) are generated from the
vector mark in [`docs/assets/logo.svg`](../../docs/assets/logo.svg), not from
`src/assets/meowcal-icon.png`.

To regenerate:

```sh
npx resvg-cli --fit-width 1024 docs/assets/logo.svg /tmp/logo-1024.png
npx tauri icon /tmp/logo-1024.png -o src-tauri/icons
git checkout -- src-tauri/icons/icon.icns src-tauri/icons/icon.png \
  src-tauri/icons/64x64.png src-tauri/icons/android src-tauri/icons/ios
```

The last step restores platforms the project does not bundle. Keep the three
MSIX assets: `build-store.ps1` copies them to the manifest's `Assets` directory.
The Windows application still embeds the four icons in `tauri.conf.json`.

The Store logos can also be rendered directly from the same SVG:

```sh
npx resvg-cli --fit-width 44 docs/assets/logo.svg src-tauri/icons/Square44x44Logo.png
npx resvg-cli --fit-width 150 docs/assets/logo.svg src-tauri/icons/Square150x150Logo.png
npx resvg-cli --fit-width 50 docs/assets/logo.svg src-tauri/icons/StoreLogo.png
npx resvg-cli --fit-width 300 docs/assets/logo.svg docs/assets/store-listing-logo-300.png
```

[`store-listing-logo-300.png`](../../docs/assets/store-listing-logo-300.png) is
the matching Partner Center listing asset. See
[Store branding](../../docs/STORE_DISTRIBUTION.md#store-branding) for the manual
listing update boundary.
