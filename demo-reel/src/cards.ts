// Cards: HTML in the Aperture language — paper #FAFAF7, ink #16160F,
// ink-soft #5C5C54, the console's vendored fonts (read-only, inlined as
// data URLs so a bare about:blank page can render them) — screenshot to
// 1920x1080 PNG by the caller's Playwright page.
//
// CARD RULES (build prompt): AUDIT and FOOTER cards render the LITERAL
// bytes read at capture time. `cardHtml` is deterministic (T-R2: same
// inputs -> identical string) and `cardTextOf` recovers the exact payload
// bytes from the HTML so the test can assert text == source bytes.

import { readFileSync } from "node:fs";
import { join } from "node:path";

export interface CardFonts {
  /** data: URLs for @font-face — chrome (Inter) and evidence (Plex Mono). */
  chrome: string;
  evidence: string;
}

export type CardSpec =
  | { kind: "title"; text: string }
  | { kind: "numbers"; text: string }
  | { kind: "close"; text: string }
  | { kind: "audit"; heading: string; raw: string }
  | { kind: "footer"; heading: string; raw: string };

export function escapeHtml(s: string): string {
  return s
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

export function unescapeHtml(s: string): string {
  return s
    .replaceAll("&#39;", "'")
    .replaceAll("&quot;", '"')
    .replaceAll("&gt;", ">")
    .replaceAll("&lt;", "<")
    .replaceAll("&amp;", "&");
}

/** Reads the console's vendored woff2 subsets (read-only) into data URLs. */
export function loadConsoleFonts(repoRoot: string): CardFonts {
  const fontDir = join(repoRoot, "console", "src", "fonts");
  const dataUrl = (file: string): string =>
    `data:font/woff2;base64,${readFileSync(join(fontDir, file)).toString("base64")}`;
  return {
    chrome: dataUrl("inter-latin-600-normal.woff2"),
    evidence: dataUrl("ibm-plex-mono-latin-400-normal.woff2"),
  };
}

const PAPER = "#FAFAF7";
const INK = "#16160F";
const INK_SOFT = "#5C5C54";

function shell(fonts: CardFonts, body: string): string {
  return `<!doctype html><html><head><meta charset="utf-8"><style>
@font-face { font-family: "EBChrome"; src: url("${fonts.chrome}") format("woff2"); }
@font-face { font-family: "EBEvidence"; src: url("${fonts.evidence}") format("woff2"); }
html, body { margin: 0; padding: 0; }
body { width: 1920px; height: 1080px; background: ${PAPER}; color: ${INK};
  font-family: "EBChrome", sans-serif; display: flex; flex-direction: column;
  align-items: center; justify-content: center; box-sizing: border-box;
  padding: 96px; text-align: center; }
.soft { color: ${INK_SOFT}; }
.evidence { font-family: "EBEvidence", monospace; }
pre.evidence { text-align: left; font-size: 26px; line-height: 1.6;
  background: ${PAPER}; border: 1px solid rgba(92, 92, 84, 0.24);
  border-radius: 4px; padding: 36px 44px; max-width: 1640px; overflow: hidden;
  white-space: pre-wrap; word-break: break-all; margin: 0; }
h1 { font-size: 64px; line-height: 1.2; font-weight: 600; max-width: 1500px; margin: 0; }
h2 { font-size: 40px; line-height: 1.25; font-weight: 600; max-width: 1500px; margin: 0 0 40px 0; }
p.sub { font-size: 30px; line-height: 1.45; max-width: 1400px; margin: 28px 0 0 0; }
</style></head><body>${body}</body></html>`;
}

export function cardHtml(spec: CardSpec, fonts: CardFonts): string {
  switch (spec.kind) {
    case "title":
      return shell(fonts, `<h1>${escapeHtml(spec.text)}</h1>`);
    case "numbers":
      return shell(
        fonts,
        `<h2>The verified numbers</h2><p class="sub">${escapeHtml(spec.text)}</p>`,
      );
    case "close":
      return shell(fonts, `<p class="sub">${escapeHtml(spec.text)}</p>`);
    case "audit":
    case "footer":
      // The literal bytes, never a paraphrase.
      return shell(
        fonts,
        `<h2>${escapeHtml(spec.heading)}</h2><pre class="evidence" data-card-payload>${escapeHtml(spec.raw)}</pre>` +
          `<p class="sub soft">real ${spec.kind === "audit" ? "audit log rows" : "PDF footer"} — read at capture time, rendered verbatim</p>`,
      );
  }
}

/** Recovers the literal payload bytes from an audit/footer card's HTML. */
export function cardTextOf(html: string): string | null {
  const match = html.match(/<pre class="evidence" data-card-payload>([\s\S]*?)<\/pre>/);
  return match ? unescapeHtml(match[1]) : null;
}

/** Renders a card HTML string to a 1920x1080 PNG via a Playwright page. */
export async function renderCardPng(
  page: {
    setViewportSize(size: { width: number; height: number }): Promise<void>;
    setContent(html: string, options?: { waitUntil?: "load" }): Promise<void>;
    evaluate<T>(fn: () => T): Promise<T>;
    screenshot(options: { path: string }): Promise<unknown>;
  },
  html: string,
  outPath: string,
): Promise<void> {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.setContent(html, { waitUntil: "load" });
  await page.evaluate(() => (document as unknown as { fonts: { ready: Promise<unknown> } }).fonts.ready.then(() => true));
  await page.screenshot({ path: outPath });
}
