import { describe, expect, it } from "vitest";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  CONTENT,
  SITE_URL,
  pagePath,
  readSub1Release,
  renderPage,
  renderSite,
  renderSitemap,
} from "../../scripts/site/render.mjs";

const DOWNLOAD = "https://github.com/PeterShanxin/Meowcal-Sub/releases/download";
const readme = (x64 = "1.2.3", arm64 = x64) =>
  `[x64](${DOWNLOAD}/v${x64}/Meowcal.Sub_${x64}_x64-setup.exe)\n` +
  `[ARM64](${DOWNLOAD}/v${arm64}/Meowcal.Sub_${arm64}_arm64-setup.exe)\n`;

const release = readSub1Release(readme());
const PRODUCTS = ["sub1", "sub2"];
const LOCALES = Object.keys(CONTENT);
const pages = PRODUCTS.flatMap((product) =>
  LOCALES.map((locale) => ({ product, locale, html: renderPage(product, locale, release) })),
);

it("refuses to clean a directory outside the designated build outputs", () => {
  const directory = mkdtempSync(path.join(tmpdir(), "meowcal-site-output-test-"));
  const sentinel = path.join(directory, "keep.txt");
  writeFileSync(sentinel, "unrelated file");
  try {
    const build = fileURLToPath(new URL("../../scripts/site/build.mjs", import.meta.url));
    const result = spawnSync(process.execPath, [build, "--out", directory], {
      encoding: "utf8",
    });
    expect(result.status).not.toBe(0);
    expect(result.stderr).toContain("designated build output");
    expect(readFileSync(sentinel, "utf8")).toBe("unrelated file");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

function attributes(html, pattern) {
  return [...html.matchAll(pattern)].map((match) => match[1]);
}

describe("the published Sub 1 release", () => {
  it("comes from the README download links", () => {
    expect(release).toEqual({
      version: "1.2.3",
      x64: `${DOWNLOAD}/v1.2.3/Meowcal.Sub_1.2.3_x64-setup.exe`,
      arm64: `${DOWNLOAD}/v1.2.3/Meowcal.Sub_1.2.3_arm64-setup.exe`,
      notes: "https://github.com/PeterShanxin/Meowcal-Sub/releases/tag/v1.2.3",
    });
  });

  it("is refused when an installer link is missing", () => {
    expect(() => readSub1Release(readme().split("\n")[0])).toThrow(/no arm64 installer link/);
  });

  it("is refused when the two installers come from different releases", () => {
    expect(() => readSub1Release(readme("1.2.3", "1.2.2"))).toThrow(/different releases/);
  });
});

describe.each(pages)("the $product page in $locale", ({ product, locale, html }) => {
  const url = SITE_URL + pagePath(product, locale);

  it("sends support requests to this product's issue tracker", () => {
    const repository = product === "sub1" ? "Meowcal-Sub" : "Meowcal-Sub-2";
    expect(html).toContain(
      `href="https://github.com/PeterShanxin/${repository}/issues/new/choose"`,
    );
  });

  it("declares its language, one heading, and its own canonical URL", () => {
    expect(html).toContain(`<html lang="${locale}"`);
    expect(html.match(/<h1>/g)).toHaveLength(1);
    expect(attributes(html, /<link rel="canonical" href="([^"]+)">/g)).toEqual([url]);
    expect(html).toContain(`<meta property="og:url" content="${url}">`);
  });

  it("names every language version, itself included, and an x-default", () => {
    const alternates = attributes(html, /<link rel="alternate" hreflang="[^"]+" href="([^"]+)">/g);
    expect(alternates).toEqual([
      ...LOCALES.map((alternate) => SITE_URL + pagePath(product, alternate)),
      SITE_URL + pagePath(product, "en"),
    ]);
  });

  it("describes the same product in its structured data as on the page", () => {
    const [json] = attributes(
      html,
      /<script type="application\/ld\+json">\n([\s\S]+?)\n<\/script>/g,
    );
    const data = JSON.parse(json);
    const page = CONTENT[locale][product];

    expect(data["@type"]).toBe("SoftwareApplication");
    expect(html).toContain(`<h1>${data.name}`);
    expect(data.description).toBe(page.description);
    expect(data.url).toBe(url);
    expect(data.offers.price).toBe("0");
    // Ratings and reviews are never invented for a rich result.
    expect(data).not.toHaveProperty("aggregateRating");
    expect(data).not.toHaveProperty("review");
    for (const name of data.alternateName ?? []) {
      expect(html.replace(json, ""), `${name} must be visible on the page`).toContain(name);
    }
  });

  it("is complete without JavaScript", () => {
    expect(attributes(html, /<script\b([^>]*)>/gi)).toEqual([' type="application/ld+json"']);
    expect(html).not.toMatch(/\son[a-z]+="/i);
  });

  it("offers working in-page download and setup navigation", () => {
    const anchors = attributes(html, /href="#([^"]+)"/g);
    expect(anchors).toEqual(expect.arrayContaining(["download", "how-it-works"]));
    for (const anchor of anchors) {
      expect(html).toContain(`id="${anchor}"`);
    }
    expect(html).toContain('fetchpriority="high"');
  });

  it("links only to pages and files the site contains", () => {
    const directory = pagePath(product, locale);
    const siteFiles = new Set(renderSite(release).map((file) => file.path));
    const local = attributes(html, /href="([^"]+)"/g).filter(
      (href) => !/^(?:https?:|#)/.test(href) && !/(?:^|\/)(?:assets\/|styles\.css$)/.test(href),
    );

    expect(local.length).toBeGreaterThan(0);
    for (const href of local) {
      const target = new URL(href, `https://site.test/${directory}`).pathname.slice(1);
      expect(siteFiles, `${href} from ${directory || "/"}`).toContain(`${target}index.html`);
    }
  });

  it("leaves no placeholder unfilled", () => {
    expect(html).not.toMatch(/\{\w+\}|\*\*|\]\(/);
  });
});

describe("release-specific content", () => {
  it("offers the Sub 1 installers of the published release", () => {
    for (const locale of LOCALES) {
      const html = renderPage("sub1", locale, release);
      expect(html).toContain(`href="${release.x64}"`);
      expect(html).toContain(`href="${release.arm64}"`);
      expect(html).toContain(`href="${release.notes}"`);
      expect(html).toContain("1.2.3");
    }
  });

  it("does not put the Sub 1 release on the Sub 2 page", () => {
    expect(renderPage("sub2", "en", release)).not.toContain("1.2.3");
  });
});

describe("the two languages", () => {
  // Shape only: the same sections, lists, and links. Meaning is a review task.
  function shape(value) {
    if (Array.isArray(value)) return value.map(shape);
    if (value && typeof value === "object") {
      return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, shape(item)]));
    }
    return typeof value;
  }

  it("carry the same structure", () => {
    expect(shape(CONTENT["zh-CN"])).toEqual(shape(CONTENT.en));
  });

  it("use the same section order and anchors", () => {
    for (const product of PRODUCTS) {
      const outline = (locale) =>
        CONTENT[locale][product].sections.map(({ type, id }) => `${type}#${id}`);
      expect(outline("zh-CN")).toEqual(outline("en"));
    }
  });
});

describe("the sitemap", () => {
  it("lists every page once, each with all language versions", () => {
    const sitemap = renderSitemap();
    const expected = PRODUCTS.flatMap((product) =>
      LOCALES.map((locale) => SITE_URL + pagePath(product, locale)),
    );

    expect(attributes(sitemap, /<loc>([^<]+)<\/loc>/g)).toEqual(expected);
    expect(attributes(sitemap, /<xhtml:link[^>]+href="([^"]+)"\/>/g)).toHaveLength(
      expected.length * LOCALES.length,
    );
    expect(sitemap).not.toContain("<lastmod>");
  });
});
