# Microsoft Store listing

[English copy](en-US.md) · [简体中文文案](zh-CN.md)

These paired files own the Store description, features, update summary, and
screenshot captions for product `9NNK2X23VLWT`. Keep them aligned with the latest
published app and update both languages together. Use concise product copy;
keep legal model notices intact and describe setup requirements once.

## Screenshots

Run `node scripts/generate-store-screenshots.mjs` on Windows after `npm ci`.
The default browser is Edge; `MEOWCAL_BROWSER_CHANNEL` can select Chrome.
Outputs in [`../assets/store/`](../assets/store/) are 2560 × 1440 PNGs, numbered
in Store display order: overlay, Home, selection, subtitle style, Settings.
Upload `en-*` to English (United States) and `zh-CN-*` to Chinese (Simplified).

The generator renders current app components with deterministic sample settings
and events. It preserves app dimensions and shows the Store update controls.
It does not alter the app or an installed profile. These are frontend captures,
not native capture/OCR/translation test evidence. `capture.json` records the
version and sample text. `coast.webp` is an original imagegen-created example
film still. The authored sample translates Chinese to English for the English
listing, and English to Chinese for the Chinese listing. UI, selection, and
overlay controls come from application code.

Check each language visually after regeneration. Keep text sharp, retain the
captured aspect ratio, and avoid additional marketing copy or logos inside
screenshots. The [Microsoft screenshot requirements](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/screenshots-and-images)
require PNG and at least 1366 × 768 pixels for desktop. Captions are separate
Store fields, limited to 200 characters.

## Publication

In Partner Center, start a listing update from the published submission. Update
each language's text, screenshots, and captions, then inspect the saved preview
before submitting. Retain packages, pricing, markets, and age ratings. Do not
edit a pending API-created draft in the browser; wait for it to finish first.
Use the existing [Store inventory workflow](../STORE_SUBMISSION_SETUP.md) to
confirm package versions and certification state.

Package release automation preserves published listing copy and screenshots.
A listing-only edit does not need a new application version or package build.
