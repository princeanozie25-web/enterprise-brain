/**
 * Atlas-room tests U-14..U-17 (AP-3). Fully offline: captured /atlas
 * fixtures embedded as typed literals; fetch stubbed; no sockets. U-6
 * sweeps the new components for color literals on its own.
 */
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import type { AtlasCapability, AtlasResponse, LensResponse } from "@/lib/api";
import { AtlasRoom } from "@/components/AtlasRoom";
import { Console } from "@/components/Console";
import { EgoGraph } from "@/components/EgoGraph";
import { atlasExpander, atlasP060 } from "./fixtures/atlas_typed";
import { lensP060Self } from "./fixtures/lens_typed";

afterEach(() => {
  vi.unstubAllGlobals();
});

function stubAtlasFetch(atlas: AtlasResponse) {
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.endsWith("/atlas")) {
      return new Response(JSON.stringify(atlas), { status: 200 });
    }
    if (url.includes("/doc/")) {
      return new Response(
        JSON.stringify({
          document_id: "d0183",
          sensitivity: "internal",
          snippet: "snippet",
          title: "Customer Account: Elderwick Pharmacy (AC-0051)",
        }),
        { status: 200 },
      );
    }
    if (url.includes("/scope")) {
      return new Response(
        JSON.stringify({
          demo_identity_mode: true,
          principal_id: "p060",
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

function allCapabilities(atlas: AtlasResponse): AtlasCapability[] {
  return atlas.strategies.flatMap((strategy) =>
    strategy.initiatives.flatMap((initiative) =>
      initiative.workflows.flatMap((workflow) => workflow.capabilities),
    ),
  );
}

// ---------------------------------------------------------------------------
// U-14 GRID FIDELITY
// ---------------------------------------------------------------------------

describe("U-14: the grid renders the capture completely, in fixture order", () => {
  it("renders every band, column, group, and card in order", async () => {
    stubAtlasFetch(atlasP060);
    render(<AtlasRoom actor="p060" />);
    await waitFor(() => expect(screen.getAllByTestId("strategy-band").length).toBe(6));

    expect(screen.getAllByTestId("band-id").map((n) => n.textContent)).toEqual(
      atlasP060.strategies.map((s) => s.id),
    );
    expect(screen.getAllByTestId("band-name").map((n) => n.textContent)).toEqual(
      atlasP060.strategies.map((s) => s.name),
    );
    expect(screen.getAllByTestId("column-id").map((n) => n.textContent)).toEqual(
      atlasP060.strategies.flatMap((s) => s.initiatives.map((i) => i.id)),
    );
    expect(screen.getAllByTestId("group-id").map((n) => n.textContent)).toEqual(
      atlasP060.strategies.flatMap((s) =>
        s.initiatives.flatMap((i) => i.workflows.map((w) => w.id)),
      ),
    );
    const capabilities = allCapabilities(atlasP060);
    expect(screen.getAllByTestId("capability-id").map((n) => n.textContent)).toEqual(
      capabilities.map((c) => c.id),
    );
    expect(capabilities.length).toBe(90);
  });

  it("renders the em-dash for a zero-evidence capability with NO other text", async () => {
    stubAtlasFetch(atlasP060);
    render(<AtlasRoom actor="p060" />);
    await waitFor(() => expect(screen.getAllByTestId("strategy-band").length).toBe(6));

    const capabilities = allCapabilities(atlasP060);
    const emptyIndex = capabilities.findIndex((c) => c.docs.length === 0);
    expect(emptyIndex).toBeGreaterThanOrEqual(0);
    const cards = screen.getAllByTestId("capability-card");
    const evidence = within(cards[emptyIndex]).getByTestId("capability-evidence");
    expect(evidence.textContent).toBe("—");
    expect(within(cards[emptyIndex]).queryAllByTestId("card-doc-row")).toEqual([]);
    expect(within(cards[emptyIndex]).queryByTestId("capability-more")).toBeNull();

    // And an evidence-bearing card previews at most 3 of the viewer's rows.
    const fullIndex = capabilities.findIndex((c) => c.docs.length > 0);
    const rows = within(cards[fullIndex]).getAllByTestId("card-doc-row");
    expect(rows.length).toBe(Math.min(capabilities[fullIndex].docs.length, 3));
  });
});

// ---------------------------------------------------------------------------
// U-15 SHEET + INSPECTOR
// ---------------------------------------------------------------------------

describe("U-15: the sheet and inspector chain", () => {
  it("card -> sheet -> doc row -> inspector works on the capture", async () => {
    const fetchMock = stubAtlasFetch(atlasP060);
    render(<AtlasRoom actor="p060" />);
    await waitFor(() => expect(screen.getAllByTestId("strategy-band").length).toBe(6));

    const capabilities = allCapabilities(atlasP060);
    const targetIndex = capabilities.findIndex((c) => c.docs.length > 0);
    const target = capabilities[targetIndex];
    const cards = screen.getAllByTestId("capability-card");
    fireEvent.click(within(cards[targetIndex]).getByTestId("capability-open"));

    const sheet = await screen.findByTestId("capability-sheet");
    expect(within(sheet).getByTestId("sheet-name").textContent).toBe(target.name);
    const rows = within(sheet).getAllByTestId("sheet-doc-row");
    expect(rows.length).toBe(target.docs.length);

    fireEvent.click(rows[0]);
    await waitFor(() => expect(screen.getByTestId("inspector-card")).toBeTruthy());
    const docCalls = fetchMock.mock.calls
      .map((call) => String(call[0]))
      .filter((url) => url.includes("/doc/"));
    expect(docCalls).toEqual([
      expect.stringContaining(`/doc/${target.docs[0].document_id}`),
    ]);
  });

  it("'+N more' expands to exactly the fixture's remainder", async () => {
    stubAtlasFetch(atlasExpander);
    render(<AtlasRoom actor="p_synth" />);
    await waitFor(() => expect(screen.getAllByTestId("capability-card").length).toBe(2));

    const card = screen.getAllByTestId("capability-card")[0];
    expect(within(card).getAllByTestId("card-doc-row").length).toBe(3);
    const more = within(card).getByTestId("capability-more");
    expect(more.textContent).toBe("+2 more");
    fireEvent.click(more);
    expect(within(card).getAllByTestId("card-doc-row").length).toBe(5);
    expect(within(card).queryByTestId("capability-more")).toBeNull();

    // The sheet keeps the strike + effective-version link, redaction honored.
    fireEvent.click(within(card).getByTestId("capability-open"));
    const sheet = await screen.findByTestId("capability-sheet");
    expect(within(sheet).getAllByTestId("sheet-doc-row").length).toBe(5);
    const successors = within(sheet).getAllByTestId("sheet-successor-link");
    expect(successors.map((n) => n.textContent)).toEqual(["d9105"]);
  });
});

// ---------------------------------------------------------------------------
// U-16 RING
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

describe("U-16: the ego-graph capability ring", () => {
  it("renders capability nodes within the cap", () => {
    render(
      <EgoGraph
        lens={lensP060Self}
        onGroupClick={() => {}}
        capabilities={["cap03", "cap17", "cap55"]}
        onCapabilityClick={() => {}}
      />,
    );
    expect(screen.getByTestId("ego-graph")).toBeTruthy();
    expect(screen.getAllByTestId("ego-node-capability").length).toBe(3);
    expect(screen.getAllByTestId("ego-node-group").length).toBe(
      lensP060Self.subject.groups.length,
    );
  });

  it("falls back to the list with capabilities included at 22+ nodes", () => {
    const capabilities = Array.from({ length: 10 }, (_, i) =>
      `cap${String(i + 1).padStart(2, "0")}`,
    );
    // 12 lens ring nodes + 10 capabilities + center = 23 > 21.
    const { container } = render(
      <EgoGraph
        lens={syntheticLens(12)}
        onGroupClick={() => {}}
        capabilities={capabilities}
        onCapabilityClick={() => {}}
      />,
    );
    expect(screen.getByTestId("ego-fallback")).toBeTruthy();
    expect(container.querySelectorAll("svg").length).toBe(0);
    const items = within(screen.getByTestId("ego-fallback")).getAllByRole("listitem");
    expect(items.length).toBe(22);
    expect(items.filter((li) => li.textContent?.startsWith("capability:")).length).toBe(10);
  });
});

// ---------------------------------------------------------------------------
// U-17 ITERATION SAFETY
// ---------------------------------------------------------------------------

describe("U-17: an actor switch clears the Atlas room", () => {
  it("closes the open sheet and re-renders through the iris", async () => {
    stubAtlasFetch(atlasP060);
    render(<Console view="atlas" />);

    // The shell offers the doors; the reserved one is disabled, not a link.
    // AR-2: the Org Graph is the entry view (href "/"); Ask moved to /ask.
    expect(screen.getByTestId("view-door-graph").getAttribute("href")).toBe("/");
    expect(screen.getByTestId("view-door-ask").getAttribute("href")).toBe("/ask");
    expect(screen.getByTestId("view-door-lens").getAttribute("href")).toBe("/lens");
    expect(screen.getByTestId("view-door-atlas").getAttribute("aria-current")).toBe("page");
    const ledger = screen.getByTestId("ledger-door");
    expect(ledger.getAttribute("aria-disabled")).toBe("true");
    expect(ledger.getAttribute("href")).toBeNull();

    fireEvent.change(screen.getByTestId("principal-search"), { target: { value: "p060" } });
    fireEvent.click(
      screen.getAllByTestId("principal-row").find((b) => b.textContent === "p060")!,
    );
    await waitFor(() => expect(screen.getAllByTestId("strategy-band").length).toBe(6));

    const capabilities = allCapabilities(atlasP060);
    const targetIndex = capabilities.findIndex((c) => c.docs.length > 0);
    const cards = screen.getAllByTestId("capability-card");
    fireEvent.click(within(cards[targetIndex]).getByTestId("capability-open"));
    await screen.findByTestId("capability-sheet");

    // Switch the lens: the sheet is gone SYNCHRONOUSLY (state cleared on
    // remount, before any fetch resolves) — the residue rule.
    fireEvent.change(screen.getByTestId("principal-search"), { target: { value: "p061" } });
    fireEvent.click(
      screen.getAllByTestId("principal-row").find((b) => b.textContent === "p061")!,
    );
    expect(screen.queryByTestId("capability-sheet")).toBeNull();

    await waitFor(() => expect(screen.getAllByTestId("strategy-band").length).toBe(6));
    expect(screen.queryByTestId("capability-sheet")).toBeNull();
  });
});
