# Microsoft Store listing

[English copy](en-US.md) · [简体中文文案](zh-CN.md)

These paired files own the Store description, search terms, features, update
summary, and screenshot captions for product `9NNK2X23VLWT`. Keep them aligned
with the latest published app and update both languages together. Use concise
product copy, keep legal model notices intact, and describe setup requirements
once.

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

In Partner Center, open **Store presence → View product details → Store
listings** to inspect the published English (United States) and Chinese
(Simplified) listings. Start an update from that submission and apply the
changed fields for each language. Enter each search term as a separate
[Keywords](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/add-additional-information#keywords)
entry. Inspect the saved fields and listing preview before submitting. For a
keywords-only update, retain both product names, packages, pricing, markets,
and age ratings.
Wait for any pending API-created submission to finish before starting the
update. Use the [Store inventory workflow](../STORE_SUBMISSION_SETUP.md) to
confirm package versions and certification state.

Package release automation carries forward published keywords, descriptions,
and screenshots. Apply keyword edits in Partner Center even after the
repository copy is merged. A listing-only update uses the existing app version
and packages.

After publication, check the saved keywords in the published listings. Search
for `meowcal sub`, `meowcal`, and `妙影` in the Windows Microsoft Store using
the China and Singapore market settings. Record the market, Store language,
and position of product `9NNK2X23VLWT`, or how many results you checked
without finding it.
