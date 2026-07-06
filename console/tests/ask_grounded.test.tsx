/**
 * K1 grounded answers — console copy states. Fully offline.
 *
 * Pins: (1) the three generated-path aria-live states (answer-anchored /
 * no-claim-survived / no-written-answer) plus the untouched zero-results
 * refusal; (2) the removed-claims disclosure line renders ONLY when
 * refused > 0; (3) the ProvenanceStrip grounding badge claims ANCHORING,
 * never semantic verification (ruling R-B); (4) the honest-copy law: no
 * "verified" wording anywhere on the grounding surfaces.
 */
import fs from "node:fs";
import path from "node:path";
import React from "react";
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";

import { ProvenanceStrip } from "@/components/ProvenanceStrip";
import type { AnswerEnvelope } from "@/lib/api";
import { richEnvelope } from "./fixtures/typed";

const SRC = path.resolve(__dirname, "..", "src");
const read = (p: string) => fs.readFileSync(p, "utf8");

/** richEnvelope, upgraded to a grounded K1 envelope. */
function groundedEnvelope(admitted: number, refused: number): AnswerEnvelope {
  const base = { ...richEnvelope } as AnswerEnvelope;
  return {
    ...base,
    answer:
      admitted > 0
        ? {
            citations: ["d0200"],
            claims: Array.from({ length: admitted }, (_, i) => ({
              doc_id: "d0200",
              locator: `d0200@${i * 7}`,
              text: `Anchored claim ${i}.`,
            })),
            text: "Anchored claim [d0200].",
          }
        : undefined,
    generation_applied: admitted > 0,
    grounding: { admitted, refused },
    grounding_applied: true,
  };
}

// ---------------------------------------------------------------------------
// T-K1a: the three generated-path aria-live states are pinned in source
// ---------------------------------------------------------------------------

describe("T-K1a: Console.tsx carries the three generated-path live states", () => {
  const console_ = read(path.join(SRC, "components", "Console.tsx"));

  it("answer-present names anchoring and an openable source", () => {
    expect(console_).toContain("each anchored to a source you can open");
  });

  it("all-refused grounding is its own honest state", () => {
    expect(console_).toContain(
      "no claim survived grounding — nothing unverifiable was invented",
    );
  });

  it("no-generation keeps an honest, build-agnostic line", () => {
    expect(console_).toContain("no written answer was generated for this ask");
    expect(console_).not.toContain("no written answer was generated in this build");
  });

  it("the zero-results refusal is unchanged", () => {
    expect(console_).toContain(
      "Nothing within your access supports an answer, and nothing was invented.",
    );
  });

  it("the live region survives as role=status aria-live=polite", () => {
    expect(console_).toContain('role="status"');
    expect(console_).toContain('aria-live="polite"');
  });
});

// ---------------------------------------------------------------------------
// T-K1b: the removed-claims disclosure line
// ---------------------------------------------------------------------------

describe("T-K1b: the removed-claims line is disclosed, calm, and conditional", () => {
  const console_ = read(path.join(SRC, "components", "Console.tsx"));

  it("carries the disclosure copy verbatim (plural + singular)", () => {
    expect(console_).toContain(
      "draft claims were removed: not verbatim-supported by your sources.",
    );
    expect(console_).toContain(
      "1 draft claim was removed: not verbatim-supported by your sources.",
    );
  });

  it("renders only when refused > 0 (the guard is in source)", () => {
    expect(console_).toContain("envelope.grounding.refused > 0");
  });
});

// ---------------------------------------------------------------------------
// T-K1c: the ProvenanceStrip grounding badge (render-level)
// ---------------------------------------------------------------------------

describe("T-K1c: ProvenanceStrip discloses grounding as ANCHORING", () => {
  it("renders admitted + removed counts when the gate ran", () => {
    render(<ProvenanceStrip envelope={groundedEnvelope(3, 2)} />);
    const badge = screen.getByTestId("badge-grounding");
    expect(badge.textContent).toBe("grounding: anchored (3 admitted · 2 removed)");
  });

  it("renders no grounding badge when the gate did not run", () => {
    render(<ProvenanceStrip envelope={richEnvelope as AnswerEnvelope} />);
    expect(screen.queryByTestId("badge-grounding")).toBeNull();
  });

  it("renders the all-refused disclosure (0 admitted) the same calm way", () => {
    render(<ProvenanceStrip envelope={groundedEnvelope(0, 4)} />);
    const badge = screen.getByTestId("badge-grounding");
    expect(badge.textContent).toBe("grounding: anchored (0 admitted · 4 removed)");
  });
});

// ---------------------------------------------------------------------------
// T-K1d: honest-copy law — anchoring, never semantic verification (R-B)
// ---------------------------------------------------------------------------

describe("T-K1d: grounding copy never claims semantic verification", () => {
  it("the grounding surfaces say anchored/verbatim, not verified", () => {
    for (const file of ["components/ProvenanceStrip.tsx"]) {
      const source = read(path.join(SRC, file));
      // "verified"/"verification" may not appear in any grounding-adjacent
      // rendered string of the strip (the Ask toggle's own "Verified
      // answers" label lives in Console.tsx and is out of grounding scope).
      expect(source.toLowerCase()).not.toContain("verified");
      expect(source).toContain("anchored");
    }
    const console_ = read(path.join(SRC, "components", "Console.tsx"));
    expect(console_).toContain("not verbatim-supported by your sources");
  });
});
