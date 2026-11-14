// Renders the product site as static HTML: one page per product and language,
// plus the sitemap. Pure functions over the copy in content.*.mjs, so the pages
// can be checked without building images or touching the network.
//
// Every page is complete without JavaScript. Internal links are relative, so the
// output works under any base path; only canonical, hreflang, Open Graph, and
// sitemap URLs are absolute.

import en from "./content.en.mjs";
import zhCN from "./content.zh-CN.mjs";

export const SITE_URL = "https://petershanxin.github.io/Meowcal-Sub/";
export const CONTENT = { en, "zh-CN": zhCN };
export const IMAGE_WIDTHS = [800, 1600];

const SUB1_REPO = "https://github.com/PeterShanxin/Meowcal-Sub";
const SUB2_REPO = "https://github.com/PeterShanxin/Meowcal-Sub-2";
const STORE_URL = "https://apps.microsoft.com/detail/9NNK2X23VLWT";
// The Sub 2 release that the Sub 2 copy and screenshot describe.
const SUB2_VERSION = "0.2.2";

const LOCALE_PATHS = { en: "", "zh-CN": "zh-CN/" };
const PRODUCT_PATHS = { sub1: "", sub2: "sub-2/" };
const DEFAULT_LOCALE = "en";

// A source per language where the screenshot shows localized UI, one otherwise.
// The Store screenshots are 2560 x 1440 scenes. The step images crop them to
// 3:2: the app window for Home and Subtitle style, the scene centre for the selector.
const APP_WINDOW = { left: 500, top: 200, width: 1560, height: 1040 };
export const IMAGES = {
  "sub1-overlay": storeScreenshot("01-overlay"),
  "sub1-home": storeScreenshot("02-home", APP_WINDOW),
  "sub1-selection": storeScreenshot("03-selection", {
    left: 200,
    top: 0,
    width: 2160,
    height: 1440,
  }),
  "sub1-style": storeScreenshot("04-style", APP_WINDOW),
  "sub2-studio": { width: 2880, height: 1920, sources: { en: "site/assets/sub2-studio.png" } },
};

const PRODUCTS = {
  sub1: {
    name: "Meowcal Sub",
    mark: "assets/mark.svg",
    socialImage: { file: "assets/social-preview.png", width: 1280, height: 640 },
    repository: SUB1_REPO,
    operatingSystem: "Windows 11",
    interfaceLanguages: ["en", "zh-CN"],
    alternateNames: ["MeowcalSub", "妙影"],
  },
  sub2: {
    name: "Meowcal Sub 2",
    mark: "assets/sub2-icon.svg",
    socialImage: { file: "assets/sub2-social-preview.png", width: 1280, height: 640 },
    repository: SUB2_REPO,
    operatingSystem: "Windows",
    interfaceLanguages: ["en"],
    alternateNames: [],
  },
};

function storeScreenshot(name, crop) {
  return {
    width: crop?.width ?? 2560,
    height: crop?.height ?? 1440,
    crop,
    sources: {
      en: `docs/assets/store/en-${name}.png`,
      "zh-CN": `docs/assets/store/zh-CN-${name}.png`,
    },
  };
}

export function pagePath(product, locale) {
  return LOCALE_PATHS[locale] + PRODUCT_PATHS[product];
}

export function imageFile(id, locale, width) {
  const variant = IMAGES[id].sources[locale] ? locale : DEFAULT_LOCALE;
  return `assets/${id}.${variant}-${width}.webp`;
}

/**
 * The published Sub 1 release, read from the README download table.
 *
 * The README is already the reviewed record of the latest published installers,
 * so the site follows it instead of keeping a second version to update.
 */
export function readSub1Release(readme) {
  const found = {};
  for (const architecture of ["x64", "arm64"]) {
    const pattern = new RegExp(
      `${SUB1_REPO.replaceAll(".", "\\.")}/releases/download/v(\\d+\\.\\d+\\.\\d+)/Meowcal\\.Sub_\\1_${architecture}-setup\\.exe`,
    );
    const match = pattern.exec(readme);
    if (!match) {
      throw new Error(`README.md has no ${architecture} installer link for the product site.`);
    }
    found[architecture] = { url: match[0], version: match[1] };
  }
  if (found.x64.version !== found.arm64.version) {
    throw new Error("README.md links x64 and ARM64 installers from different releases.");
  }
  const { version } = found.x64;
  return {
    version,
    x64: found.x64.url,
    arm64: found.arm64.url,
    notes: `${SUB1_REPO}/releases/tag/v${version}`,
  };
}

function escapeHtml(text) {
  return String(text)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function fill(text, values) {
  return text.replace(/\{(\w+)\}/g, (placeholder, key) => values[key] ?? placeholder);
}

function pageContext(product, locale, release) {
  const path = pagePath(product, locale);
  const root = "../".repeat(path.split("/").length - 1);
  const values =
    product === "sub1"
      ? { version: release.version, notes: release.notes }
      : { version: SUB2_VERSION };

  const context = {
    product,
    locale,
    root,
    url: SITE_URL + path,
    asset: (file) => root + file,
    // "./" keeps a link to the directory itself valid when root is empty.
    page: (target, targetLocale = locale) => root + pagePath(target, targetLocale) || "./",
    href: (target) => {
      if (target === "@issues") return `${PRODUCTS[product].repository}/issues/new/choose`;
      return target.startsWith("@") ? context.page(target.slice(1)) : target;
    },
    text: (text) => escapeHtml(fill(text, values)),
    rich: (text) =>
      escapeHtml(fill(text, values))
        .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
        .replace(
          /\[([^\]]+)\]\(([^)\s]+)\)/g,
          (_, label, target) => `<a href="${context.href(target)}">${label}</a>`,
        ),
    fill: (text) => fill(text, values),
  };
  return context;
}

function image(context, id, alt, { sizes, eager = false }) {
  const { width, height } = IMAGES[id];
  const largest = IMAGE_WIDTHS.at(-1);
  const srcset = IMAGE_WIDTHS.map(
    (candidate) => `${context.asset(imageFile(id, context.locale, candidate))} ${candidate}w`,
  ).join(", ");
  const loading = eager ? 'fetchpriority="high"' : 'loading="lazy" decoding="async"';
  return (
    `<img src="${context.asset(imageFile(id, context.locale, largest))}" srcset="${srcset}" ` +
    `sizes="${sizes}" width="${largest}" height="${Math.round((largest * height) / width)}" ` +
    `alt="${context.text(alt)}" ${loading}>`
  );
}

const SECTION_RENDERERS = {
  callout: (section, context) =>
    `<aside class="callout"><p><strong>${context.text(section.title)}</strong> ${context.rich(section.body)}</p></aside>`,

  steps: (section, context) => {
    const items = section.items
      .map((item) => {
        const picture = item.image
          ? image(context, item.image, item.alt, { sizes: "(min-width: 60rem) 22rem, 100vw" })
          : "";
        return `<li><h3>${context.text(item.title)}</h3><p>${context.rich(item.body)}</p>${picture}</li>`;
      })
      .join("\n");
    return `<h2>${context.text(section.heading)}</h2>\n<ol class="steps">\n${items}\n</ol>`;
  },

  pairs: (section, context) => {
    const rows = section.items
      .map(
        (item) =>
          `<tr><th scope="row">${context.text(item.term)}</th><td>${context.rich(item.detail)}</td></tr>`,
      )
      .join("\n");
    return (
      `<h2>${context.text(section.heading)}</h2>\n<table class="pairs">\n` +
      `<thead><tr><th scope="col">${context.text(section.termLabel)}</th>` +
      `<th scope="col">${context.text(section.detailLabel)}</th></tr></thead>\n` +
      `<tbody>\n${rows}\n</tbody>\n</table>`
    );
  },

  columns: (section, context) => {
    const columns = section.columns
      .map((column) => {
        const items = column.items.map((item) => `<li>${context.rich(item)}</li>`).join("");
        return `<div class="panel"><h3>${context.text(column.title)}</h3><ul>${items}</ul></div>`;
      })
      .join("\n");
    return (
      `<h2>${context.text(section.heading)}</h2>\n<div class="columns">\n${columns}\n</div>\n` +
      `<p class="note">${context.rich(section.body)}</p>`
    );
  },

  faq: (section, context) => {
    const items = section.items
      .map((item) => `<div><h3>${context.text(item.q)}</h3><p>${context.rich(item.a)}</p></div>`)
      .join("\n");
    return `<h2>${context.text(section.heading)}</h2>\n<div class="faq">\n${items}\n</div>`;
  },

  products: (section, context) => {
    const items = section.items
      .map((item) => {
        const name = item.href
          ? `<a href="${context.href(item.href)}">${context.text(item.name)}</a>`
          : `${context.text(item.name)} <span class="badge">${context.text(item.current)}</span>`;
        return `<li><h3>${name}</h3><p>${context.rich(item.body)}</p></li>`;
      })
      .join("\n");
    return (
      `<h2>${context.text(section.heading)}</h2>\n<p class="intro">${context.rich(section.intro)}</p>\n` +
      `<ul class="products">\n${items}\n</ul>`
    );
  },
};

function renderSection(section, context) {
  const id = section.id ? ` id="${section.id}"` : "";
  return `<section${id} class="wrap section">\n${SECTION_RENDERERS[section.type](section, context)}\n</section>`;
}

function renderActions(page, context, release) {
  const targets = {
    x64: { href: release.x64, primary: true },
    arm64: { href: release.arm64, primary: true },
    store: { href: STORE_URL },
    latest: { href: `${SUB2_REPO}/releases/latest`, primary: true },
  };
  const buttons = Object.keys(targets)
    .filter((key) => page.actions[key])
    .map((key) => {
      const { label, hint } = page.actions[key];
      const hintMarkup = hint ? `<span class="hint">${context.text(hint)}</span>` : "";
      const variant = targets[key].primary ? " primary" : "";
      return `<a class="button${variant}" href="${targets[key].href}"><span>${context.text(label)}</span>${hintMarkup}</a>`;
    })
    .join("\n");
  const links = page.actions.links
    .map((link) => `<li><a href="${context.fill(link.href)}">${context.text(link.label)}</a></li>`)
    .join("");
  return (
    `<div class="actions">\n${buttons}\n</div>\n` +
    `<p class="meta">${context.text(page.actions.meta)}</p>\n<ul class="links">${links}</ul>`
  );
}

function structuredData(page, context, release) {
  const product = PRODUCTS[context.product];
  const data = {
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    name: product.name,
    ...(product.alternateNames.length > 0 && { alternateName: product.alternateNames }),
    description: page.description,
    url: context.url,
    image: SITE_URL + product.socialImage.file,
    screenshot: SITE_URL + imageFile(page.figure.image, context.locale, IMAGE_WIDTHS.at(-1)),
    applicationCategory: "MultimediaApplication",
    operatingSystem: product.operatingSystem,
    softwareVersion: context.product === "sub1" ? release.version : SUB2_VERSION,
    downloadUrl: `${product.repository}/releases/latest`,
    inLanguage: product.interfaceLanguages,
    isAccessibleForFree: true,
    license: "https://www.gnu.org/licenses/agpl-3.0.html",
    offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
    author: { "@type": "Person", name: "Shanxin Li", url: "https://github.com/PeterShanxin" },
    sameAs: context.product === "sub1" ? [SUB1_REPO, STORE_URL] : [SUB2_REPO],
  };
  // "<" cannot end the script element early once it is escaped.
  return JSON.stringify(data, null, 2).replaceAll("<", "\\u003c");
}

function renderHead(page, context, release) {
  const product = PRODUCTS[context.product];
  const copy = CONTENT[context.locale];
  const alternates = Object.keys(CONTENT)
    .map(
      (locale) =>
        `<link rel="alternate" hreflang="${locale}" href="${SITE_URL + pagePath(context.product, locale)}">`,
    )
    .join("\n");
  const otherLocales = Object.keys(CONTENT)
    .filter((locale) => locale !== context.locale)
    .map((locale) => `<meta property="og:locale:alternate" content="${CONTENT[locale].ogLocale}">`)
    .join("\n");

  return `<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${context.text(page.title)}</title>
<meta name="description" content="${context.text(page.description)}">
<link rel="canonical" href="${context.url}">
${alternates}
<link rel="alternate" hreflang="x-default" href="${SITE_URL + pagePath(context.product, DEFAULT_LOCALE)}">
<meta property="og:type" content="website">
<meta property="og:site_name" content="Meowcal Sub">
<meta property="og:title" content="${context.text(page.title)}">
<meta property="og:description" content="${context.text(page.description)}">
<meta property="og:url" content="${context.url}">
<meta property="og:image" content="${SITE_URL + product.socialImage.file}">
<meta property="og:image:width" content="${product.socialImage.width}">
<meta property="og:image:height" content="${product.socialImage.height}">
<meta property="og:locale" content="${copy.ogLocale}">
${otherLocales}
<meta name="twitter:card" content="summary_large_image">
<meta name="theme-color" content="#0b0c11">
<link rel="icon" href="${context.asset("assets/favicon.svg")}" type="image/svg+xml">
<link rel="stylesheet" href="${context.asset("styles.css")}">
<script type="application/ld+json">
${structuredData(page, context, release)}
</script>
</head>`;
}

function renderHeader(context) {
  const copy = CONTENT[context.locale];
  const current = (product) => (product === context.product ? ' aria-current="page"' : "");
  const [otherLocale] = Object.keys(CONTENT).filter((locale) => locale !== context.locale);
  const other = CONTENT[otherLocale];

  return `<header class="site-header">
<div class="wrap">
<a class="brand" href="${context.page("sub1")}"${current("sub1")}><img src="${context.asset(PRODUCTS.sub1.mark)}" width="28" height="28" alt="">${context.text(copy.sub1.navName)}</a>
<nav aria-label="${context.text(copy.navLabel)}">
<a href="${context.page("sub2")}"${current("sub2")}>${context.text(copy.sub2.navName)}</a>
<a href="${PRODUCTS[context.product].repository}">${context.text(copy.source)}</a>
<a class="language" href="${context.page(context.product, otherLocale)}" lang="${other.lang}" hreflang="${other.lang}" aria-label="${context.text(copy.languageLabel)}: ${other.languageName}">${other.languageName}</a>
</nav>
</div>
</header>`;
}

function renderFooter(context) {
  const { footer } = CONTENT[context.locale];
  const links = footer.links
    .map((link) => `<li><a href="${context.href(link.href)}">${context.text(link.label)}</a></li>`)
    .join("");
  return `<footer class="site-footer">
<div class="wrap">
<p class="name">${context.text(footer.name)}</p>
<p>${context.text(footer.provider)}</p>
<p>${context.rich(footer.model)}</p>
<ul class="links">${links}</ul>
</div>
</footer>`;
}

export function renderPage(product, locale, release) {
  const copy = CONTENT[locale];
  const page = copy[product];
  const context = pageContext(product, locale, release);

  return `<!doctype html>
<html lang="${copy.lang}" data-product="${product}">
${renderHead(page, context, release)}
<body>
<a class="skip" href="#main">${context.text(copy.skipLink)}</a>
${renderHeader(context)}
<main id="main">
<section class="wrap hero">
<img class="hero-mark" src="${context.asset(PRODUCTS[product].mark)}" width="72" height="72" alt="">
<h1>${context.text(page.name)}</h1>
<p class="tagline">${context.text(page.tagline)}</p>
<p class="promise">${context.text(page.promise)}</p>
<p class="lead">${context.rich(page.lead)}</p>
${renderActions(page, context, release)}
<figure>
${image(context, page.figure.image, page.figure.alt, { sizes: "(min-width: 72rem) 70rem, 100vw", eager: true })}
<figcaption>${context.text(page.figure.caption)}</figcaption>
</figure>
</section>
${page.sections.map((section) => renderSection(section, context)).join("\n")}
</main>
${renderFooter(context)}
</body>
</html>
`;
}

export function renderSitemap() {
  const entries = Object.keys(PRODUCT_PATHS).flatMap((product) =>
    Object.keys(CONTENT).map((locale) => {
      const alternates = Object.keys(CONTENT)
        .map(
          (alternate) =>
            `    <xhtml:link rel="alternate" hreflang="${alternate}" href="${SITE_URL + pagePath(product, alternate)}"/>`,
        )
        .join("\n");
      return `  <url>\n    <loc>${SITE_URL + pagePath(product, locale)}</loc>\n${alternates}\n  </url>`;
    }),
  );
  return `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">
${entries.join("\n")}
</urlset>
`;
}

/** Every text file of the site, as `{ path, contents }` relative to its root. */
export function renderSite(release) {
  const pages = Object.keys(PRODUCT_PATHS).flatMap((product) =>
    Object.keys(CONTENT).map((locale) => ({
      path: `${pagePath(product, locale)}index.html`,
      contents: renderPage(product, locale, release),
    })),
  );
  return [...pages, { path: "sitemap.xml", contents: renderSitemap() }];
}
