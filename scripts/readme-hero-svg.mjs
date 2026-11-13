import { writeFile } from "node:fs/promises";
import sharp from "sharp";

const escape = (value) =>
  String(value).replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll('"', "&quot;");

/** Preserve browser layout while keeping the large background and copy vector-based. */
export async function writeHeroSvg(page, logo, screenshotPath, outputPath) {
  const layout = await page.evaluate(() => {
    const document = globalThis.document;
    const box = (element) => {
      const { x, y, width, height } = element.getBoundingClientRect();
      return { x, y, width, height };
    };
    const canvas = document.createElement("canvas").getContext("2d");
    const lines = [];
    for (const element of document.querySelectorAll(".copy h1, .copy p")) {
      const style = globalThis.getComputedStyle(element);
      canvas.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
      for (const node of element.childNodes) {
        if (node.nodeType !== globalThis.Node.TEXT_NODE || !node.textContent.trim()) continue;
        const range = document.createRange();
        range.selectNodeContents(node);
        const bounds = box(range);
        lines.push({
          text: node.textContent,
          x: bounds.x,
          y: bounds.y + canvas.measureText(node.textContent).fontBoundingBoxAscent,
          size: style.fontSize,
          weight: style.fontWeight,
          spacing: style.letterSpacing,
          color: style.color,
          family: style.fontFamily,
        });
      }
    }
    return {
      logo: box(document.querySelector(".logo")),
      app: box(document.querySelector(".app")),
      lines,
    };
  });
  const screenshot = await sharp(screenshotPath).webp({ lossless: true, effort: 6 }).toBuffer();
  const original = await sharp(screenshotPath).ensureAlpha().raw().toBuffer();
  const decoded = await sharp(screenshot).ensureAlpha().raw().toBuffer();
  if (!original.equals(decoded)) throw new Error("Hero screenshot compression changed pixels");
  const { app, lines } = layout;
  const logoSvg = logo.replace(
    "<svg ",
    `<svg x="${layout.logo.x}" y="${layout.logo.y}" width="${layout.logo.width}" height="${layout.logo.height}" `,
  );
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="720" viewBox="0 0 1600 720" role="img" aria-label="${escape(lines[0].text)}">
    <defs>
      <radialGradient id="background" gradientUnits="userSpaceOnUse" cx="0" cy="0" r="1" gradientTransform="translate(256 0) scale(${1344 * Math.SQRT2} ${720 * Math.SQRT2})">
        <stop stop-color="#292635"/><stop offset=".42" stop-color="#17181f"/><stop offset="1" stop-color="#101115"/>
      </radialGradient>
      <filter id="shadow" x="-30%" y="-30%" width="160%" height="180%"><feGaussianBlur stdDeviation="35"/></filter>
      <clipPath id="app-clip"><rect x="${app.x}" y="${app.y}" width="${app.width}" height="${app.height}" rx="12"/></clipPath>
    </defs>
    <rect width="1600" height="720" fill="url(#background)"/>
    <rect x="${app.x}" y="${app.y + 30}" width="${app.width}" height="${app.height}" rx="12" fill="#000" opacity=".6" filter="url(#shadow)"/>
    ${logoSvg}
    ${lines.map((line) => `<text x="${line.x}" y="${line.y}" font-family="${escape(line.family)}" font-size="${line.size}" font-weight="${line.weight}" letter-spacing="${line.spacing}" fill="${line.color}">${escape(line.text)}</text>`).join("\n")}
    <image x="${app.x + 1}" y="${app.y + 1}" width="${app.width - 2}" height="${app.height - 2}" href="data:image/webp;base64,${screenshot.toString("base64")}" clip-path="url(#app-clip)"/>
    <rect x="${app.x + 0.5}" y="${app.y + 0.5}" width="${app.width - 1}" height="${app.height - 1}" rx="11.5" fill="none" stroke="#fff" stroke-opacity=".13"/>
  </svg>`;
  await writeFile(outputPath, svg);
  return svg;
}
