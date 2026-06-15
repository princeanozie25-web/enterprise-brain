/**
 * Aperture AP-1 governance tests U-6..U-9. Fully offline. U-1..U-5 continue
 * unmodified in tests/governance_u.test.tsx.
 */
import fs from "node:fs";
import path from "node:path";
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import { AnswerCard } from "@/components/AnswerCard";
import { Console } from "@/components/Console";
import { richEnvelope } from "./fixtures/typed";

afterEach(() => {
  vi.unstubAllGlobals();
});

// ---------------------------------------------------------------------------
// U-6 COLOR DISCIPLINE — the reserved-color law as CI
// ---------------------------------------------------------------------------

const COLOR_LITERAL =
  /#[0-9a-fA-F]{3,8}\b|\brgba?\s*\(|\bhsla?\s*\(/;

function walkFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...walkFiles(full));
    } else if (/\.(tsx?|css)$/.test(entry.name)) {
      out.push(full);
    }
  }
  return out;
}

describe("U-6: color discipline", () => {
  const consoleRoot = path.resolve(__dirname, "..");

  it("no color literal exists outside tokens.ts", () => {
    const scanned = [
      ...walkFiles(path.join(consoleRoot, "src", "components")),
      ...walkFiles(path.join(consoleRoot, "src", "app")),
    ];
    expect(scanned.length).toBeGreaterThan(8);
    const offenders: string[] = [];
    for (const file of scanned) {
      const text = fs.readFileSync(file, "utf8");
      const match = text.match(COLOR_LITERAL);
      if (match) {
        offenders.push(`${path.relative(consoleRoot, file)}: ${match[0]}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it("tokens.ts contains exactly the reserved palette plus the five sensitivity hues", () => {
    const tokens = fs.readFileSync(
      path.join(consoleRoot, "src", "lib", "tokens.ts"),
      "utf8",
    );
    const hexes = new Set(
      (tokens.match(/#[0-9a-fA-F]{6}\b/g) ?? []).map((h) => h.toUpperCase()),
    );
    const expected = new Set(
      [
        // The reserved neutrals + affordance.
        "#FAFAF7",
        "#16160F",
        "#5C5C54",
        "#3D5A80",
        // The five sensitivity hues (background + border), unchanged.
        "#E8F1F8",
        "#0072B2",
        "#E6F4EF",
        "#009E73",
        "#FBF1DC",
        "#E69F00",
        "#F9E8DE",
        "#D55E00",
        "#F6EAF1",
        "#CC79A7",
        // The dark theme (Org Brain surface): a designed palette, not an
        // inversion — paper, ink, ink-soft, affordance, hairline, wash.
        "#14130E",
        "#F4F3EE",
        "#9A9A90",
        "#7AA0CE",
        "#33322C",
        "#222019",
        // The one reserved warm accent — lit path + core glow.
        "#C77F3A",
      ].map((h) => h.toUpperCase()),
    );
    expect([...hexes].sort()).toEqual([...expected].sort());

    // Derived alphas may only reuse the ink-soft components — no new hues
    // can hide inside an rgba().
    for (const rgba of tokens.match(/rgba?\([^)]*\)/g) ?? []) {
      expect(rgba.startsWith("rgba(92, 92, 84")).toBe(true);
    }
  });
});

// ---------------------------------------------------------------------------
// Shared console harness (fetch stubbed; no sockets)
// ---------------------------------------------------------------------------

function stubFetch() {
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (url.endsWith("/ask")) {
      return new Response(JSON.stringify(richEnvelope), { status: 200 });
    }
    if (url.includes("/scope")) {
      const principal = new Headers(init?.headers).get("x-demo-principal");
      return new Response(
        JSON.stringify({
          demo_identity_mode: true,
          principal_id: principal,
          scope_statement: { band: 5, groups: ["grp_finance"], sites: ["site_keldonbury"] },
        }),
        { status: 200 },
      );
    }
    if (url.includes("/doc/")) {
      return new Response(
        JSON.stringify({
          document_id: "d0200",
          sensitivity: "confidential",
          snippet: "snippet",
          title: "Notice: aggregate financial position",
        }),
        { status: 200 },
      );
    }
    return new Response("{}", { status: 200 });
  });
  vi.stubGlobal("fetch", fetchMock);
}

function stubMatchMedia(reduceMotion: boolean) {
  vi.stubGlobal(
    "matchMedia",
    vi.fn().mockReturnValue({
      matches: reduceMotion,
      media: "(prefers-reduced-motion: reduce)",
      addEventListener: () => {},
      removeEventListener: () => {},
    } as unknown as MediaQueryList),
  );
}

function switchLens(principal: string) {
  fireEvent.change(screen.getByTestId("principal-search"), {
    target: { value: principal },
  });
  fireEvent.click(
    screen.getAllByTestId("principal-row").find((b) => b.textContent === principal)!,
  );
}

// ---------------------------------------------------------------------------
// U-7 IRIS + RESIDUE
// ---------------------------------------------------------------------------

describe("U-7: the iris clears residue and respects reduced motion", () => {
  it("lens switch through the LensBar clears answer and results", async () => {
    stubFetch();
    stubMatchMedia(false);
    render(<Console />);

    switchLens("p060");
    fireEvent.change(screen.getByTestId("query-input"), {
      target: { value: "payroll salary review" },
    });
    fireEvent.click(screen.getByTestId("ask-button"));
    await waitFor(() => expect(screen.getByTestId("answer-card")).toBeTruthy());
    expect(screen.getAllByTestId("citation-chip").length).toBeGreaterThan(0);

    switchLens("p_void");
    expect(screen.queryByTestId("answer-card")).toBeNull();
    expect(screen.queryByTestId("citation-chip")).toBeNull();
    expect(screen.queryByTestId("results-list")).toBeNull();
    expect(screen.queryByTestId("doc-inspector")).toBeNull();
    // The stage is present and keyed to the new lens (the iris remounts it).
    expect(screen.getByTestId("iris-stage")).toBeTruthy();
  });

  it("animates with the iris class when motion is allowed", () => {
    stubFetch();
    stubMatchMedia(false);
    render(<Console />);
    const stage = screen.getByTestId("iris-stage");
    expect(stage.className).toContain("irisIn");
    expect(stage.className).not.toContain("fadeIn");
  });

  it("prefers-reduced-motion renders without the clip-path class", () => {
    stubFetch();
    stubMatchMedia(true);
    render(<Console />);
    const stage = screen.getByTestId("iris-stage");
    expect(stage.className).not.toContain("irisIn");
    expect(stage.className).toContain("fadeIn");
  });
});

// ---------------------------------------------------------------------------
// U-8 REGISTER INTEGRITY
// ---------------------------------------------------------------------------

describe("U-8: type registers", () => {
  it("the answer speaks serif; the evidence speaks mono", () => {
    render(<AnswerCard envelope={richEnvelope} onOpenDoc={() => {}} />);
    const answer = screen.getByTestId("answer-text");
    expect(answer.className).toContain("ap-register-answer");
    const chips = screen.getAllByTestId("citation-chip");
    expect(chips.length).toBeGreaterThan(0);
    for (const chip of chips) {
      expect(chip.className).toContain("ap-register-evidence");
    }
    // The model's voice never bleeds into chrome: the no-answer state is
    // chrome register (asserted via the fixture-free branch).
    render(
      <AnswerCard
        envelope={{ ...richEnvelope, answer: undefined, generation_applied: false }}
        onOpenDoc={() => {}}
      />,
    );
    expect(screen.getByTestId("no-answer").className).toContain("ap-register-chrome");
  });
});

// ---------------------------------------------------------------------------
// U-9 BANNER PERMANENCE
// ---------------------------------------------------------------------------

describe("U-9: the demo caption is furniture", () => {
  it("survives every interaction path and offers no dismissal", async () => {
    stubFetch();
    stubMatchMedia(false);
    render(<Console />);

    const expectBanner = () => {
      const banner = screen.getByTestId("demo-banner");
      expect(banner.textContent).toContain("Demo identity mode");
      expect(within(banner).queryAllByRole("button")).toEqual([]);
    };

    expectBanner();
    fireEvent.click(screen.getByTestId("lens-current")); // open switcher
    expectBanner();
    switchLens("p060");
    expectBanner();
    fireEvent.change(screen.getByTestId("query-input"), {
      target: { value: "payroll salary review" },
    });
    fireEvent.click(screen.getByTestId("ask-button"));
    await waitFor(() => expect(screen.getByTestId("answer-card")).toBeTruthy());
    expectBanner();
    fireEvent.click(screen.getAllByTestId("citation-chip")[0]); // open inspector
    await waitFor(() => expect(screen.getByTestId("doc-inspector")).toBeTruthy());
    expectBanner();
    fireEvent.click(screen.getByTestId("inspector-close"));
    expectBanner();
    switchLens("p_void");
    expectBanner();
  });
});
