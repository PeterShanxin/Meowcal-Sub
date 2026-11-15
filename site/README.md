# Product site

The site at <https://petershanxin.github.io/Meowcal-Sub/> introduces Meowcal Sub
and Meowcal Sub 2 in English and Simplified Chinese:

| Page | English | 简体中文 |
| --- | --- | --- |
| Meowcal Sub | `/` | `/zh-CN/` |
| Meowcal Sub 2 | `/sub-2/` | `/zh-CN/sub-2/` |
| Translate visible subtitles | `/guides/translate-hardcoded-subtitles/` | `/zh-CN/guides/translate-hardcoded-subtitles/` |
| Choose an app | `/compare/meowcal-sub-vs-sub-2/` | `/zh-CN/compare/meowcal-sub-vs-sub-2/` |

It is static HTML with no JavaScript. Each page carries its own title,
description, canonical URL, and language alternates. Product pages use
`SoftwareApplication` structured data; guides use `WebPage`. `sitemap.xml`
lists all eight pages.

## Where things live

- [`scripts/site/content.en.mjs`](../scripts/site/content.en.mjs) and
  [`content.zh-CN.mjs`](../scripts/site/content.zh-CN.mjs) own the copy. Keep the
  two files equivalent in meaning; a unit test compares their structure.
- [`scripts/site/guides.en.mjs`](../scripts/site/guides.en.mjs) and
  [`guides.zh-CN.mjs`](../scripts/site/guides.zh-CN.mjs) own the scenario and
  comparison guides. Write for each language: “hardcoded / burned-in subtitles”
  in English, “视频自带字幕 / 画面中的字幕” in Chinese. Link guides from product
  pages and keep setup instructions aligned with the published user guides.
- [`scripts/site/render.mjs`](../scripts/site/render.mjs) owns the markup, the
  URLs, and the structured data.
- [`styles.css`](styles.css) owns the presentation.
- CSS motion adds a single settling light effect to the hero, scroll-linked
  text and image entrances, and a brief highlight on buttons. Reduced-motion
  preferences disable these effects and smooth scrolling. Browsers without
  view-timeline support show the content normally; no script is required.
- The cinematic hero uses `assets/cinema-light.webp`, an original generated
  decorative background. Product screenshots remain separate, unaltered images.
  The shared layout puts the product preview before the installer choices and
  works without client-side scripts or motion.
- Setup steps use alternating text and screenshots. Privacy, requirements and
  related products use open text columns without card backgrounds or borders.
  The footer contains repository and support links. Sub 1 downloads link to
  the separately downloaded translation model terms.
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
contain. `npm run test:frontend` checks page metadata, language pairs, links,
and anchors. The Pages workflow also runs Chromium at 390px and 1440px with
JavaScript disabled, checks layout, images, and language switching, and uploads
`site-browser-screenshots`. Run that check locally with:

```powershell
node scripts/site/build.mjs --out _site
npm exec -- playwright install chromium
node scripts/site/check-browser.mjs
```

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

## Search measurement

Submit `https://petershanxin.github.io/Meowcal-Sub/sitemap.xml` in Search Console
and Bing Webmaster Tools after publication. Inspect each new canonical URL;
record the selected canonical, last crawl, indexing status, and inspection date.
Review search impressions and clicks by page, language, and branded versus
non-branded query over the following 4–8 weeks. The site has no visit or download
click tracker; release download counts do not measure website conversion.

Public HTTP baseline, 2026-11-15: the four product pages and sitemap returned
200; each page's canonical matched its URL, with no `noindex` in HTML or
`X-Robots-Tag` header. The host-root `robots.txt` returned 404. Search Console
and Bing indexing, queries, and impressions were not available for this check.
A successful public request does not establish an index entry or crawler access
from every network.

For AI search, record a small fixed set of non-brand questions in English and
Chinese, the engine/date, whether search was enabled, official-page citations,
and any incorrect product claims. Use [Google's search guidance](https://developers.google.com/search/docs/fundamentals/ai-optimization-guide)
and [OpenAI's crawler controls](https://developers.openai.com/api/docs/bots)
when reviewing access; search and training crawler settings are independent.
