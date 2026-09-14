# App icon source

`icon.ico`, `32x32.png`, `128x128.png`, and `128x128@2x.png` — the icons
Windows embeds for the taskbar, tray, and installer — are generated from the
vector mark in [`docs/assets/logo.svg`](../../docs/assets/logo.svg), not from
`src/assets/meowcal-icon.png`.

To regenerate:

```sh
npx resvg-cli --fit-width 1024 docs/assets/logo.svg /tmp/logo-1024.png
npx tauri icon /tmp/logo-1024.png -o src-tauri/icons
git checkout -- src-tauri/icons/icon.icns src-tauri/icons/icon.png \
  src-tauri/icons/64x64.png "src-tauri/icons/Square*.png" \
  src-tauri/icons/StoreLogo.png src-tauri/icons/android src-tauri/icons/ios
```

The last step reverts the platform icon sets (macOS, Android, iOS, Appx) that
`tauri icon` also regenerates but this project doesn't bundle — only the four
files above are referenced by `tauri.conf.json`'s `bundle.icon`.
