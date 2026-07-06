/**
 * GRAPH PRESENCE (Track A) — the layout law, staged edges, presence, and
 * motion, pinned. Fully offline: typed /graph fixtures, OrgGraph rendered
 * directly; GraphRoom exercised with a stubbed fetch for the masthead rule.
 */
import React, { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import type { GraphResponse } from "@/lib/api";
import { GEOMETRY } from "@/lib/tokens";
import { OrgGraph, type SelectedNode } from "@/components/OrgGraph";
import { GraphRoom } from "@/components/GraphRoom";
import { Console } from "@/components/Console";
import { peoplePlural } from "@/components/graphDisplay";
import { DemoIdentityNotice } from "@/components/TrustPosture";

afterEach(() => {
  vi.unstubAllGlobals();
});

const GRAPH: GraphResponse = {
  actor_id: "p060",
  center: { id: "org", label: "Bryremead Distribution Ltd" },
  departments: [
    { id: "Finance", label: "Finance", tint_key: "Finance" },
    { id: "IT", label: "IT", tint_key: "IT" },
    { id: "HR", label: "HR", tint_key: "HR" },
  ],
  people: [
    { id: "p060", display_name: "Felix Osei", title: "Head of Finance", department_id: "Finance", avatar_ref: "", is_self: true, ring: "anchor" },
    { id: "p061", display_name: "Ana Flores", title: "Accounts Payable Clerk", department_id: "Finance", avatar_ref: "", is_self: false, ring: "member" },
    { id: "p074", display_name: "Yuki Moreau", title: "Head of IT", department_id: "IT", avatar_ref: "", is_self: false, ring: "anchor" },
    { id: "p088", display_name: "Tomas Iqbal", title: "HR Associate", department_id: "HR", avatar_ref: "", is_self: false, ring: "member" },
  ],
  tools: [{ id: "agent_finance_analyst", label: "Finance analysis assistant", kind: "agent", department_id: "Finance" }],
  sources: [{ id: "docstore", kind: "source", label: "Document store" }],
  projects: [
    {
      departments: ["Finance"],
      id: "cap31",
      initiative_name: "Strengthen Workforce Capability",
      label: "Capability: Access Review 31",
      people: 2,
      primary_department_id: "Finance",
      status_counts: { Active: 1 },
      strategy_name: "Strategy: Workforce Capability",
      workflow_name: "Workflow: Goods-In Verification 31",
    },
  ],
  edges: [
    { from: "p060", kind: "member_of", to: "Finance" },
    { from: "p061", kind: "member_of", to: "Finance" },
    { from: "p074", kind: "member_of", to: "IT" },
    { from: "p088", kind: "member_of", to: "HR" },
    { from: "p061", kind: "reports_to", to: "p060" },
    { from: "p060", kind: "owns_agent", to: "agent_finance_analyst" },
    { from: "docstore", kind: "system_of", to: "org" },
    { from: "p060", kind: "works_on", to: "cap31" },
    { from: "cap31", kind: "involves_department", to: "Finance" },
  ],
  snapshot_version: "snap",
};

function renderGraph(overrides: Partial<React.ComponentProps<typeof OrgGraph>> = {}) {
  return render(
    <OrgGraph graph={GRAPH} onSelectNode={() => {}} onFocusDept={() => {}} {...overrides} />,
  );
}
function nodeById(testid: string, id: string): HTMLElement {
  return screen.getAllByTestId(testid).find((el) => el.getAttribute("data-id") === id)!;
}
function translateOf(el: HTMLElement): { x: number; y: number } {
  const match = /translate\(([-\d.]+),([-\d.]+)\)/.exec(el.getAttribute("transform") ?? "");
  return match ? { x: Number(match[1]), y: Number(match[2]) } : { x: 0, y: 0 };
}
const dist = (a: { x: number; y: number }, b: { x: number; y: number }) =>
  Math.hypot(a.x - b.x, a.y - b.y);

// ---------------------------------------------------------------------------
// GP-1 THE LAYOUT LAW (A1): radii and node sizes per tier
// ---------------------------------------------------------------------------

describe("GP-1: the layout law — hub ring 240±1, member orbit 96±1, tier sizes", () => {
  it("places hubs at 240 from center and members at 96 from their OWN hub", () => {
    renderGraph();
    const origin = { x: 0, y: 0 };
    for (const dept of GRAPH.departments) {
      const hub = translateOf(nodeById("graph-dept", dept.id));
      expect(Math.abs(dist(hub, origin) - 240)).toBeLessThanOrEqual(1);
    }
    for (const person of GRAPH.people) {
      const hub = translateOf(nodeById("graph-dept", person.department_id));
      const node = translateOf(nodeById("graph-person", person.id));
      expect(Math.abs(dist(node, hub) - 96)).toBeLessThanOrEqual(1);
    }
  });

  it("sizes each tier to the law: center 56, hubs 48, people 36, sources 28, projects 24", () => {
    renderGraph();
    // Center: a 56px square.
    const centerRect = screen.getByTestId("graph-center").querySelector("rect")!;
    expect(centerRect.getAttribute("width")).toBe("56");
    // Hub: 48px circle (r=24).
    const hubCircle = nodeById("graph-dept", "Finance").querySelector("circle")!;
    expect(hubCircle.getAttribute("r")).toBe("24");
    // Person: a 36px avatar viewport.
    const avatarBox = nodeById("graph-person", "p060").querySelector("foreignObject")!;
    expect(avatarBox.getAttribute("width")).toBe("36");
    // Source: 28px (r=14 inner disc; the halo ring sits at r+5).
    const sourceDiscs = nodeById("graph-source", "docstore").querySelectorAll("circle");
    expect(Array.from(sourceDiscs).some((c) => c.getAttribute("r") === "14")).toBe(true);
    // Project: 24px diamond (half-size 12 path).
    const diamond = nodeById("graph-project", "cap31").querySelector("path")!;
    expect(diamond.getAttribute("d")).toContain("M0 -12");
  });

  it("distributes hubs evenly and keeps sibling members ≥24px apart edge-to-edge", () => {
    renderGraph();
    const p060 = translateOf(nodeById("graph-person", "p060"));
    const p061 = translateOf(nodeById("graph-person", "p061"));
    // 36px nodes: center-to-center ≥ 36 + 24.
    expect(dist(p060, p061)).toBeGreaterThanOrEqual(60 - 0.5);
  });

  it("ring 2 rests at 80% opacity (depth = scale + opacity, no blur, no glass)", () => {
    const { container } = renderGraph();
    expect(nodeById("graph-source", "docstore").getAttribute("opacity")).toBe("0.8");
    expect(container.querySelector("filter")).toBeNull();
    expect(container.innerHTML).not.toContain("backdrop-filter");
  });
});

// ---------------------------------------------------------------------------
// GP-2 STAGED EDGES (A2): ego focus, persistence, click-away
// ---------------------------------------------------------------------------

/** A stateful harness: selection persistence is room state; the graph is
 * controlled — this mirrors GraphRoom's wiring without its fetches. */
function Harness() {
  const [selected, setSelected] = useState<SelectedNode | null>(null);
  return (
    <OrgGraph
      graph={GRAPH}
      onSelectNode={setSelected}
      onFocusDept={() => {}}
      selectedId={selected?.id ?? null}
    />
  );
}

describe("GP-2: hover lights the payload edge set; selection persists; Escape/click-away release", () => {
  it("focusing a node (hover's keyboard twin) lights its edges and dims non-connected nodes to 15%", () => {
    renderGraph();
    // onFocus mirrors hover by design (A2): the ego cue is one state.
    fireEvent.focus(nodeById("graph-person", "p060"));
    const lit = screen
      .getAllByTestId("graph-edge")
      .filter((e) => e.getAttribute("data-lit") === "true");
    const litKinds = new Set(lit.map((e) => e.getAttribute("data-kind")));
    for (const kind of ["member_of", "reports_to", "owns_agent", "works_on"]) {
      expect(litKinds.has(kind)).toBe(true);
    }
    // Non-connected: p088 (HR) dims to the 15% token.
    expect(nodeById("graph-person", "p088").getAttribute("opacity")).toBe(
      String(GEOMETRY.graphDimOpacity),
    );
    // Leaving restores rest: chords vanish, structural edges remain.
    fireEvent.blur(nodeById("graph-person", "p060"));
    const kinds = new Set(
      screen.getAllByTestId("graph-edge").map((e) => e.getAttribute("data-kind")),
    );
    expect(kinds.has("works_on")).toBe(false);
    expect(nodeById("graph-person", "p088").getAttribute("opacity")).toBe("1");
  });

  it("selection persists the focus; Escape releases it", () => {
    render(<Harness />);
    fireEvent.click(nodeById("graph-person", "p060"));
    // Persisted: chords stay lit with no hover.
    expect(
      screen.getAllByTestId("graph-edge").some((e) => e.getAttribute("data-kind") === "works_on"),
    ).toBe(true);
    fireEvent.keyDown(nodeById("graph-person", "p060"), { key: "Escape" });
    expect(
      screen.getAllByTestId("graph-edge").some((e) => e.getAttribute("data-kind") === "works_on"),
    ).toBe(false);
  });

  it("click-away on the stage background releases the persisted focus", () => {
    render(<Harness />);
    fireEvent.click(nodeById("graph-person", "p060"));
    expect(
      screen.getAllByTestId("graph-edge").some((e) => e.getAttribute("data-kind") === "works_on"),
    ).toBe(true);
    fireEvent.click(screen.getByLabelText("Organization graph"));
    expect(
      screen.getAllByTestId("graph-edge").some((e) => e.getAttribute("data-kind") === "works_on"),
    ).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// GP-3 PRESENCE (A3): pluralize + neutral-ramp avatars
// ---------------------------------------------------------------------------

describe("GP-3: people counts pluralize honestly; department identity rides the neutral ramp", () => {
  it("peoplePlural: 1 person / 14 people, everywhere counts render", () => {
    expect(peoplePlural(1)).toBe("1 person");
    expect(peoplePlural(14)).toBe("14 people");
    renderGraph();
    // HR has exactly one member: the hub sub-label must say "1 person".
    expect(within(nodeById("graph-dept", "HR")).getByTestId("graph-dept-count").textContent).toBe(
      "1 person",
    );
    expect(
      within(nodeById("graph-dept", "Finance")).getByTestId("graph-dept-count").textContent,
    ).toBe("2 people");
  });

  it("person nodes pass the neutral-ramp tint to PersonAvatar (no saturated department hues)", () => {
    // OrgGraph hands the ramp tint to PersonAvatar; the monogram (the tinted
    // surface) appears when the face image errors. Simulate that fallback.
    renderGraph();
    const container = nodeById("graph-person", "p088");
    const img = within(container).getByTestId("person-avatar-img");
    fireEvent.error(img);
    const monogram = within(container).getByTestId("person-avatar-monogram");
    const style = monogram.getAttribute("style") ?? "";
    // The neutral ramp is ink-over-paper color-mix — never an Okabe–Ito hex.
    expect(style).toContain("color-mix");
    expect(style).not.toMatch(/#0072B2|#009E73|#E69F00|#D55E00|#CC79A7/i);
  });

  it("the sr mirror regroups by department: one list per hub, members inside", () => {
    renderGraph();
    const deptLists = screen.getAllByTestId("graph-sr-dept");
    expect(deptLists.length).toBe(GRAPH.departments.length);
    const finance = deptLists.find((ul) => ul.getAttribute("aria-label") === "Finance department")!;
    expect(finance.textContent).toContain("Felix Osei");
    expect(finance.textContent).toContain("Ana Flores");
    expect(finance.textContent).not.toContain("Yuki Moreau");
    expect(finance.textContent).toContain(`${peoplePlural(2)} in scope`);
  });
});

// ---------------------------------------------------------------------------
// GP-4 MOTION (A4): one transition, dead under prefers-reduced-motion
// ---------------------------------------------------------------------------

describe("GP-4: the focus/dim transition is ≤200ms ease-out and dead under reduced motion", () => {
  it("nodes and edges carry the 180ms ease-out transition by default", () => {
    renderGraph();
    expect(nodeById("graph-person", "p060").getAttribute("style") ?? "").toContain(
      "opacity 180ms ease-out",
    );
    const edge = screen.getAllByTestId("graph-edge")[0];
    expect(edge.getAttribute("style") ?? "").toContain("stroke-opacity 180ms ease-out");
  });

  it("reduced motion strips every transition — instant state swap", () => {
    renderGraph({ reducedMotion: true });
    expect(nodeById("graph-person", "p060").getAttribute("style") ?? "").not.toContain("transition");
    for (const edge of screen.getAllByTestId("graph-edge")) {
      expect(edge.getAttribute("style") ?? "").not.toContain("transition");
    }
  });
});

// ---------------------------------------------------------------------------
// GP-5 F6 RE-PIN: real entities only, against the NEW layout
// ---------------------------------------------------------------------------

describe("GP-5 (F6 law re-pin): every rendered node and edge maps to the payload", () => {
  it("every data-id is a payload entity; every edge joins payload endpoints", () => {
    const { container } = renderGraph({ selectedId: "p060" }); // chords lit too
    const payloadIds = new Set<string>([
      GRAPH.center.id,
      ...GRAPH.departments.map((d) => d.id),
      ...GRAPH.people.map((p) => p.id),
      ...GRAPH.tools.map((t) => t.id),
      ...GRAPH.sources.map((s) => s.id),
      ...GRAPH.projects.map((p) => p.id),
    ]);
    for (const el of Array.from(container.querySelectorAll("[data-id]"))) {
      expect(payloadIds.has(el.getAttribute("data-id")!)).toBe(true);
    }
    const payloadEdges = new Set(GRAPH.edges.map((e) => `${e.from}|${e.kind}|${e.to}`));
    const rendered = screen.getAllByTestId("graph-edge");
    expect(rendered.length).toBeGreaterThan(0);
    // EVERY rendered edge IS a payload edge, endpoint for endpoint.
    for (const edge of rendered) {
      const key = `${edge.getAttribute("data-from")}|${edge.getAttribute("data-kind")}|${edge.getAttribute("data-to")}`;
      expect(payloadEdges.has(key), `rendered edge ${key} is not in the payload`).toBe(true);
    }
    expect(rendered.length).toBeLessThanOrEqual(GRAPH.edges.length);
  });

  it("a focused SOURCE lights its system_of chord (ring-2 edges stay covered)", () => {
    renderGraph({ selectedId: "docstore" });
    const lit = screen
      .getAllByTestId("graph-edge")
      .filter((e) => e.getAttribute("data-lit") === "true");
    expect(lit.some((e) => e.getAttribute("data-kind") === "system_of")).toBe(true);
  });

  it("selecting the CENTER persists like any other node and lights the org spine", () => {
    renderGraph({ selectedId: "org" });
    const lit = screen
      .getAllByTestId("graph-edge")
      .filter((e) => e.getAttribute("data-lit") === "true");
    // The center's payload edge set: the system_of spine (dept member_of
    // edges to org do not exist in this fixture's people-only member_of).
    expect(lit.some((e) => e.getAttribute("data-kind") === "system_of")).toBe(true);
    // Non-connected nodes dim — the persistence law holds for the center too.
    expect(nodeById("graph-person", "p088").getAttribute("opacity")).toBe(
      String(GEOMETRY.graphDimOpacity),
    );
  });

  it("p_void-shaped worlds stay whitespace: zero people means zero member nodes", () => {
    const empty: GraphResponse = {
      ...GRAPH,
      people: [],
      departments: [],
      tools: [],
      sources: [],
      projects: [],
      edges: [],
    };
    render(<OrgGraph graph={empty} onSelectNode={() => {}} onFocusDept={() => {}} />);
    expect(screen.queryAllByTestId("graph-person").length).toBe(0);
    expect(screen.queryAllByTestId("graph-dept").length).toBe(0);
    expect(screen.queryAllByTestId("graph-edge").length).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// GP-5b CHIP MODE (review findings): ego, filter, and scale-to-fit honesty
// ---------------------------------------------------------------------------

function manyPeople(count: number, dept: (i: number) => string) {
  return Array.from({ length: count }, (_, i) => ({
    id: `p${String(i + 1).padStart(3, "0")}`,
    display_name: `Person ${i + 1}`,
    title: "Analyst",
    department_id: dept(i),
    avatar_ref: "",
    is_self: i === 0,
    ring: "member" as const,
  }));
}

/** >40 people: every department chips (the U-40 threshold law). */
const CLUSTERED: GraphResponse = {
  ...GRAPH,
  departments: [
    { id: "Finance", label: "Finance", tint_key: "Finance" },
    { id: "IT", label: "IT", tint_key: "IT" },
  ],
  people: manyPeople(42, (i) => (i % 2 === 0 ? "Finance" : "IT")),
  tools: [],
  sources: [],
  projects: [],
  edges: [],
};

/** ≤40 people but ONE department deeper than two lawful orbit rows: that
 * department chips; the small one still fans. */
const OVERFLOW: GraphResponse = {
  ...GRAPH,
  departments: [
    { id: "Finance", label: "Finance", tint_key: "Finance" },
    { id: "IT", label: "IT", tint_key: "IT" },
  ],
  people: manyPeople(25, (i) => (i < 24 ? "Finance" : "IT")),
  tools: [],
  sources: [],
  projects: [],
  edges: manyPeople(25, (i) => (i < 24 ? "Finance" : "IT")).map((p) => ({
    from: p.id,
    kind: "member_of" as const,
    to: p.department_id,
  })),
};

describe("GP-5b: cluster chips obey the ego, filter, tier, and scale laws", () => {
  it("focusing a chip emphasizes it (never a whole-map dim with an invisible ego)", () => {
    render(<OrgGraph graph={CLUSTERED} onSelectNode={() => {}} onFocusDept={() => {}} />);
    const chip = screen
      .getAllByTestId("graph-people-cluster")
      .find((c) => c.getAttribute("data-id") === "Finance")!;
    fireEvent.focus(chip);
    // The chip and its hub stay fully emphasized under their own focus.
    expect(chip.getAttribute("opacity")).toBe("1");
    expect(nodeById("graph-dept", "Finance").getAttribute("opacity")).toBe("1");
  });

  it("the People filter hides chips exactly as it hides fans (no silent no-op)", () => {
    render(
      <OrgGraph
        graph={CLUSTERED}
        onSelectNode={() => {}}
        onFocusDept={() => {}}
        hiddenKinds={["people"]}
      />,
    );
    expect(screen.queryAllByTestId("graph-people-cluster").length).toBe(0);
    expect(screen.queryAllByTestId("graph-person").length).toBe(0);
  });

  it("chip mode sizes the viewBox for what is DRAWN, not for invisible fans", () => {
    render(<OrgGraph graph={CLUSTERED} onSelectNode={() => {}} onFocusDept={() => {}} />);
    const viewBox = screen.getByLabelText("Organization graph").getAttribute("viewBox")!;
    const bound = Math.abs(Number(viewBox.split(" ")[0]));
    // Chips reach hub(240) + orbit(96) + chip half(22); +84 margin ≈ 442 —
    // far below a fan-inflated bound.
    expect(bound).toBeLessThanOrEqual(460);
  });

  it("a department deeper than two orbit rows chips; member rows never pierce ring 2", () => {
    render(<OrgGraph graph={OVERFLOW} onSelectNode={() => {}} onFocusDept={() => {}} />);
    // Finance (24 members > two-row capacity) is an honest chip…
    const chips = screen.getAllByTestId("graph-people-cluster");
    expect(chips.length).toBe(1);
    expect(chips[0].getAttribute("data-id")).toBe("Finance");
    expect(Number(chips[0].getAttribute("data-count"))).toBe(24);
    // …its members draw no nodes and no edges…
    expect(screen.getAllByTestId("graph-person").length).toBe(1); // IT's one
    expect(screen.getAllByTestId("graph-edge").length).toBe(1); // IT's spoke
    // …but the SR mirror still names every real member.
    const mirror = screen.getByTestId("graph-sr-mirror");
    expect(mirror.textContent).toContain("Person 1");
    expect(mirror.textContent).toContain("Person 24");
  });
});

// ---------------------------------------------------------------------------
// GP-6 THE MASTHEAD RULE (A2): disclosed totals never shrink
// ---------------------------------------------------------------------------

describe("GP-6: the masthead keeps the FULL payload edge count with the focus sub-label", () => {
  it("Connections (N) counts the whole payload; 'shown on focus' names the staging", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/graph")) return new Response(JSON.stringify(GRAPH), { status: 200 });
      if (url.endsWith("/node/org/summary"))
        return new Response(
          JSON.stringify({ demo_identity_mode: true, id: "org", kind: "org", name: "Bryremead Distribution Ltd" }),
          { status: 200 },
        );
      return new Response('{"demo_identity_mode":true,"error":"not found"}', { status: 404 });
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<GraphRoom actor="p060" />);
    await waitFor(() => expect(screen.getByTestId("graph-connections-sublabel")).toBeTruthy());
    expect(screen.getByTestId("graph-room").textContent).toContain(
      `Connections (${GRAPH.edges.length})`,
    );
    expect(screen.getByTestId("graph-connections-sublabel").textContent).toBe("shown on focus");
    // The rendered rest-state edges are FEWER than the disclosed total —
    // the disclosure, not the render, carries the truth.
    expect(screen.getAllByTestId("graph-edge").length).toBeLessThan(GRAPH.edges.length);
  });
});

// ---------------------------------------------------------------------------
// GP-7 (B5): the view-as disclosure line
// ---------------------------------------------------------------------------

describe("GP-7 (B5): the view-as disclosure renders once, calm register", () => {
  it("names the demo view-as rule and its audit, without error styling", () => {
    render(<DemoIdentityNotice />);
    const line = screen.getByTestId("view-as-disclosure");
    expect(line.textContent).toBe(
      "In demo mode, any identity may view-as any other — every view-as is audited before render.",
    );
    expect(line.className).toContain("ap-soft");
    expect(line.getAttribute("role")).toBeNull();
  });

  it("a routed surface carries the line EXACTLY once (the per-page law, not the component)", () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response('{"demo_identity_mode":true}', { status: 404 })),
    );
    const { unmount } = render(<Console view="ask" />);
    expect(screen.getAllByTestId("view-as-disclosure").length).toBe(1);
    unmount();
    render(<Console view="lane" />);
    expect(screen.getAllByTestId("view-as-disclosure").length).toBe(1);
  });
});
