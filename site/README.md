# Product site

The site at <https://petershanxin.github.io/Meowcal-Sub/> introduces Meowcal Sub
and Meowcal Sub 2 in English and Simplified Chinese:

| Page | English | 简体中文 |
| --- | --- | --- |
| Meowcal Sub | `/` | `/zh-CN/` |
| Meowcal Sub 2 | `/sub-2/` | `/zh-CN/sub-2/` |

It is static HTML with no JavaScript. Each page carries its own title,
description, canonical URL, language alternates, and `SoftwareApplication`
structured data; `sitemap.xml` lists the four pages.

## Where things live

- [`scripts/site/content.en.mjs`](../scripts/site/content.en.mjs) and
  [`content.zh-CN.mjs`](../scripts/site/content.zh-CN.mjs) own the copy. Keep the
  two files equivalent in meaning; a unit test compares their structure.
- [`scripts/site/render.mjs`](../scripts/site/render.mjs) owns the markup, the
  URLs, and the structured data.
- [`styles.css`](styles.css) owns the presentation.
- The cinematic hero uses `assets/cinema-light.webp`, an original generated
  decorative background. Product screenshots remain separate, unaltered images.
  The shared layout puts the product preview before the installer choices and
  works without client-side scripts or motion.
- Setup steps use alternating text and screenshots. Privacy, requirements and
  related products use open text columns without card backgrounds or borders.
  The footer contains repository and support links; license terms remain in
  the repositories.
- Sub 1 screenshots are the Store screenshots in
  [`docs/assets/store/`](../docs/assets/store/). The build converts them, so
  regenerating those screenshots updates the site.
- [`assets/`](assets/) holds what comes from the Sub 2 repository: its icon, its
  social preview, and `sub2-studio.png`.

## Build and preview

```powershell
node scripts/site/build.mjs
python -m http.server 4173 --bind 127.0.0.1 --directory .local/site
```

The build fails when a page references a file or route that the site does not
contain. `npm run test:frontend` checks the pages themselves.
The build replaces only `.local/site` (the default) or `_site` (with
`--out _site`). Other output paths and symbolic links or junctions are refused.

## Keeping it current

Site copy follows the same rule as the user documents: it describes the latest
published release.

- **Sub 1 release.** The version and installer links come from the download
  table in [`README.md`](../README.md). The release pull request already updates
  that table, and merging it redeploys the site. Update the copy when the release
  changes what the page says.
- **Sub 2 release.** Update `SUB2_VERSION` in `render.mjs` and the Sub 2 copy.
  When the studio looks different, replace `assets/sub2-studio.png`: in a
  checkout of the released tag, run that repository's
  `docs/audit-20260926/ui/demo_server.py`, prepare the sample session, and
  capture the page at 1440 × 960 with a device scale factor of 2. The demo
  server uses invented titles and no provider account.

## Publishing

[`pages.yml`](../.github/workflows/pages.yml) builds the site for pull requests
that touch it and publishes on a push to `main`. Pages must use **GitHub
Actions** as its source in the repository settings.

[`google4bfbcf53cdd665f4.html`](google4bfbcf53cdd665f4.html) verifies the site
in Google Search Console for the property
`https://petershanxin.github.io/Meowcal-Sub/`. Keep it: removing the file ends
the verification. Search Console is also where the sitemap is submitted and
where query, impression, and click data for the site is read.

A project site cannot serve `robots.txt`: crawlers read that file only at the
host root, `petershanxin.github.io/robots.txt`, which belongs to a different
repository. The site therefore relies on the default of being crawlable, and
`robots.txt` cannot announce the sitemap.
