/**
 * Console governance tests U-1..U-5. Fully offline: every envelope is a
 * checked-in fixture captured from a live service run (ids and numbers
 * only), embedded as typed literals; fetch is stubbed; no socket is ever
 * opened.
 */
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

import type { AnswerEnvelope } from "@/lib/api";
import { AnswerCard } from "@/components/AnswerCard";
import { Console } from "@/components/Console";
import { DocInspector } from "@/components/DocInspector";
import { ProvenanceStrip } from "@/components/ProvenanceStrip";
import { ResultsList } from "@/components/ResultsList";

// U-1 (compile half): these imports `satisfies` the contract mirror — a
// field drift between service and console fails `tsc`, not the demo.
import {
  boundedEnvelope,
  docCard,
  emptyEnvelope,
  richEnvelope,
  supersededCard,
} from "./fixtures/typed";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("U-1: the contract mirror renders captured real envelopes", () => {
  it("renders the rich envelope's provenance and citations", () => {
    render(<AnswerCard envelope={richEnvelope} onOpenDoc={() => {}} />);
    expect(screen.getByTestId("badge-retrieval").textContent).toBe("retrieval: hybrid");
    expect(screen.getByTestId("badge-judge").textContent).toBe("judge: applied");
    expect(screen.getByTestId("badge-generation").textContent).toBe("generation: applied");
    const chips = screen.getAllByTestId("citation-chip");
    expect(chips.length).toBeGreaterThan(0);
    expect(chips[0].textContent).toMatch(/^\[d\d{4}\]$/);
  });

  it("renders the empty envelope as quiet fact, no counts, no hints", () => {
    const { container } = render(
      <ResultsList results={emptyEnvelope.results} onOpenDoc={() => {}} />,
    );
    expect(screen.getByTestId("empty-results").textContent).toBe(
      "Nothing in your scope matches",
    );
    expect(container.textContent).not.toMatch(/\d+ (hidden|suppressed|filtered|document)/i);
    expect(container.textContent).not.toMatch(/broaden/i);
  });

  it("renders enriched results with labeled sensitivity badges", () => {
    render(<ResultsList results={richEnvelope.results} onOpenDoc={() => {}} />);
    const rows = screen.getAllByTestId("result-row");
    expect(rows.length).toBe(richEnvelope.results.length);
    const badges = screen.getAllByTestId("sensitivity-badge");
    expect(badges.length).toBe(richEnvelope.results.length);
    for (const badge of badges) {
      expect(badge.textContent?.trim().length).toBeGreaterThan(0);
    }
  });
});

describe("U-2: the aggregation badge tracks aggregation_bounded exactly", () => {
  it("renders the badge when the rule fired", () => {
    render(<ProvenanceStrip envelope={boundedEnvelope} />);
    expect(screen.getByTestId("badge-aggregation").textContent).toBe(
      "aggregation rule applied",
    );
  });

  it("renders no badge when it did not", () => {
    render(<ProvenanceStrip envelope={richEnvelope} />);
    expect(screen.queryByTestId("badge-aggregation")).toBeNull();
  });
});

describe("U-3: no dark counts, at both layers", () => {
  it("the envelope type cannot represent a suppressed count (compile-time)", () => {
    // @ts-expect-error — suppressed_count is not representable in
    // AnswerEnvelope; if this ever compiles, the no-dark-counts rule broke.
    const tampered: AnswerEnvelope = { ...richEnvelope, suppressed_count: 3 };
    expect(tampered).toBeTruthy();
  });

  it("the runtime renderer ignores unknown fields entirely", () => {
    const tampered = JSON.parse(JSON.stringify(richEnvelope)) as Record<string, unknown>;
    // Distinctive values that could only appear by reading the dark fields.
    tampered["suppressed_count"] = 73154;
    (tampered["results"] as Record<string, unknown>[])[0]["hidden_count"] = 90217;
    const { container } = render(
      <div>
        <AnswerCard envelope={tampered as unknown as AnswerEnvelope} onOpenDoc={() => {}} />
        <ResultsList
          results={(tampered as unknown as AnswerEnvelope).results}
          onOpenDoc={() => {}}
        />
      </div>,
    );
    expect(container.textContent).not.toContain("73154");
    expect(container.textContent).not.toContain("90217");
    expect(container.textContent).not.toMatch(/suppressed|hidden/i);
  });
});

describe("U-4: switching principals clears the answer view", () => {
  it("leaves no cross-principal residue on screen", async () => {
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
      return new Response("{}", { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<Console />);
    // The list is virtualized; search narrows it so the row is rendered.
    fireEvent.change(screen.getByTestId("principal-search"), { target: { value: "p060" } });
    fireEvent.click(
      screen.getAllByTestId("principal-row").find((b) => b.textContent === "p060")!,
    );
    fireEvent.change(screen.getByTestId("query-input"), {
      target: { value: "payroll salary review" },
    });
    fireEvent.click(screen.getByTestId("ask-button"));
    await waitFor(() => expect(screen.getByTestId("answer-card")).toBeTruthy());
    expect(screen.getAllByTestId("citation-chip").length).toBeGreaterThan(0);

    // Switch principal: the answer view clears, no residue.
    fireEvent.change(screen.getByTestId("principal-search"), { target: { value: "p_void" } });
    fireEvent.click(
      screen.getAllByTestId("principal-row").find((b) => b.textContent === "p_void")!,
    );
    expect(screen.queryByTestId("answer-card")).toBeNull();
    expect(screen.queryByTestId("citation-chip")).toBeNull();
    expect(screen.queryByTestId("results-list")).toBeNull();
    expect(screen.queryByTestId("doc-inspector")).toBeNull();
  });
});

describe("U-5: every 404 is one indistinguishable empty state", () => {
  it("renders the identical inspector state for both 404 flavors", () => {
    // The service guarantees out-of-scope and nonexistent 404s are
    // byte-identical (A-10); the console maps both to null. Render the
    // inspector for two such nulls and compare the DOM byte-for-byte.
    const first = render(
      <DocInspector open loading={false} card={null} onClose={() => {}} onOpenDoc={() => {}} />,
    );
    const firstHtml = first.getByTestId("doc-inspector").innerHTML;
    first.unmount();
    const second = render(
      <DocInspector open loading={false} card={null} onClose={() => {}} onOpenDoc={() => {}} />,
    );
    const secondHtml = second.getByTestId("doc-inspector").innerHTML;
    expect(firstHtml).toBe(secondHtml);
    expect(second.getByTestId("inspector-empty").textContent).toBe(
      "This document isn't available.",
    );
  });

  it("renders real doc cards, including the superseded notice", () => {
    render(
      <DocInspector
        open
        loading={false}
        card={docCard}
        onClose={() => {}}
        onOpenDoc={() => {}}
      />,
    );
    expect(screen.getByTestId("inspector-card").textContent).toContain(docCard.title);
    // The service caps Unicode chars (Rust chars().count()); count code
    // points here, not UTF-16 units.
    expect(Array.from(docCard.snippet).length).toBeLessThanOrEqual(480);

    render(
      <DocInspector
        open
        loading={false}
        card={supersededCard}
        onClose={() => {}}
        onOpenDoc={() => {}}
      />,
    );
    expect(screen.getByTestId("superseded-notice")).toBeTruthy();
    expect(screen.getByTestId("successor-link").textContent).toBe(
      supersededCard.effective_successor,
    );
  });
});
