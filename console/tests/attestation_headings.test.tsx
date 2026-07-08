/**
 * ATTESTATION (Track B2/B3) — machine heading-order across every routed
 * surface, and the lane route's static (pre-hydration) structure.
 *
 * THE LAW: every routed surface renders exactly ONE h1, and heading levels
 * never skip (an hN may be followed by at most hN+1 when descending).
 * Each surface is rendered in a deterministic state: the identity-less
 * default where that is the exported/prerendered state, and a fixture-fed
 * loaded state where the surface only exists with data (Operating Map).
 */
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

import { Console } from "@/components/Console";
import { ProductHome } from "@/components/ProductHome";
import { LaneRoom } from "@/components/LaneRoom";
import { EntryScreen } from "@/components/EntryScreen";
import { GraphRoom } from "@/components/GraphRoom";
import { BursarSurface } from "@/components/BursarSurface";
import { ProjectSurface } from "@/components/ProjectSurface";
import type { GraphResponse, ProjectWorkflowResponse } from "@/lib/api";

const PIPE_PROV = {
  capability: { id: "cap31", name: "Access Review 31" },
  initiative: { id: "init03", name: "Strengthen Workforce Capability" },
  strategy: { id: "strat01", name: "Workforce Capability" },
  workflow: { id: "wf11", name: "Goods-In Verification 31" },
};
const PIPE_WORKFLOW: ProjectWorkflowResponse = {
  actor_id: "p060",
  capability_id: "cap31",
  demo_identity_mode: true,
  provenance: PIPE_PROV,
  snapshot_version: "snap",
  items: [
    { capability_id: "cap31", dependencies: [], item_id: "box_active", kind: "lane_box", owner_id: "p060", provenance: PIPE_PROV, snapshot_version: "snap", status: "active", title: "Verify goods-in batch 31" },
    { capability_id: "cap31", dependencies: [], item_id: "ar_mine", kind: "access_request", approver_id: "p060", requester_id: "p074", provenance: PIPE_PROV, snapshot_version: "snap", status: "pending", title: "Access request for Access Review 31" },
  ],
};
// SHOWCASE-III: one pending proposal so the heading gate sees the populated
// proposals panel (h2 "Grounded proposals" -> h3 proposal title) too.
const PIPE_PROPOSAL = {
  proposal_id: "wfp_0001",
  proposer_id: "p060",
  capability_id: "cap31",
  approver_id: "p113",
  title: "Onboarding new hires",
  goal: "confidential financial statements",
  drafted_from: "Drafted by a model from documents p060 is authorized to see.",
  boxes: [
    {
      box_index: 0,
      stage: "Next",
      title: "Collect the signed statements",
      description: "Gather the statements.",
      anchors: [{ visible: true, doc_id: "doc_fin_042", quote: "filed quarterly", locator: "doc_fin_042@118" }],
      sources_total: 1,
      sources_outside_view: 0,
    },
  ],
  grounding: { admitted: 1, refused: 0 },
  status: "pending",
  created_ordinal: 1,
  materialized: false,
  snapshot_version: "snap",
};
function stubPipelineFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/workflow/project/cap31")) return new Response(JSON.stringify(PIPE_WORKFLOW), { status: 200 });
      if (url.includes("/workflow/proposals?role=proposer"))
        return new Response(
          JSON.stringify({ actor_id: "p060", demo_identity_mode: true, role: "proposer", proposals: [PIPE_PROPOSAL], snapshot_version: "snap" }),
          { status: 200 },
        );
      if (url.includes("/workflow/proposals?role=approver"))
        return new Response(
          JSON.stringify({ actor_id: "p060", demo_identity_mode: true, role: "approver", proposals: [], snapshot_version: "snap" }),
          { status: 200 },
        );
      return new Response('{"demo_identity_mode":true,"error":"not found"}', { status: 404 });
    }),
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
  cleanup();
});

/** Collect rendered heading levels in DOM order. */
function headingLevels(root: ParentNode = document): number[] {
  return Array.from(root.querySelectorAll("h1, h2, h3, h4, h5, h6")).map((el) =>
    Number(el.tagName.slice(1)),
  );
}

/** Exactly one h1; descending steps never skip a level. */
function assertHeadingDiscipline(surface: string) {
  const levels = headingLevels();
  const h1s = levels.filter((level) => level === 1).length;
  expect(h1s, `${surface}: exactly one h1 (saw ${h1s} in [${levels.join(",")}])`).toBe(1);
  let deepest = 0;
  for (const level of levels) {
    expect(
      level <= deepest + 1,
      `${surface}: heading skips a level (h${deepest} -> h${level} in [${levels.join(",")}])`,
    ).toBe(true);
    deepest = level;
  }
}

const GRAPH: GraphResponse = {
  actor_id: "p060",
  center: { id: "org", label: "Bryremead Distribution Ltd" },
  departments: [{ id: "Finance", label: "Finance", tint_key: "Finance" }],
  people: [
    {
      id: "p060",
      display_name: "Felix Osei",
      title: "Head of Finance",
      department_id: "Finance",
      avatar_ref: "",
      is_self: true,
      ring: "anchor",
    },
  ],
  tools: [],
  sources: [],
  projects: [],
  edges: [{ from: "p060", kind: "member_of", to: "Finance" }],
  snapshot_version: "snap",
};

function stubQuietFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/graph")) return new Response(JSON.stringify(GRAPH), { status: 200 });
      if (url.endsWith("/node/org/summary"))
        return new Response(
          JSON.stringify({
            demo_identity_mode: true,
            id: "org",
            kind: "org",
            name: "Bryremead Distribution Ltd",
          }),
          { status: 200 },
        );
      return new Response('{"demo_identity_mode":true,"error":"not found"}', { status: 404 });
    }),
  );
}

// ---------------------------------------------------------------------------
// B2 — every routed surface
// ---------------------------------------------------------------------------

describe("B2: one h1 and no skipped heading levels on every routed surface", () => {
  it("/ (front door)", () => {
    render(<ProductHome />);
    assertHeadingDiscipline("/");
  });

  it("/ (cinematic entry screen — Showreel Track A)", () => {
    render(<EntryScreen onEnter={() => {}} />);
    assertHeadingDiscipline("/ (entry)");
    // The entry's one h1 IS the wordmark.
    expect(document.querySelector("h1")?.textContent).toBe("Enterprise Brain");
  });

  for (const view of ["ask", "me", "lens", "atlas", "lane", "project"] as const) {
    it(`/${view === "me" ? "me" : view} (identity-less prerender state)`, () => {
      stubQuietFetch();
      render(<Console view={view} />);
      assertHeadingDiscipline(`/${view}`);
    });
  }

  for (const view of ["adminGraph", "adminBursar"] as const) {
    it(`/${view === "adminGraph" ? "admin/graph" : "admin/bursar"} (locked gate state)`, () => {
      stubQuietFetch();
      render(<Console view={view} />);
      assertHeadingDiscipline(view);
    });
  }

  it("/admin/graph — the revealed Operating Map (fixture-fed)", async () => {
    stubQuietFetch();
    render(<GraphRoom actor="p060" adminPreview />);
    await waitFor(() => expect(screen.getByTestId("graph-audit-panel")).toBeTruthy());
    assertHeadingDiscipline("/admin/graph (map)");
    // The room's one h1 IS the surface name.
    expect(document.querySelector("h1")?.textContent).toBe("Operating Map");
  });

  it("/admin/graph — the revealed map's EMPTY state still carries one h1", () => {
    render(<GraphRoom actor={null} adminPreview />);
    assertHeadingDiscipline("/admin/graph (empty map)");
  });

  it("/admin/bursar — the revealed Spend Ledger (honest STATE 3)", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("producer down", { status: 503 })));
    render(<BursarSurface />);
    await waitFor(() => expect(screen.getByTestId("bursar-unavailable")).toBeTruthy());
    assertHeadingDiscipline("/admin/bursar (ledger)");
  });

  it("the Pipeline projects room keeps heading discipline with its detail drawer OPEN", async () => {
    // h1 (project title) -> h2 (five stage names) -> h2 (drawer title) -> h3
    // (drawer sections). One h1, no skips — even with the drawer open. The
    // populated surface is driven directly (the routed identity-less state is
    // the entry card, covered above).
    stubPipelineFetch();
    render(<ProjectSurface actor="p060" capabilityId="cap31" />);
    await waitFor(() => expect(screen.getByTestId("pipeline-board")).toBeTruthy());
    fireEvent.click(screen.getAllByTestId("pipeline-card")[0]);
    expect(screen.getByTestId("pipeline-drawer")).toBeTruthy();
    assertHeadingDiscipline("/project (pipeline + open drawer)");
  });

  it("the Settings drawer keeps heading discipline when OPEN (Track A's new shell modal)", () => {
    // The gear drawer is a brand-new modal with its own h2 + three h3 sections.
    // It is mounted AFTER the #main surface, so the surface's h1 still comes
    // first and the levels stay [1,2,3,3,3] — no skip, one h1 — even open.
    stubQuietFetch();
    render(<Console view="ask" />);
    fireEvent.click(screen.getByTestId("settings-open"));
    expect(screen.getByTestId("settings-drawer")).toBeTruthy();
    assertHeadingDiscipline("/ask + open settings drawer");
  });
});

// ---------------------------------------------------------------------------
// B3 — the lane route's static structure (what next export prerenders)
// ---------------------------------------------------------------------------

describe("B3: the lane route ships real structure pre-hydration", () => {
  it("the identity-less LaneRoom (the exported shell) carries h1 + section h2", () => {
    render(<LaneRoom actor={null} />);
    const levels = headingLevels();
    expect(levels[0]).toBe(1);
    expect(levels).toContain(2);
    expect(document.querySelector("h1")?.textContent).toBe("Review Queue");
    expect(
      Array.from(document.querySelectorAll("h2")).some((h) =>
        (h.textContent ?? "").includes("Inbox"),
      ),
    ).toBe(true);
    assertHeadingDiscipline("/lane (static shell)");
  });
});
