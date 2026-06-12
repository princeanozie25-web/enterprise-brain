/**
 * Lens-room tests U-10..U-13 (AP-2). Fully offline: captured /lens fixtures
 * embedded as typed literals; fetch stubbed; no sockets. U-6 sweeps the new
 * components for color literals on its own.
 */
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import type { LensResponse } from "@/lib/api";
import { EgoGraph } from "@/components/EgoGraph";
import { LensRoom } from "@/components/LensRoom";
import { lensP060Cross, lensP060Self, lensP061Self } from "./fixtures/lens_typed";

afterEach(() => {
  vi.unstubAllGlobals();
});

function stubLensFetch() {
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.endsWith("/lens/p060")) {
      // The actor header decides which capture is faithful; the test routes
      // by subject and uses the cross capture for the p061-actor case.
      return new Response(JSON.stringify(currentP060Fixture), { status: 200 });
    }
    if (url.endsWith("/lens/p061")) {
      return new Response(JSON.stringify(lensP061Self), { status: 200 });
    }
    if (url.includes("/doc/")) {
      return new Response(
        JSON.stringify({
          document_id: "d0121",
          sensitivity: "restricted",
          snippet: "snippet",
          title: "Board minutes",
        }),
        { status: 200 },
      );
    }
    return new Response("{\"demo_identity_mode\":true,\"error\":\"not found\"}", {
      status: 404,
    });
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

let currentP060Fixture: LensResponse = lensP060Self;

// ---------------------------------------------------------------------------
// U-10 GROUP RENDER
// ---------------------------------------------------------------------------

describe("U-10: reason-grouped holdings render completely", () => {
  it("renders every section sentence, every primary doc exactly once, and the chips", async () => {
    currentP060Fixture = lensP060Self;
    stubLensFetch();
    render(<LensRoom actor="p060" />);
    await waitFor(() => expect(screen.getByTestId("masthead")).toBeTruthy());

    // Every section header sentence, in order, PUBLIC:all last.
    const sentences = screen.getAllByTestId("section-sentence").map((n) => n.textContent);
    expect(sentences).toEqual(lensP060Self.holdings.map((s) => s.sentence));
    const rules = screen.getAllByTestId("section-rule").map((n) => n.textContent);
    expect(rules).toEqual(lensP060Self.holdings.map((s) => s.reason));
    expect(rules[rules.length - 1]).toBe("PUBLIC:all");

    // Every primary doc exactly once.
    const rows = screen.getAllByTestId("lens-doc-row");
    const expectedDocs = lensP060Self.holdings.flatMap((s) => s.docs.map((d) => d.document_id));
    expect(rows.length).toBe(expectedDocs.length);
    const renderedIds = rows.map(
      (row) => within(row).getByText(/^d\d{4}$/).textContent,
    );
    expect(new Set(renderedIds).size).toBe(expectedDocs.length);
    expect([...renderedIds].sort()).toEqual([...expectedDocs].sort());

    // also_via chips present, one per secondary reason.
    const expectedChips = lensP060Self.holdings
      .flatMap((s) => s.docs)
      .reduce((sum, d) => sum + d.also_via.length, 0);
    expect(expectedChips).toBeGreaterThan(0);
    expect(screen.getAllByTestId("also-via-chip").length).toBe(expectedChips);
  });
});

// ---------------------------------------------------------------------------
// U-11 EGO-GRAPH CAP
// ---------------------------------------------------------------------------

function syntheticLens(ringNodes: number): LensResponse {
  return {
    actor_id: "p001",
    agents: [],
    cross_lens: false,
    holdings: [],
    snapshot_version: "synthetic",
    subject: {
      groups: Array.from({ length: ringNodes - 1 }, (_, i) =>
        `grp_synth_${String(i).padStart(2, "0")}`,
      ),
      id: "p001",
      kind: "human",
      name: "Synthetic Person",
      sites: ["site_one"],
    },
  };
}

describe("U-11: the ego-graph cap is a cliff, not a truncation", () => {
  it("renders the SVG at exactly 21 nodes", () => {
    // 20 ring nodes (19 groups + 1 site) + center = 21 total.
    const { container } = render(
      <EgoGraph lens={syntheticLens(20)} onGroupClick={() => {}} />,
    );
    expect(screen.getByTestId("ego-graph")).toBeTruthy();
    expect(container.querySelectorAll("svg").length).toBe(1);
    expect(screen.queryByTestId("ego-fallback")).toBeNull();
  });

  it("renders the list fallback and NO svg at 22 nodes", () => {
    const { container } = render(
      <EgoGraph lens={syntheticLens(21)} onGroupClick={() => {}} />,
    );
    expect(screen.getByTestId("ego-fallback")).toBeTruthy();
    expect(container.querySelectorAll("svg").length).toBe(0);
    // The fallback lists EVERY ring node — nothing truncated silently.
    expect(within(screen.getByTestId("ego-fallback")).getAllByRole("listitem").length).toBe(21);
  });

  it("renders the captured p060 graph well under the cap", () => {
    render(<EgoGraph lens={lensP060Self} onGroupClick={() => {}} />);
    expect(screen.getByTestId("ego-graph")).toBeTruthy();
    expect(screen.getAllByTestId("ego-node-group").length).toBe(
      lensP060Self.subject.groups.length,
    );
  });
});

// ---------------------------------------------------------------------------
// U-12 CROSS-LENS LINE
// ---------------------------------------------------------------------------

describe("U-12: the audited-view line states crossing plainly", () => {
  it("renders for cross_lens=true, absent for false, with no dismissal", async () => {
    currentP060Fixture = lensP060Cross;
    stubLensFetch();
    render(<LensRoom actor="p061" />);
    await waitFor(() => expect(screen.getByTestId("masthead")).toBeTruthy());
    // Self view first (p061 as p061): no line.
    expect(screen.queryByTestId("cross-lens-line")).toBeNull();

    // Cross to p060: the quiet line appears, neutral, undismissable.
    fireEvent.change(screen.getByTestId("subject-search"), { target: { value: "p060" } });
    fireEvent.click(
      screen.getAllByTestId("subject-row").find((b) => b.textContent === "p060")!,
    );
    await waitFor(() => expect(screen.getByTestId("cross-lens-line")).toBeTruthy());
    const line = screen.getByTestId("cross-lens-line");
    expect(line.textContent).toBe("Viewing as p061 — this view is audited.");
    expect(within(line).queryAllByRole("button")).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// U-13 REGISTERS
// ---------------------------------------------------------------------------

describe("U-13: the room speaks the right registers", () => {
  it("masthead name chrome, id evidence; sentences chrome; rule chips evidence", async () => {
    currentP060Fixture = lensP060Self;
    stubLensFetch();
    render(<LensRoom actor="p060" />);
    await waitFor(() => expect(screen.getByTestId("masthead")).toBeTruthy());

    expect(screen.getByTestId("masthead-name").className).toContain("ap-register-chrome");
    expect(screen.getByTestId("masthead-id").className).toContain("ap-register-evidence");
    for (const sentence of screen.getAllByTestId("section-sentence")) {
      expect(sentence.className).toContain("ap-register-chrome");
    }
    for (const rule of screen.getAllByTestId("section-rule")) {
      expect(rule.className).toContain("ap-register-evidence");
    }
    for (const chip of screen.getAllByTestId("also-via-chip")) {
      expect(chip.className).toContain("ap-register-evidence");
    }
  });
});
