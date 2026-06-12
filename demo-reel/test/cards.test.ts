// T-R2: card determinism, and the card law — audit cards render the
// LITERAL source bytes, recoverable byte-for-byte from the HTML.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { cardHtml, cardTextOf, escapeHtml, unescapeHtml } from "../src/cards.ts";
import type { CardFonts } from "../src/cards.ts";

const here = dirname(fileURLToPath(import.meta.url));
const fonts: CardFonts = {
  chrome: "data:font/woff2;base64,QUFB",
  evidence: "data:font/woff2;base64,QkJC",
};

describe("T-R2: card determinism and literal bytes", () => {
  it("same inputs -> identical HTML string", () => {
    const spec = { kind: "title" as const, text: "Enterprise Brain — the demo" };
    expect(cardHtml(spec, fonts)).toBe(cardHtml(spec, fonts));
    const audit = { kind: "audit" as const, heading: "h", raw: "{\"a\":1}\n{\"b\":2}" };
    expect(cardHtml(audit, fonts)).toBe(cardHtml(audit, fonts));
  });

  it("audit card text == the fixture log tail, byte for byte", () => {
    const raw = readFileSync(join(here, "fixtures", "audit_tail.txt"), "utf8");
    const html = cardHtml(
      { kind: "audit", heading: "One act, two rows", raw },
      fonts,
    );
    expect(cardTextOf(html)).toBe(raw);
  });

  it("escaping round-trips hostile bytes", () => {
    const hostile = `{"x":"<pre> & 'quotes' \\"double\\""}`;
    expect(unescapeHtml(escapeHtml(hostile))).toBe(hostile);
    const html = cardHtml({ kind: "footer", heading: "f", raw: hostile }, fonts);
    expect(cardTextOf(html)).toBe(hostile);
  });

  it("cards never paraphrase: the payload appears exactly once, escaped", () => {
    const raw = "actor: p061    subjects: p060 | agent_finance_analyst";
    const html = cardHtml({ kind: "footer", heading: "footer", raw }, fonts);
    const occurrences = html.split(escapeHtml(raw)).length - 1;
    expect(occurrences).toBe(1);
  });
});
