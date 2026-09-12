import { html, svg, type SVGTemplateResult, type TemplateResult } from "lit";
import type { IconName } from "./contracts";

// One stroke family for the whole interface: 24px grid, 1.8 stroke, round caps.
// Inline so icons inherit `currentColor` and stay crisp at every DPI.
const glyphs: Record<IconName, SVGTemplateResult> = {
  "arrow-right": svg`<path d="M4.5 12h15M13.5 6l6 6-6 6" />`,
  "chevron-right": svg`<path d="M9.5 6l6 6-6 6" />`,
  area: svg`<rect x="3.5" y="6.5" width="17" height="11" rx="1.5" stroke-dasharray="3 2.4" />`,
  play: svg`<path d="M8 5.2v13.6L19 12z" fill="currentColor" stroke="none" />`,
  stop: svg`<rect x="6.5" y="6.5" width="11" height="11" rx="1.6" fill="currentColor" stroke="none" />`,
  home: svg`<path d="M4 11l8-6.5 8 6.5M6 9.8v9.7h12V9.8M10 19.5v-5h4v5" />`,
  subtitles: svg`<rect x="3" y="5" width="18" height="14" rx="2.5" /><path d="M7 12.5h3.5M13 12.5h4M7 15.5h7" />`,
  gear: svg`<circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />`,
  wrench: svg`<path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" />`,
  download: svg`<path d="M12 4v11M7 10l5 5 5-5M5 20h14" />`,
  check: svg`<path d="M5 12.5l4.5 4.5L19 7.5" />`,
  "check-circle": svg`<circle cx="12" cy="12" r="9" /><path d="M8 12.3l2.7 2.7L16 9.6" />`,
  alert: svg`<circle cx="12" cy="12" r="9" /><path d="M12 7.5v5.5M12 16.4v.1" />`,
  ring: svg`<circle cx="12" cy="12" r="8.5" />`,
  spinner: svg`<path d="M12 3.5a8.5 8.5 0 1 0 8.5 8.5" />`,
  shield: svg`<path d="M12 3l7 3v5.5c0 4.5-3 8-7 9.5-4-1.5-7-5-7-9.5V6z" /><path d="M9 12l2 2 4-4" />`,
  clock: svg`<circle cx="12" cy="12" r="9" /><path d="M12 7v5l3.2 2" />`,
  info: svg`<circle cx="12" cy="12" r="9" /><path d="M12 11v5.5M12 7.8v.1" />`,
  text: svg`<path d="M3.5 18L8 6h1l4.5 12M5.2 14h6.6M16.5 18v-5.8M16.5 12.8c.6-1 1.5-1.5 2.6-1.5 1.3 0 2.4 1 2.4 2.6V18" />`,
  copy: svg`<rect x="8.5" y="8.5" width="11" height="11" rx="2" /><path d="M15.5 8.5v-2a2 2 0 0 0-2-2h-7a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h2" />`,
  redo: svg`<path d="M4.5 12a7.5 7.5 0 1 0 2.2-5.3M4.5 4v4.5H9" />`,
  close: svg`<path d="M6 6l12 12M18 6L6 18" />`,
  update: svg`<circle cx="12" cy="12" r="9" /><path d="M12 16.5v-9M8 11l4-4 4 4" />`,
};

export function icon(name: IconName, className = ""): TemplateResult {
  return html`<svg
    class=${className ? `icon ${className}` : "icon"}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.8"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    focusable="false"
  >
    ${glyphs[name]}
  </svg>`;
}

/** The line-art cat, lit from above. The coloured app icon stays for the taskbar and installer. */
export function catLogo(): TemplateResult {
  return html`<svg
    class="logo"
    viewBox="0 0 24 24"
    fill="none"
    stroke="url(#meowcal-logo-gradient)"
    stroke-width="1.8"
    stroke-linejoin="round"
    aria-hidden="true"
    focusable="false"
  >
    <defs>
      <linearGradient id="meowcal-logo-gradient" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="#eef3fb" />
        <stop offset="1" stop-color="#9fb0c8" />
      </linearGradient>
    </defs>
    <path
      d="M4.6 10V4.2l4.3 3.3c1-.33 2-.5 3.1-.5s2.1.17 3.1.5l4.3-3.3V10c.85 1.1 1.3 2.4 1.3 3.8 0 3.9-3.8 6.7-8.7 6.7s-8.7-2.8-8.7-6.7c0-1.4.45-2.7 1.3-3.8z"
    />
  </svg>`;
}
