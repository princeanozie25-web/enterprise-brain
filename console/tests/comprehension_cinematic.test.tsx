/**
 * COMPREHENSION + CINEMATIC PASS — standing tests (T-A1..T-A4, T-B1..T-B8).
 *
 * These are the durable guards for the one-pass corrections brief. Where a
 * behavior is already covered by an existing suite (graph keyboard/cluster/
 * zero-synthetic-nodes in aperture_graph, the picker + locked labels in
 * aperture_routes), this file adds the source-level LAWS that must not drift:
 * the string bans, the radius/glass/lift greps, the badge-contrast math, and
 * the reduced-motion budget.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";

import { SENSITIVITY_SCALE, SENSITIVITY_BADGE_INK, MOTION } from "@/lib/tokens";
import { SensitivityBadge } from "@/components/SensitivityBadge";
import { DocInspector } from "@/components/DocInspector";

const SRC = join(__dirname, "..", "src");

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const full = join(dir, name);
    return statSync(full).isDirectory() ? walk(full) : [full];
  });
}
const SRC_FILES = walk(SRC);
const TSX = SRC_FILES.filter((f) => f.endsWith(".tsx") || f.endsWith(".ts"));
const read = (f: string) => readFileSync(f, "utf8");

/** Render-facing strings: JSX text + common string-prop attributes. Comments
 * and identifiers are intentionally out of scope (the bans are about what a
 * USER reads, not internal names). */
function renderedText(source: string): string {
  const jsxText = source.match(/>[^<>{}]*[A-Za-z][^<>{}]*</g)?.join("\n") ?? "";
  const props =
    source.match(/(?:placeholder|aria-label|title|label|heading|lead|headline|sub|detail)=("[^"]*"|'[^']*')/g)?.join(
      "\n",
    ) ?? "";
  return `${jsxText}\n${props}`;
}

// ---- sRGB relative-luminance contrast (WCAG 2.x) ----
function luminance(hex: string): number {
  const m = hex.replace("#", "");
  const to = (h: string) => parseInt(h, 16) / 255;
  const [r, g, b] = [to(m.slice(0, 2)), to(m.slice(2, 4)), to(m.slice(4, 6))].map((c) =>
    c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4,
  );
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
function contrast(a: string, b: string): number {
  const [la, lb] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (la + 0.05) / (lb + 0.05);
}

// ===========================================================================
// TRACK A — comprehension
// ===========================================================================

describe("T-A2 / T-A3: no retired product names or coined room labels in rendered text", () => {
  const BANNED = [
    "Aperture",
    "Company Operating System",
    "Knowledge View",
    "Capability Map",
    // Copy pass: internal product names and retired room labels never render.
    "Bursar",
    "Workflow Command",
  ];
  for (const term of BANNED) {
    // Letter-boundary match: bans the term as a rendered WORD while letting
    // identifiers-as-strings (view keys like "adminBursar") stay out of scope,
    // per the renderedText contract above.
    const asWord = new RegExp(`(?<![A-Za-z])${term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![A-Za-z])`);
    it(`"${term}" appears in no rendered string`, () => {
      const offenders = TSX.filter((f) => asWord.test(renderedText(read(f)))).map((f) =>
        f.replace(SRC, "src"),
      );
      expect(offenders).toEqual([]);
    });
  }
});

describe("Copy pass: locked-table extension — labels + subtitles render verbatim", () => {
  const LOCKED: Array<{ file: string; strings: string[] }> = [
    // Nav doors carry the locked labels (label= is a rendered string prop).
    {
      file: "components/Console.tsx",
      strings: ['label="Projects"', 'label="Operating Map"', 'label="Spend Ledger"'],
    },
    // Room mastheads carry the one-line subtitles, verbatim from the table.
    { file: "components/ProjectSurface.tsx", strings: ["Work in flight, scoped to what you can see."] },
    { file: "components/GraphRoom.tsx", strings: ["The organization as your access renders it."] },
    {
      file: "components/BursarSurface.tsx",
      strings: ["Spend Ledger", "What AI assistance costs, and who authorized it."],
    },
  ];
  for (const { file, strings } of LOCKED) {
    for (const s of strings) {
      it(`${file} carries ${JSON.stringify(s)}`, () => {
        expect(read(join(SRC, file))).toContain(s);
      });
    }
  }

  it("the under-claim phrases are gone from every source file", () => {
    // The last two died when the Spend Ledger went live: STATE 3 copy is
    // fetch-driven, never a standing claim of disconnection.
    for (const stale of [
      "still being added",
      "in-progress slice",
      "not connected in this UI surface",
      "No ledger fixture",
    ]) {
      const offenders = TSX.filter((f) => read(f).includes(stale)).map((f) => f.replace(SRC, "src"));
      expect(offenders).toEqual([]);
    }
  });

  it("the Operating Map gate note claims grant-reachable enforcement (reconciled, not modest)", () => {
    expect(read(join(SRC, "components/AdminPreviewGate.tsx"))).toContain(
      "Structural and grant-reachable visibility are both enforced now.",
    );
  });
});

describe("T-A3: the Ask 'Verified answers' disabled copy kills the 'hallucinations on' reading", () => {
  it("names that every answer always shows its sources", () => {
    const console_ = read(join(SRC, "components", "Console.tsx"));
    expect(console_).toContain("every answer always shows its sources either way");
    expect(console_).toContain('role="status"');
    expect(console_).toContain('aria-live="polite"');
  });
});

// ===========================================================================
// TRACK B — composition
// ===========================================================================

describe("T-B1: SensitivityBadge text ≥4.5:1 on every tint in BOTH themes", () => {
  it("pins the badge ink so no level collapses in dark", () => {
    for (const [level, scale] of Object.entries(SENSITIVITY_SCALE)) {
      const ratio = contrast(SENSITIVITY_BADGE_INK, scale.background);
      expect(ratio, `${level} @ ${scale.background}`).toBeGreaterThanOrEqual(4.5);
    }
  });
  it("renders the pinned ink, not the theme ink", () => {
    render(<SensitivityBadge sensitivity="special_category" />);
    const badge = screen.getByTestId("sensitivity-badge");
    // jsdom serializes the hex to rgb(); SENSITIVITY_BADGE_INK #1a2233 = 26,34,51.
    expect(badge.getAttribute("style") ?? "").toContain("rgb(26, 34, 51)");
    // and NOT a theme-var (which would collapse on dark).
    expect(badge.getAttribute("style") ?? "").not.toContain("var(--ink)");
  });
});

describe("T-B3 / T-B7: radius law {8,16,9999} and a single hover-lift", () => {
  it("uses only rounded-lg / rounded-2xl / rounded-full (no bare rounded, no rounded-xl)", () => {
    const bad: string[] = [];
    for (const f of TSX) {
      const classes = read(f).match(/\brounded(-[a-z0-9]+)?\b/g) ?? [];
      for (const c of classes) {
        if (!["rounded-lg", "rounded-2xl", "rounded-full"].includes(c)) {
          bad.push(`${f.replace(SRC, "src")}: ${c}`);
        }
      }
    }
    expect(bad).toEqual([]);
  });
  it("the one hover-lift value is -2px, shared by CSS and framer", () => {
    expect(MOTION.framerHoverY).toBe(-2);
    const layout = read(join(SRC, "app", "layout.tsx"));
    expect(layout).toContain("translateY(-2px)");
    expect(layout).not.toContain("translateY(-4px)");
  });
});

describe("T-B4 (glass law): backdrop-filter lives only in overlay classes", () => {
  it("no page-level component sets a backdrop filter or a page-level glass class", () => {
    const componentOffenders = TSX.filter(
      (f) => f.includes("components") && /backdrop-?[fF]ilter|ap-glass-(?:panel|elevated|nav)\b/.test(read(f)),
    ).map((f) => f.replace(SRC, "src"));
    expect(componentOffenders).toEqual([]);
  });

  it("the retired page-level glass classes are gone; only overlay glass remains defined", () => {
    const layout = read(join(SRC, "app", "layout.tsx"));
    // Parse the base stylesheet into { selector -> rule-body } and check which
    // rules actually DECLARE a backdrop-filter (prose in comments is ignored).
    const rules = new Map<string, string>();
    for (const m of layout.matchAll(/(\.[a-z-]+)\s*\{([^{}]*)\}/g)) {
      rules.set(m[1], m[2]);
    }
    // Retired page-level glass classes no longer exist as definitions.
    for (const gone of [".ap-glass-panel", ".ap-glass-elevated", ".ap-glass-nav"]) {
      expect(rules.has(gone)).toBe(false);
    }
    // Overlay glass classes still exist.
    for (const kept of [".ap-glass-popover", ".ap-glass-scrim"]) {
      expect(rules.has(kept)).toBe(true);
    }
    // Every rule that DECLARES a backdrop-filter is an overlay class.
    for (const [selector, body] of rules) {
      if (/backdrop-filter:/.test(body)) {
        expect([".ap-glass-popover", ".ap-glass-scrim"]).toContain(selector);
      }
    }
  });
});

describe("T-B4 (color doctrine): amber is not spent on focus rings", () => {
  it("focus-visible uses the affordance token, never accent-warm", () => {
    const layout = read(join(SRC, "app", "layout.tsx"));
    const focusRule = layout.match(/:focus-visible \{[^}]*\}/)?.[0] ?? "";
    expect(focusRule).toContain("var(--affordance)");
    expect(focusRule).not.toContain("accent-warm");
    expect(focusRule).not.toContain("warm");
  });
});

describe("T-B6: DocInspector is a real dialog (role, modal, focus, Escape)", () => {
  it("carries dialog semantics and closes on Escape", () => {
    let open = true;
    const card = {
      document_id: "d0001",
      title: "Q3 Finance summary",
      sensitivity: "confidential",
      snippet: "…",
      superseded: false,
    } as never;
    const { rerender } = render(
      <DocInspector open loading={false} card={card} onClose={() => { open = false; }} onOpenDoc={() => {}} />,
    );
    const dialog = screen.getByTestId("doc-inspector");
    expect(dialog.getAttribute("role")).toBe("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(dialog.getAttribute("aria-label")).toBe("Document");
    // Escape closes via the shared primitive.
    const evt = new KeyboardEvent("keydown", { key: "Escape", bubbles: true });
    dialog.dispatchEvent(evt);
    expect(open).toBe(false);
    rerender(
      <DocInspector open={open} loading={false} card={card} onClose={() => {}} onOpenDoc={() => {}} />,
    );
    expect(screen.queryByTestId("doc-inspector")).toBeNull();
  });
});

describe("T-B5 (reduced motion): the media query zeroes animation + transforms", () => {
  it("layout honours prefers-reduced-motion", () => {
    const layout = read(join(SRC, "app", "layout.tsx"));
    const block = layout.match(/@media \(prefers-reduced-motion: reduce\) \{[\s\S]*?\n\}/)?.[0] ?? "";
    expect(block).toContain("animation: none");
    expect(block).toContain("transform: none");
  });
});

describe("skip link (WCAG 2.4.1) exists on the shell", () => {
  it("layout ships a skip-to-content link targeting #main", () => {
    const layout = read(join(SRC, "app", "layout.tsx"));
    expect(layout).toContain('href="#main"');
    expect(layout).toContain("ap-skip-link");
  });
});
