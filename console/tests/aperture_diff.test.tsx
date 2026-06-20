/**
 * Lens-diff tests U-18..U-21 (AP-4). Fully offline: captured /lens/diff
 * fixtures embedded as typed literals; fetch stubbed; no sockets. U-6
 * sweeps the new components for color literals on its own.
 */
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import type { DiffResponse, LensResponse } from "@/lib/api";
import { Console } from "@/components/Console";
import { DiffView } from "@/components/DiffView";
import { diffP016P087, diffVoidP060 } from "./fixtures/diff_typed";

afterEach(() => {
  vi.unstubAllGlobals();
  // chooseCompare reflects the diff in the address bar; later mounts read
  // the entry doors from it, so every test starts from a clean URL.
  window.history.replaceState(null, "", "/");
});

function syntheticSelfLens(id: string, name: string): LensResponse {
  return {
    actor_id: id,
    agents: [],
    cross_lens: false,
    holdings: [],
    snapshot_version: "synthetic",
    subject: {
      groups: ["grp_synth"],
      id,
      kind: "human",
      name,
      sites: ["site_one"],
    },
  };
}

function stubDiffFetch(
  diff: DiffResponse,
  lensBodies: Record<string, LensResponse> = {},
) {
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.endsWith("/auth/login")) {
      return new Response(
        JSON.stringify({ principal_id: "demo", session_token: "test-session", expires_at: 9_999_999_999 }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }
    if (url.includes("/lens/diff?")) {
      return new Response(JSON.stringify(diff), { status: 200 });
    }
    for (const [id, body] of Object.entries(lensBodies)) {
      if (url.endsWith(`/lens/${id}`)) {
        return new Response(JSON.stringify(body), { status: 200 });
      }
    }
    if (url.includes("/doc/")) {
      return new Response(
        JSON.stringify({
          document_id: "d0093",
          sensitivity: "special_category",
          snippet: "snippet",
          title: "HR Record (Absence Summary): Gethin Tarnwold",
        }),
        { status: 200 },
      );
    }
    if (url.includes("/scope")) {
      return new Response(
        JSON.stringify({
          demo_identity_mode: true,
          principal_id: "p016",
          scope_statement: { band: null, groups: [], sites: [] },
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

const docIdsOf = (rows: HTMLElement[]) =>
  rows.map((row) => within(row).getByText(/^d\d{4}$/).textContent);

// ---------------------------------------------------------------------------
// U-18 COLUMNS
// ---------------------------------------------------------------------------

describe("U-18: exclusive columns render exactly; emptiness is whitespace", () => {
  it("renders every exclusive doc exactly once under its sentence header", async () => {
    stubDiffFetch(diffP016P087);
    render(
      <DiffView actor="p016" left="p016" right="p087" onClose={() => {}} onOpenDoc={() => {}} />,
    );
    await waitFor(() => expect(screen.getByTestId("diff-audited-line")).toBeTruthy());

    const leftRegion = screen.getByTestId("diff-left-only");
    const rightRegion = screen.getByTestId("diff-right-only");

    expect(
      within(leftRegion).getAllByTestId("diff-section-sentence").map((n) => n.textContent),
    ).toEqual(diffP016P087.left_only.map((s) => s.sentence));
    expect(
      within(rightRegion).getAllByTestId("diff-section-sentence").map((n) => n.textContent),
    ).toEqual(diffP016P087.right_only.map((s) => s.sentence));
    expect(
      within(leftRegion).getAllByTestId("diff-section-rule").map((n) => n.textContent),
    ).toEqual(diffP016P087.left_only.map((s) => s.reason));
    expect(
      within(rightRegion).getAllByTestId("diff-section-rule").map((n) => n.textContent),
    ).toEqual(diffP016P087.right_only.map((s) => s.reason));

    const leftIds = docIdsOf(within(leftRegion).getAllByTestId("diff-doc-row"));
    const expectedLeft = diffP016P087.left_only.flatMap((s) => s.docs.map((d) => d.document_id));
    expect(leftIds).toEqual(expectedLeft);
    expect(new Set(leftIds).size).toBe(leftIds.length);

    const rightIds = docIdsOf(within(rightRegion).getAllByTestId("diff-doc-row"));
    const expectedRight = diffP016P087.right_only.flatMap((s) => s.docs.map((d) => d.document_id));
    expect(rightIds).toEqual(expectedRight);
    expect(new Set(rightIds).size).toBe(rightIds.length);
  });

  it("renders a whitespace-empty exclusive column with NO placeholder text", async () => {
    stubDiffFetch(diffVoidP060);
    render(
      <DiffView actor="p060" left="p_void" right="p060" onClose={() => {}} onOpenDoc={() => {}} />,
    );
    await waitFor(() => expect(screen.getByTestId("diff-audited-line")).toBeTruthy());

    expect(diffVoidP060.left_only.length).toBe(0);
    const leftRegion = screen.getByTestId("diff-left-only");
    expect(leftRegion.textContent).toBe("");

    const rightRows = within(screen.getByTestId("diff-right-only")).getAllByTestId(
      "diff-doc-row",
    );
    expect(rightRows.length).toBe(
      diffVoidP060.right_only.reduce((n, s) => n + s.docs.length, 0),
    );
  });
});

// ---------------------------------------------------------------------------
// U-19 DIVERGENT ROWS
// ---------------------------------------------------------------------------

describe("U-19: divergent rows lead the shared table with both chips", () => {
  it("orders divergent first (stable within) and chips render on every row", async () => {
    stubDiffFetch(diffP016P087);
    render(
      <DiffView actor="p016" left="p016" right="p087" onClose={() => {}} onOpenDoc={() => {}} />,
    );
    await waitFor(() => expect(screen.getByTestId("diff-shared")).toBeTruthy());

    const rows = screen.getAllByTestId("shared-row");
    expect(rows.length).toBe(diffP016P087.shared.length);
    const divergentCount = diffP016P087.shared.filter((r) => r.divergent_route).length;
    expect(divergentCount).toBeGreaterThan(0);

    const flags = rows.map((row) => row.getAttribute("data-divergent"));
    expect(flags.slice(0, divergentCount).every((f) => f === "true")).toBe(true);
    expect(flags.slice(divergentCount).every((f) => f === "false")).toBe(true);

    // Stable within each partition: the service order is document_id asc.
    const ids = rows.map(
      (row) => within(row).getByTestId("shared-doc-row").textContent!.match(/d\d{4}/)![0],
    );
    const leadIds = ids.slice(0, divergentCount);
    const tailIds = ids.slice(divergentCount);
    expect([...leadIds].sort()).toEqual(leadIds);
    expect([...tailIds].sort()).toEqual(tailIds);

    for (const row of rows) {
      expect(within(row).getByTestId("shared-chip-left")).toBeTruthy();
      expect(within(row).getByTestId("shared-chip-right")).toBeTruthy();
    }

    // The charter example leads: d0093, SUBJECT:self against REBAC:grp_hr —
    // the two chips disagreeing IS the marker.
    const lead = rows[0];
    expect(within(lead).getByText("d0093")).toBeTruthy();
    expect(within(lead).getByTestId("shared-chip-left").textContent).toBe("SUBJECT:self");
    expect(within(lead).getByTestId("shared-chip-right").textContent).toBe("REBAC:grp_hr");

    // Non-divergent rows carry both chips too — in agreement, without the
    // lead position.
    const tail = rows[rows.length - 1];
    expect(tail.getAttribute("data-divergent")).toBe("false");
    expect(within(tail).getByTestId("shared-chip-left").textContent).toBe(
      within(tail).getByTestId("shared-chip-right").textContent,
    );
  });
});

// ---------------------------------------------------------------------------
// U-20 AUDITED LINE
// ---------------------------------------------------------------------------

describe("U-20: the audited line states the comparison plainly", () => {
  it("renders with both names and the actor; no dismissal", async () => {
    stubDiffFetch(diffP016P087);
    render(
      <DiffView actor="p016" left="p016" right="p087" onClose={() => {}} onOpenDoc={() => {}} />,
    );
    await waitFor(() => expect(screen.getByTestId("diff-audited-line")).toBeTruthy());

    expect(screen.getByTestId("diff-name-left").textContent).toBe(diffP016P087.left.name);
    expect(screen.getByTestId("diff-name-right").textContent).toBe(diffP016P087.right.name);
    const line = screen.getByTestId("diff-audited-line");
    expect(line.textContent).toBe(
      `Comparing as ${diffP016P087.actor_id} — this view is audited.`,
    );
    expect(within(line).queryAllByRole("button")).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// U-21 RESIDUE
// ---------------------------------------------------------------------------

describe("U-21: an actor switch mid-diff clears to the fresh lens", () => {
  it("drops the diff through the iris and renders the new subject's lens", async () => {
    stubDiffFetch(diffP016P087, {
      p016: syntheticSelfLens("p016", "Gethin Tarnwold"),
      p061: syntheticSelfLens("p061", "Synthetic Person Two"),
    });
    render(<Console view="lens" />);

    fireEvent.change(screen.getByTestId("principal-search"), { target: { value: "p016" } });
    fireEvent.click(
      screen.getAllByTestId("principal-row").find((b) => b.textContent === "p016")!,
    );
    await waitFor(() => expect(screen.getByTestId("masthead")).toBeTruthy());

    // Compare with p087: the diff view takes the room.
    fireEvent.change(screen.getByTestId("compare-search"), { target: { value: "p087" } });
    fireEvent.click(
      screen.getAllByTestId("compare-row").find((b) => b.textContent === "p087")!,
    );
    await waitFor(() => expect(screen.getByTestId("diff-view")).toBeTruthy());
    await waitFor(() => expect(screen.getByTestId("diff-audited-line")).toBeTruthy());

    // Switch the lens: the diff is gone SYNCHRONOUSLY (state cleared on
    // remount, before any fetch resolves) — the residue rule.
    fireEvent.change(screen.getByTestId("principal-search"), { target: { value: "p061" } });
    fireEvent.click(
      screen.getAllByTestId("principal-row").find((b) => b.textContent === "p061")!,
    );
    expect(screen.queryByTestId("diff-view")).toBeNull();

    // The fresh subject's lens renders; the diff never comes back.
    await waitFor(() => expect(screen.getByTestId("masthead")).toBeTruthy());
    expect(screen.getByTestId("masthead-id").textContent).toBe("p061");
    expect(screen.queryByTestId("diff-view")).toBeNull();
  });
});
