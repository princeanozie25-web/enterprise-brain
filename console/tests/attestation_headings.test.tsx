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
import { cleanup, render, screen, waitFor } from "@testing-library/react";

import { Console } from "@/components/Console";
import { ProductHome } from "@/components/ProductHome";
import { LaneRoom } from "@/components/LaneRoom";
import { GraphRoom } from "@/components/GraphRoom";
import { BursarSurface } from "@/components/BursarSurface";
import type { GraphResponse } from "@/lib/api";

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
