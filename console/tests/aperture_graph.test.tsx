/**
 * Org Brain tests U-33..U-48. Fully offline: small typed /graph + node-summary
 * fixtures; OrgGraph rendered directly (deterministic polar layout, React owns
 * the DOM); GraphRoom/Sidebar/Inspector exercised with a stubbed fetch.
 */
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import type { GraphResponse, NodeSummary, OrgStats } from "@/lib/api";
import { GEOMETRY } from "@/lib/tokens";
import { OrgGraph } from "@/components/OrgGraph";
import { GraphRoom, lensHref } from "@/components/GraphRoom";
import { GraphSidebar } from "@/components/GraphSidebar";
import { GraphInspector } from "@/components/GraphInspector";

afterEach(() => {
  vi.unstubAllGlobals();
});

const GRAPH: GraphResponse = {
  actor_id: "p060",
  center: { id: "org", label: "Bryremead Distribution Ltd" },
  departments: [
    { id: "Finance", label: "Finance", tint_key: "Finance" },
    { id: "IT", label: "IT", tint_key: "IT" },
  ],
  people: [
    { id: "p060", display_name: "Felix Osei", title: "Head of Finance", department_id: "Finance", avatar_ref: "faces/p060.jpg", is_self: true, ring: "anchor" },
    { id: "p061", display_name: "Ana Flores", title: "Accounts Payable Clerk", department_id: "Finance", avatar_ref: "faces/p061.jpg", is_self: false, ring: "member" },
    { id: "p074", display_name: "Yuki Moreau", title: "Head of IT", department_id: "IT", avatar_ref: "faces/p074.jpg", is_self: false, ring: "anchor" },
    { id: "p075", display_name: "Mei Kim", title: "Service Desk Analyst", department_id: "IT", avatar_ref: "faces/p075.jpg", is_self: false, ring: "member" },
  ],
  tools: [{ id: "agent_finance_analyst", label: "Finance analysis assistant", kind: "agent", department_id: "Finance" }],
  sources: [
    { id: "docstore", kind: "source", label: "Document store" },
    { id: "wiki", kind: "source", label: "Wiki" },
  ],
  projects: [
    {
      departments: ["Finance", "IT"],
      id: "cap31",
      initiative_name: "Strengthen Workforce Capability",
      label: "Capability: Access Review 31",
      people: 2,
      primary_department_id: "Finance",
      status_counts: { Active: 1, Planned: 1 },
      strategy_name: "Strategy: Workforce Capability",
      workflow_name: "Workflow: Goods-In Verification 31",
    },
  ],
  edges: [
    { from: "p060", kind: "member_of", to: "Finance" },
    { from: "p061", kind: "member_of", to: "Finance" },
    { from: "p074", kind: "member_of", to: "IT" },
    { from: "p075", kind: "member_of", to: "IT" },
    { from: "p061", kind: "reports_to", to: "p060" },
    { from: "p075", kind: "reports_to", to: "p074" },
    { from: "Finance", kind: "member_of", to: "org" },
    { from: "IT", kind: "member_of", to: "org" },
    { from: "p060", kind: "owns_agent", to: "agent_finance_analyst" },
    { from: "docstore", kind: "system_of", to: "org" },
    { from: "wiki", kind: "system_of", to: "org" },
    { from: "p060", kind: "works_on", to: "cap31" },
    { from: "p075", kind: "works_on", to: "cap31" },
    { from: "cap31", kind: "involves_department", to: "Finance" },
    { from: "cap31", kind: "involves_department", to: "IT" },
  ],
  snapshot_version: "snap",
};

const MINIMAL: GraphResponse = {
  actor_id: "p046",
  center: { id: "org", label: "Bryremead Distribution Ltd" },
  departments: [{ id: "Pharmacy Services", label: "Pharmacy Services", tint_key: "Pharmacy Services" }],
  people: [
    { id: "p046", display_name: "Bao Costa", title: "Responsible Pharmacist", department_id: "Pharmacy Services", avatar_ref: "faces/p046.jpg", is_self: true, ring: "member" },
  ],
  tools: [],
  sources: [],
  projects: [],
  edges: [{ from: "p046", kind: "member_of", to: "Pharmacy Services" }],
  snapshot_version: "snap",
};

const STATS: OrgStats = {
  agents: 4, capabilities: 90, departments: 8, document_total: 600, groups: 14,
  initiatives: 18, people: 120, permission_edges: 16881, principals: 124, sites: 2,
  sources: 5, strategies: 6, total_decisions: 74400, workflows: 40,
};

function renderGraph(overrides: Partial<React.ComponentProps<typeof OrgGraph>> = {}) {
  return render(
    <OrgGraph graph={GRAPH} onSelectNode={() => {}} onFocusDept={() => {}} {...overrides} />,
  );
}
function personNode(id: string): HTMLElement {
  return screen.getAllByTestId("graph-person").find((el) => el.getAttribute("data-id") === id)!;
}
/** A2 rest law: only structural person→department edges render at rest. */
function restEdges(graph: GraphResponse): number {
  const people = new Set(graph.people.map((p) => p.id));
  const depts = new Set(graph.departments.map((d) => d.id));
  return graph.edges.filter(
    (edge) => edge.kind === "member_of" && people.has(edge.from) && depts.has(edge.to),
  ).length;
}

// ===========================================================================
// SHOWCASE-1 (Track B) — THE REFERENCE OPERATING MAP. The OrgGraph rebuild.
// These supersede the graph-presence radial-cluster suite (U-33..U-45): the
// rest-state is now the deep-navy rim map — hubs, department arcs, an 8px
// people rim, promoted heads, dotted-amber hub edges, rim-spoke texture. The
// GP interaction laws survive and are re-pinned below.
// ===========================================================================

describe("REF-1: the map renders center, hubs, sources, agents, and every person once", () => {
  it("renders one node per payload entity — members are rim dots, heads are promoted, NO duplicates, NO project nodes", () => {
    renderGraph();
    const members = GRAPH.people.filter((p) => p.ring !== "anchor");
    const heads = GRAPH.people.filter((p) => p.ring === "anchor");
    expect(screen.getByTestId("org-graph")).toBeTruthy();
    expect(screen.getByTestId("graph-center")).toBeTruthy();
    expect(screen.getByTestId("graph-canvas")).toBeTruthy();
    expect(screen.getAllByTestId("graph-dept").length).toBe(GRAPH.departments.length);
    expect(screen.getAllByTestId("graph-source").length).toBe(GRAPH.sources.length);
    expect(screen.getAllByTestId("graph-tool").length).toBe(GRAPH.tools.length);
    // A head is drawn ONCE (as a promoted avatar), never also as a rim dot.
    expect(screen.getAllByTestId("graph-person").length).toBe(members.length);
    expect(screen.getAllByTestId("graph-head").length).toBe(heads.length);
    // No person id is both a dot AND a head (no duplicate tab stop / marker).
    const dotIds = screen.getAllByTestId("graph-person").map((el) => el.getAttribute("data-id"));
    const headIds = screen.getAllByTestId("graph-head").map((el) => el.getAttribute("data-id"));
    expect(dotIds.filter((id) => headIds.includes(id))).toEqual([]);
    expect(screen.queryAllByTestId("graph-project").length).toBe(0);
    expect(screen.getAllByTestId("graph-dept-arc").length).toBe(GRAPH.departments.length);
  });
});

describe("REF-2: heads are promoted; members are dots (name on focus, not always)", () => {
  it("each department's anchor renders a promoted head with an always-on name", () => {
    renderGraph();
    const heads = screen.getAllByTestId("graph-head");
    expect(heads.length).toBe(2);
    const headNames = heads.map((h) => within(h).getByTestId("graph-head-name").textContent);
    expect(headNames).toContain("Felix Osei");
    expect(headNames).toContain("Yuki Moreau");
    expect(within(personNode("p061")).queryByTestId("graph-person-name")).toBeNull();
  });
});

describe("REF-3: the layout law — hub ring 210, people rim 430, arc ring 448", () => {
  it("places hubs, people, and arcs at the reference radii", () => {
    renderGraph();
    const dist = (el: HTMLElement) => {
      const m = /translate\(([-\d.]+),([-\d.]+)\)/.exec(el.getAttribute("transform") ?? "");
      return m ? Math.hypot(Number(m[1]), Number(m[2])) : NaN;
    };
    for (const d of GRAPH.departments) {
      const hub = screen.getAllByTestId("graph-dept").find((x) => x.getAttribute("data-id") === d.id)!;
      expect(Math.abs(dist(hub) - 210)).toBeLessThanOrEqual(1);
    }
    // Members ride the 430 rim; heads are promoted off it (452), so only the
    // non-anchor dots are checked here.
    for (const person of GRAPH.people.filter((p) => p.ring !== "anchor")) {
      expect(Math.abs(dist(personNode(person.id)) - 430)).toBeLessThanOrEqual(1);
    }
    for (const arc of screen.getAllByTestId("graph-dept-arc")) {
      expect(arc.getAttribute("d")).toMatch(/A448,448/);
    }
  });
});

describe("REF-4: departments ride the pastel family; center is the glowing core", () => {
  it("arc strokes carry a pastel hex (never a sensitivity hue); the core glows, no blur", () => {
    const { container } = renderGraph();
    const arc = screen.getAllByTestId("graph-dept-arc")[0];
    const stroke = arc.getAttribute("stroke") ?? "";
    expect(stroke).toMatch(/^#[0-9A-Fa-f]{6}$/);
    expect(stroke).not.toMatch(/#0072B2|#009E73|#E69F00|#D55E00|#CC79A7/i);
    expect(container.querySelector("#og-core-glow")).toBeTruthy();
    expect(container.querySelector("filter")).toBeNull();
  });
});

describe("REF-5 (GP ego, adapted): rest is rim spokes + dotted-amber hub edges; focus lights the payload set", () => {
  it("at rest, chords are staged away; hub edges + rim spokes carry the texture", () => {
    renderGraph();
    const spokes = screen.getAllByTestId("graph-rim-spoke");
    expect(spokes.length).toBe(GRAPH.people.length);
    for (const s of spokes) {
      expect(s.getAttribute("data-kind")).toBe("member_of");
      expect(s.getAttribute("stroke-width")).toBe("0.5");
    }
    const edges = screen.getAllByTestId("graph-edge");
    for (const e of edges) {
      expect(e.getAttribute("data-hub")).toBe("true");
      expect(e.getAttribute("stroke-dasharray")).toBe("1 5");
    }
    const kinds = new Set(edges.map((e) => e.getAttribute("data-kind")));
    expect(kinds.has("reports_to")).toBe(false);
    expect(kinds.has("works_on")).toBe(false);
  });

  it("selecting a node lights its full payload edge set at 1.5px/100%; others dim to 15%", () => {
    renderGraph({ selectedId: "p060" });
    const lit = screen.getAllByTestId("graph-edge").filter((e) => e.getAttribute("data-lit") === "true");
    const litKinds = new Set(lit.map((e) => e.getAttribute("data-kind")));
    for (const kind of ["reports_to", "owns_agent", "works_on"]) {
      expect(litKinds.has(kind)).toBe(true);
    }
    for (const e of lit) {
      expect(e.getAttribute("stroke-width")).toBe("1.5");
      expect(e.getAttribute("stroke-opacity")).toBe("1");
    }
    expect(personNode("p075").getAttribute("opacity")).toBe(String(GEOMETRY.graphDimOpacity));
  });
});

describe("REF-6: self marker, minimal world, keyboard, and F6 re-pin", () => {
  it("marks the actor's own node 'you are here'", () => {
    renderGraph();
    const p060Head = screen.getAllByTestId("graph-head").find((h) => h.getAttribute("data-id") === "p060")!;
    expect(within(p060Head).getByTestId("graph-self-marker")).toBeTruthy();
  });

  it("a minimal one-person world stays small — no padding, no ghosts", () => {
    const { container } = render(<OrgGraph graph={MINIMAL} onSelectNode={() => {}} onFocusDept={() => {}} />);
    expect(screen.getAllByTestId("graph-person").length).toBe(1);
    expect(screen.getAllByTestId("graph-dept").length).toBe(1);
    expect(screen.queryAllByTestId("graph-source").length).toBe(0);
    expect(container.textContent ?? "").not.toMatch(/\+\d+\b/);
  });

  it("p_void-shaped world: whitespace + no nodes", () => {
    const empty: GraphResponse = { ...GRAPH, people: [], departments: [], tools: [], sources: [], projects: [], edges: [] };
    render(<OrgGraph graph={empty} onSelectNode={() => {}} onFocusDept={() => {}} />);
    expect(screen.queryAllByTestId("graph-person").length).toBe(0);
    expect(screen.queryAllByTestId("graph-dept").length).toBe(0);
    expect(screen.queryAllByTestId("graph-edge").length).toBe(0);
  });

  it("is keyboard-operable: nodes are tab stops, Enter activates, Escape climbs to the hub", () => {
    const onSelectNode = vi.fn();
    renderGraph({ onSelectNode });
    const node = personNode("p061");
    expect(node.getAttribute("tabindex")).toBe("0");
    expect(node.getAttribute("role")).toBe("button");
    node.focus();
    fireEvent.keyDown(node, { key: "Enter" });
    expect(onSelectNode).toHaveBeenCalledWith({ id: "p061", kind: "human", label: "Ana Flores" });
    fireEvent.keyDown(node, { key: "Escape" });
    const finance = screen.getAllByTestId("graph-dept").find((d) => d.getAttribute("data-dept") === "Finance")!;
    expect(document.activeElement).toBe(finance);
  });

  it("F6 re-pin: every rendered data-id and every rendered edge maps to the payload", () => {
    const { container } = renderGraph({ selectedId: "p060" });
    const payloadIds = new Set<string>([
      GRAPH.center.id,
      ...GRAPH.departments.map((d) => d.id),
      ...GRAPH.people.map((p) => p.id),
      ...GRAPH.tools.map((t) => t.id),
      ...GRAPH.sources.map((s) => s.id),
    ]);
    for (const el of Array.from(container.querySelectorAll("[data-id]"))) {
      expect(payloadIds.has(el.getAttribute("data-id")!)).toBe(true);
    }
    const payloadEdges = new Set(GRAPH.edges.map((e) => `${e.from}|${e.kind}|${e.to}`));
    for (const e of screen.getAllByTestId("graph-edge")) {
      const key = `${e.getAttribute("data-from")}|${e.getAttribute("data-kind")}|${e.getAttribute("data-to")}`;
      expect(payloadEdges.has(key)).toBe(true);
    }
  });

  it("the sr mirror regroups by department and names every member (heads flagged)", () => {
    renderGraph();
    const deptLists = screen.getAllByTestId("graph-sr-dept");
    expect(deptLists.length).toBe(GRAPH.departments.length);
    const finance = deptLists.find((ul) => ul.getAttribute("aria-label") === "Finance department")!;
    expect(finance.textContent).toContain("Felix Osei");
    expect(finance.textContent).toContain("head");
    expect(finance.textContent).toContain("Ana Flores");
  });
});

describe("REF-7: search + reduced motion", () => {
  it("a query lights the match and dims the rest", () => {
    renderGraph({ query: "Flores" });
    expect(personNode("p061").getAttribute("opacity")).toBe("1");
    expect(personNode("p075").getAttribute("opacity")).toBe(String(GEOMETRY.graphDimOpacity));
  });

  it("reduced motion strips every transition", () => {
    renderGraph({ reducedMotion: true });
    expect(personNode("p061").getAttribute("style") ?? "").not.toContain("transition");
    for (const e of screen.getAllByTestId("graph-edge")) {
      expect(e.getAttribute("style") ?? "").not.toContain("transition");
    }
  });
});

// ===========================================================================
// REF-8 — THE LIVE INTERACTION LAWS still wired into OrgGraph: type filters,
// pan/zoom + fit-reset, within-tier arrow traversal (hubs and heads), focus-
// mode ghosting, and click-away release. These are exercised through the same
// props/controls GraphRoom drives at runtime, so a regression in any of them
// fails here (the reference-map rewrite kept the source; this keeps the test).
// ===========================================================================

function hubNode(deptId: string): HTMLElement {
  return screen.getAllByTestId("graph-dept").find((el) => el.getAttribute("data-dept") === deptId)!;
}
function headNode(id: string): HTMLElement {
  return screen.getAllByTestId("graph-head").find((el) => el.getAttribute("data-id") === id)!;
}

describe("REF-8: filters, zoom/reset, arrows, focus-mode, click-away", () => {
  it("a type filter hides those nodes without disturbing any surviving position", () => {
    const { rerender } = renderGraph({ hiddenKinds: [] });
    const before = personNode("p061").getAttribute("transform");
    rerender(
      <OrgGraph graph={GRAPH} onSelectNode={() => {}} onFocusDept={() => {}} hiddenKinds={["agents"]} />,
    );
    expect(screen.queryAllByTestId("graph-tool").length).toBe(0);
    expect(personNode("p061").getAttribute("transform")).toBe(before);
    // The people rim is untouched (members = the non-anchor payload).
    expect(screen.getAllByTestId("graph-person").length).toBe(
      GRAPH.people.filter((p) => p.ring !== "anchor").length,
    );
  });

  it("hiding people clears the dots, the promoted heads, and the rim spokes", () => {
    renderGraph({ hiddenKinds: ["people"] });
    expect(screen.queryAllByTestId("graph-person").length).toBe(0);
    expect(screen.queryAllByTestId("graph-head").length).toBe(0);
    expect(screen.queryAllByTestId("graph-rim-spoke").length).toBe(0);
  });

  it("a wheel zoom transforms the scene; Fit/reset restores identity and releases selection + focus", () => {
    const onFocusDept = vi.fn();
    const onSelectNode = vi.fn();
    renderGraph({ onFocusDept, onSelectNode });
    const svg = screen.getByLabelText("Organization graph");
    expect(screen.getByTestId("graph-scene").getAttribute("transform")).toBe("translate(0,0) scale(1)");
    fireEvent.wheel(svg, { deltaY: -400, clientX: 400, clientY: 400 });
    expect(screen.getByTestId("graph-scene").getAttribute("transform")).not.toBe("translate(0,0) scale(1)");
    fireEvent.click(screen.getByTestId("graph-reset"));
    expect(screen.getByTestId("graph-scene").getAttribute("transform")).toBe("translate(0,0) scale(1)");
    expect(onFocusDept).toHaveBeenCalledWith(null);
    expect(onSelectNode).toHaveBeenCalledWith(null);
  });

  it("arrow keys traverse WITHIN a tier — hubs among hubs, heads among heads (a head never ejects the arc)", () => {
    renderGraph();
    // Hub tier: Finance → IT.
    const finance = hubNode("Finance");
    finance.focus();
    fireEvent.keyDown(finance, { key: "ArrowRight" });
    expect(document.activeElement).toBe(hubNode("IT"));
    // Head tier: the two promoted heads walk among themselves (finding: a head
    // used to sit in BOTH its member arc and the head tier, trapping traversal).
    const financeHead = headNode("p060");
    financeHead.focus();
    fireEvent.keyDown(financeHead, { key: "ArrowRight" });
    expect(document.activeElement).toBe(headNode("p074"));
    fireEvent.keyDown(headNode("p074"), { key: "ArrowRight" });
    expect(document.activeElement).toBe(headNode("p060"));
  });

  it("focus mode ghosts every out-of-department node (0.1), not the 0.15 rest-dim", () => {
    renderGraph({ focusDept: "Finance" });
    expect(personNode("p061").getAttribute("opacity")).toBe("1");
    expect(personNode("p075").getAttribute("opacity")).toBe(String(GEOMETRY.graphGhostOpacity));
    // The ghost branch is distinct from the ego rest-dim.
    expect(GEOMETRY.graphGhostOpacity).not.toBe(GEOMETRY.graphDimOpacity);
  });

  it("click-away on the stage background releases a persisted selection", () => {
    const onSelectNode = vi.fn();
    renderGraph({ selectedId: "p060", onSelectNode });
    fireEvent.click(screen.getByLabelText("Organization graph"));
    expect(onSelectNode).toHaveBeenCalledWith(null);
  });
});


// ---------------------------------------------------------------------------
// U-46 SIDEBAR — real counts, filters, department focus
// ---------------------------------------------------------------------------

describe("U-46: the sidebar shows real counts and drives filters + focus", () => {
  it("renders the org cardinalities and fires the controls", () => {
    const onToggleKind = vi.fn();
    const onFocusDept = vi.fn();
    render(
      <GraphSidebar
        orgName="Bryremead Distribution Ltd"
        actor="p060"
        stats={STATS}
        graph={GRAPH}
        hiddenKinds={[]}
        onToggleKind={onToggleKind}
        focusDept={null}
        onFocusDept={onFocusDept}
      />,
    );
    const stat = (label: string) =>
      screen.getAllByTestId("sidebar-stat").find((s) => s.getAttribute("data-key") === label)!;
    expect(within(stat("People")).getByTestId("sidebar-stat-value").textContent).toBe("120");
    expect(within(stat("Documents")).getByTestId("sidebar-stat-value").textContent).toBe("600");
    expect(within(stat("Permission edges")).getByTestId("sidebar-stat-value").textContent).toBe("16,881");
    expect(within(stat("Agents")).getByTestId("sidebar-stat-value").textContent).toBe("4");
    expect(within(stat("Graph projects")).getByTestId("sidebar-stat-value").textContent).toBe("1");

    fireEvent.click(screen.getAllByTestId("filter-toggle").find((b) => b.getAttribute("data-kind") === "agents")!);
    expect(onToggleKind).toHaveBeenCalledWith("agents");
    fireEvent.click(screen.getAllByTestId("sidebar-dept").find((b) => b.getAttribute("data-dept") === "IT")!);
    expect(onFocusDept).toHaveBeenCalledWith("IT");
  });
});

// ---------------------------------------------------------------------------
// U-47 INSPECTOR — real governance per node kind
// ---------------------------------------------------------------------------

describe("U-47: the inspector shows the compiled governance", () => {
  const humanSummary: NodeSummary = {
    demo_identity_mode: true,
    id: "p060",
    kind: "human",
    name: "Felix Osei",
    title: "Head of Finance",
    department: "Finance",
    band: 5,
    groups: ["grp_finance"],
    sites: ["site_keldonbury"],
    reports_to: "Ingrid Cohen",
    manages: 13,
    corpus_documents: 600,
    visible_documents: 193,
    access_by_reason: [
      { reason: "REBAC:grp_finance", sentence: "You see this because you are in grp_finance.", granted: 120 },
      { reason: "PUBLIC:all", sentence: "You see this because it is public to every principal.", granted: 73 },
    ],
    agents_owned: [],
  };

  it("renders a person's reach, reasons, and an audited lens entry", () => {
    const onEnterLens = vi.fn();
    render(
      <GraphInspector
        node={{ id: "p060", kind: "human", label: "Felix Osei" }}
        summary={humanSummary}
        loading={false}
        graph={GRAPH}
        onEnterLens={onEnterLens}
        onClose={() => {}}
      />,
    );
    expect(screen.getByTestId("inspector-name").textContent).toBe("Felix Osei");
    expect(screen.getByTestId("inspector-relationship-trace").textContent).toContain("Relationship trace");
    expect(screen.getByTestId("inspector-relationship-trace").textContent).toContain("member of Finance");
    expect(screen.getByTestId("inspector-reach")).toBeTruthy();
    expect(screen.getAllByTestId("inspector-reason").length).toBe(2);
    fireEvent.click(screen.getByTestId("inspector-enter-lens"));
    expect(onEnterLens).toHaveBeenCalledWith("p060");
  });

  it("renders an agent's permitted and blocked actions", () => {
    const agentSummary: NodeSummary = {
      demo_identity_mode: true,
      id: "agent_finance_analyst",
      kind: "agent",
      name: "Finance analysis assistant",
      owner_user_id: "p061",
      grant_groups: ["grp_finance"],
      corpus_documents: 600,
      visible_documents: 168,
      permitted_actions: ["retrieve_within_allowlist", "propose_draft"],
      blocked_actions: ["approve_or_reject_proposals", "mutate_records"],
    };
    render(
      <GraphInspector
        node={{ id: "agent_finance_analyst", kind: "agent", label: "Finance analysis assistant" }}
        summary={agentSummary}
        loading={false}
        graph={GRAPH}
        onEnterLens={() => {}}
        onClose={() => {}}
      />,
    );
    expect(screen.getAllByTestId("inspector-permitted").length).toBe(2);
    expect(screen.getAllByTestId("inspector-blocked").length).toBe(2);
  });

  it("composes a department panel from the graph (no endpoint)", () => {
    render(
      <GraphInspector
        node={{ id: "Finance", kind: "department", label: "Finance" }}
        summary={null}
        loading={false}
        graph={GRAPH}
        onEnterLens={() => {}}
        onClose={() => {}}
      />,
    );
    expect(screen.getByTestId("inspector-department")).toBeTruthy();
    expect(screen.getByText(/2 people/)).toBeTruthy();
  });

  it("composes a project panel from graph capability data (no endpoint)", () => {
    render(
      <GraphInspector
        node={{ id: "cap31", kind: "project", label: "Capability: Access Review 31" }}
        summary={null}
        loading={false}
        graph={GRAPH}
        onEnterLens={() => {}}
        onClose={() => {}}
      />,
    );
    expect(screen.getByTestId("inspector-project")).toBeTruthy();
    expect(screen.getByText("cap31")).toBeTruthy();
    expect(screen.getByText("2 people")).toBeTruthy();
    expect(screen.getByText("Finance")).toBeTruthy();
    expect(screen.getByText("Project trace")).toBeTruthy();
    expect(screen.getByTestId("inspector-relationship-trace").textContent).toContain("works on Access Review 31");
    expect(screen.getByText("Active: 1")).toBeTruthy();
  });

  it("offers a project-only access request form without approver or document fields", async () => {
    const onRequestAccess = vi.fn().mockResolvedValue(undefined);
    render(
      <GraphInspector
        actor="p060"
        node={{ id: "cap31", kind: "project", label: "Capability: Access Review 31" }}
        summary={null}
        loading={false}
        graph={GRAPH}
        accessRequests={[]}
        onRequestAccess={onRequestAccess}
        onEnterLens={() => {}}
        onClose={() => {}}
      />,
    );
    fireEvent.click(screen.getByTestId("access-request-submit"));
    expect(screen.getByTestId("access-request-feedback").textContent).toMatch(/short reason/i);

    fireEvent.change(screen.getByTestId("access-request-justification"), {
      target: { value: "Need this capability context for assigned project work." },
    });
    fireEvent.click(screen.getByTestId("access-request-submit"));
    await waitFor(() =>
      expect(onRequestAccess).toHaveBeenCalledWith(
        { kind: "project", capability_id: "cap31" },
        "Need this capability context for assigned project work.",
      ),
    );
  });
});

// ---------------------------------------------------------------------------
// U-48 THE ROOM — counts wire up, select opens the inspector, theme toggles
// ---------------------------------------------------------------------------

function stubGraphFetch() {
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.endsWith("/graph")) return new Response(JSON.stringify(GRAPH), { status: 200 });
    if (url.endsWith("/node/org/summary"))
      return new Response(JSON.stringify({ demo_identity_mode: true, id: "org", kind: "org", name: "Bryremead Distribution Ltd", stats: STATS }), { status: 200 });
    if (url.includes("/node/")) {
      return new Response(
        JSON.stringify({
          demo_identity_mode: true,
          id: "p074",
          kind: "human",
          name: "Yuki Moreau",
          department: "IT",
          band: 6,
          groups: ["grp_it"],
          sites: ["site_keldonbury"],
          corpus_documents: 600,
          visible_documents: 105,
          access_by_reason: [{ reason: "REBAC:grp_it", sentence: "You see this because you are in grp_it.", granted: 90 }],
        }),
        { status: 200 },
      );
    }
    return new Response("{\"demo_identity_mode\":true,\"error\":\"not found\"}", { status: 404 });
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

function stubGraphFetchWithAccess() {
  let approved = false;
  const request = () => ({
    approver_id: "p060",
    created_ordinal: 0,
    decision: approved ? { actor_principal: "p060", decided_ordinal: 1, outcome: "approved" } : undefined,
    justification: "Need this capability context for assigned project work.",
    request_id: "ar_test",
    request_key: "rk",
    requester_id: "p061",
    snapshot_version: "snap",
    status: approved ? "approved" : "pending",
    target: { kind: "project", capability_id: "cap31" },
  });
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (url.endsWith("/graph")) return new Response(JSON.stringify(GRAPH), { status: 200 });
    if (url.endsWith("/node/org/summary"))
      return new Response(JSON.stringify({ demo_identity_mode: true, id: "org", kind: "org", name: "Bryremead Distribution Ltd", stats: STATS }), { status: 200 });
    if (url.endsWith("/access-requests/inbox")) {
      return new Response(
        JSON.stringify({ actor_id: "p060", demo_identity_mode: true, requests: approved ? [] : [request()], snapshot_version: "snap" }),
        { status: 200 },
      );
    }
    if (url.endsWith("/access-requests") && init?.method !== "POST") {
      return new Response(
        JSON.stringify({ actor_id: "p060", demo_identity_mode: true, requests: [request()], snapshot_version: "snap" }),
        { status: 200 },
      );
    }
    if (url.endsWith("/access-requests/ar_test/approve")) {
      approved = true;
      return new Response(JSON.stringify({ demo_identity_mode: true, request: request(), snapshot_version: "snap" }), { status: 200 });
    }
    if (url.includes("/node/")) {
      return new Response(
        JSON.stringify({ demo_identity_mode: true, id: "p074", kind: "human", name: "Yuki Moreau" }),
        { status: 200 },
      );
    }
    return new Response("{\"demo_identity_mode\":true,\"error\":\"not found\"}", { status: 404 });
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

describe("U-48: the Org Brain room wires counts, selection, and theme", () => {
  it("shows real counts, opens the inspector on select, and toggles the theme", async () => {
    document.documentElement.setAttribute("data-theme", "dark");
    stubGraphFetch();
    render(<GraphRoom actor="p060" />);

    await waitFor(() => expect(screen.getByTestId("graph-sidebar")).toBeTruthy());
    await waitFor(() => expect(screen.getByTestId("graph-audit-panel")).toBeTruthy());
    expect(screen.getByTestId("graph-audit-panel").textContent).toContain("Operating Map");
    expect(screen.getByTestId("graph-acting-context").textContent).toContain("Acting as p060");
    expect(screen.getByTestId("graph-audited-line").textContent).toContain("This view is audited");
    expect(screen.getByTestId("graph-audited-line").textContent).toContain("hidden or restricted");
    expect(screen.getByTestId("graph-relationship-summary").textContent).toContain("Keyboard-readable rows");
    expect(within(screen.getByTestId("graph-relationship-summary")).getAllByTestId("graph-relationship-row").length).toBe(5);
    expect(screen.getByTestId("graph-relationship-summary").textContent).toContain("Felix Osei");
    // "signals unavailable" is now the map's honest legend row; only the
    // lying "permissions unavailable" row stays banned.
    expect(screen.getByTestId("graph-room").textContent ?? "").not.toMatch(/permissions unavailable/i);
    expect(screen.queryByTestId("graph-signal-ring")).toBeNull();
    expect(screen.queryByTestId("graph-permission-ring")).toBeNull();
    const stat = (label: string) =>
      screen.getAllByTestId("sidebar-stat").find((s) => s.getAttribute("data-key") === label)!;
    await waitFor(() =>
      expect(within(stat("People")).getByTestId("sidebar-stat-value").textContent).toBe("120"),
    );

    // Select a person -> the inspector opens with its governance. Yuki Moreau
    // (p074) is a department head, so it renders as a promoted head node.
    fireEvent.click(
      document.querySelector('[data-testid="graph-head"][data-id="p074"], [data-testid="graph-person"][data-id="p074"]')!,
    );
    await waitFor(() => expect(screen.getByTestId("inspector-card")).toBeTruthy());
    expect(screen.getByTestId("inspector-name").textContent).toBe("Yuki Moreau");
    expect(screen.getByTestId("inspector-relationship-trace").textContent).toContain("Yuki Moreau");
    expect(screen.getByTestId("inspector-enter-lens").textContent).toContain("access view");
    expect(document.querySelector("a[href*='/person']")).toBeNull();
    await waitFor(() =>
      expect(screen.getByTestId("graph-relationship-summary").textContent).toContain("Relationships connected to Yuki Moreau"),
    );

    // Theme toggle flips the document attribute (dark default -> light).
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    fireEvent.click(screen.getByTestId("theme-toggle"));
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
  });

  it("loads access request status and records approver decisions", async () => {
    const fetchMock = stubGraphFetchWithAccess();
    render(<GraphRoom actor="p060" />);

    await waitFor(() => expect(screen.getByTestId("access-request-rail")).toBeTruthy());
    expect(screen.getByTestId("access-inbox-row").textContent).toContain("Need this capability context");

    fireEvent.click(screen.getByTestId("access-approve"));
    await waitFor(() => expect(screen.getByTestId("access-rail-feedback").textContent).toMatch(/not expanded/i));
    expect(
      fetchMock.mock.calls.some((call) => String(call[0]).endsWith("/access-requests/ar_test/approve")),
    ).toBe(true);
  });
});

