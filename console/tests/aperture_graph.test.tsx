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
import { OrgGraph, PEOPLE_CLUSTER_THRESHOLD } from "@/components/OrgGraph";
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
function defaultRenderedEdges(graph: GraphResponse): number {
  return graph.edges.filter((edge) => edge.kind !== "works_on" && edge.kind !== "involves_department").length;
}

// ---------------------------------------------------------------------------
// U-33 STRUCTURE — the concentric rings, all real entities
// ---------------------------------------------------------------------------

describe("U-33: the graph renders core, hubs, agents, sources, and all people", () => {
  it("renders every baseline ring and keeps project nodes hidden by default", () => {
    const { container } = renderGraph();
    expect(screen.getByTestId("org-graph")).toBeTruthy();
    expect(screen.getByTestId("graph-center")).toBeTruthy();
    expect(screen.getAllByTestId("graph-dept").length).toBe(GRAPH.departments.length);
    expect(screen.getAllByTestId("graph-tool").length).toBe(GRAPH.tools.length);
    expect(screen.getAllByTestId("graph-source").length).toBe(GRAPH.sources.length);
    expect(screen.queryAllByTestId("graph-project").length).toBe(0);
    const people = screen.getAllByTestId("graph-person");
    expect(people.length).toBe(GRAPH.people.length);
    expect(people.filter((p) => p.getAttribute("data-ring") === "anchor").length).toBe(2);
    expect(screen.getAllByTestId("graph-edge").length).toBe(defaultRenderedEdges(GRAPH));
    expect(screen.getAllByTestId("person-avatar-img").length).toBe(GRAPH.people.length);
    expect(screen.queryByTestId("graph-signal-ring")).toBeNull();
    expect(screen.queryByTestId("graph-permission-ring")).toBeNull();
    expect(container.textContent ?? "").not.toMatch(/signals unavailable|permissions unavailable/i);
  });
});

// ---------------------------------------------------------------------------
// U-34 ANCHOR LABELS vs MEMBER HOVER-REVEAL
// ---------------------------------------------------------------------------

describe("U-34 (ring law): every rendered person carries their full name, always", () => {
  it("names anchors AND members without hover or zoom — no monogram caterpillar", () => {
    renderGraph();
    for (const [id, name] of [
      ["p060", "Felix Osei"],
      ["p061", "Ana Flores"],
      ["p074", "Yuki Moreau"],
      ["p075", "Mei Kim"],
    ] as const) {
      expect(within(personNode(id)).getByTestId("graph-person-name").textContent).toBe(name);
    }
  });
});

// ---------------------------------------------------------------------------
// U-35 SELECTION + FOCUS + CROSS-LENS ROUTE
// ---------------------------------------------------------------------------

describe("U-35: clicking selects; a hub enters focus; the lens route is audited", () => {
  it("emits the selected node for a person and focus+select for a hub", () => {
    const onSelectNode = vi.fn();
    const onFocusDept = vi.fn();
    renderGraph({ onSelectNode, onFocusDept });

    fireEvent.click(personNode("p074"));
    expect(onSelectNode).toHaveBeenCalledWith({ id: "p074", kind: "human", label: "Yuki Moreau" });

    const hub = screen.getAllByTestId("graph-dept").find((d) => d.getAttribute("data-dept") === "Finance")!;
    fireEvent.click(hub);
    expect(onFocusDept).toHaveBeenCalledWith("Finance");
    expect(onSelectNode).toHaveBeenCalledWith({ id: "Finance", kind: "department", label: "Finance" });

    // The cross-lens route the inspector navigates to (actor stays, subject set).
    expect(lensHref("p060", "p074")).toBe("/lens?as=p060&subject=p074");
  });
});

// ---------------------------------------------------------------------------
// U-36 HONEST DARK
// ---------------------------------------------------------------------------

describe("U-36: a minimal world is a small graph, never padded", () => {
  it("renders the small graph with no ghost nodes and no +N hidden", () => {
    const { container } = render(<OrgGraph graph={MINIMAL} onSelectNode={() => {}} onFocusDept={() => {}} />);
    expect(screen.getAllByTestId("graph-person").length).toBe(1);
    expect(screen.getAllByTestId("graph-dept").length).toBe(1);
    expect(screen.queryAllByTestId("graph-source").length).toBe(0);
    expect(screen.queryAllByTestId("graph-tool").length).toBe(0);
    expect(screen.queryByTestId("graph-ghost")).toBeNull();
    expect(container.textContent ?? "").not.toMatch(/\+\d+\b/);
    expect(container.textContent ?? "").not.toMatch(/hidden|more/i);
  });
});

// ---------------------------------------------------------------------------
// U-37 YOU ARE HERE
// ---------------------------------------------------------------------------

describe("U-37: the actor's own node is marked 'you are here'", () => {
  it("carries the self marker on exactly the actor", () => {
    renderGraph();
    expect(within(personNode("p060")).getByTestId("graph-self-marker")).toBeTruthy();
    for (const id of ["p061", "p074", "p075"]) {
      expect(within(personNode(id)).queryByTestId("graph-self-marker")).toBeNull();
    }
  });
});

// ---------------------------------------------------------------------------
// U-38 PAN/ZOOM + FIT/RESET
// ---------------------------------------------------------------------------

describe("U-38: pan/zoom transforms the scene and Fit/reset restores it", () => {
  it("a wheel zoom changes the scene transform; reset returns to identity", () => {
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
});

// ---------------------------------------------------------------------------
// U-39 THE RING LAW (B1) — real nodes, keyboard operability, honest clusters
// ---------------------------------------------------------------------------

/** A synthetic >threshold payload for the cluster rule (a test fixture IS a
 * payload — every rendered node still maps to one of its entries). */
function bigGraph(peopleCount: number): GraphResponse {
  const people = Array.from({ length: peopleCount }, (_, i) => ({
    id: `p${String(i + 1).padStart(3, "0")}`,
    display_name: `Person ${i + 1}`,
    title: "Analyst",
    department_id: i % 2 === 0 ? "Finance" : "IT",
    avatar_ref: "",
    is_self: i === 0,
    ring: "member" as const,
  }));
  return { ...GRAPH, people, edges: [], projects: [], tools: [], sources: [] };
}

describe("U-39 (T-B8): every rendered graph node maps to a real payload entity", () => {
  it("renders zero decorative/synthetic nodes — all data-ids come from the payload", () => {
    const { container } = renderGraph({ query: "Access Review" }); // projects revealed too
    const payloadIds = new Set<string>([
      GRAPH.center.id,
      ...GRAPH.departments.map((d) => d.id),
      ...GRAPH.people.map((p) => p.id),
      ...GRAPH.tools.map((t) => t.id),
      ...GRAPH.sources.map((s) => s.id),
      ...GRAPH.projects.map((p) => p.id),
    ]);
    const rendered = Array.from(container.querySelectorAll("[data-id]"));
    expect(rendered.length).toBeGreaterThan(0);
    for (const el of rendered) {
      expect(payloadIds.has(el.getAttribute("data-id")!)).toBe(true);
    }
  });

  it("mirrors the rendered nodes for screen readers", () => {
    renderGraph();
    const mirror = screen.getByTestId("graph-sr-mirror");
    for (const person of GRAPH.people) {
      expect(mirror.textContent).toContain(person.display_name);
    }
    expect(mirror.textContent).toContain("Bryremead Distribution Ltd");
  });
});

describe("U-39b (T-B2): the graph is keyboard-operable", () => {
  it("nodes are tab stops; Enter activates; Escape returns focus to the graph root", () => {
    const onSelectNode = vi.fn();
    renderGraph({ onSelectNode });
    const node = personNode("p061");
    expect(node.getAttribute("tabindex")).toBe("0");
    expect(node.getAttribute("role")).toBe("button");
    node.focus();
    fireEvent.keyDown(node, { key: "Enter" });
    expect(onSelectNode).toHaveBeenCalledWith({ id: "p061", kind: "human", label: "Ana Flores" });
    fireEvent.keyDown(node, { key: "Escape" });
    expect(document.activeElement).toBe(screen.getByTestId("org-graph"));
  });

  it("arrow keys traverse the ring order", () => {
    renderGraph();
    const center = screen.getByTestId("graph-center");
    center.focus();
    fireEvent.keyDown(center, { key: "ArrowRight" });
    // The next stop after the center is the first department hub.
    const finance = screen.getAllByTestId("graph-dept").find((d) => d.getAttribute("data-dept") === "Finance")!;
    expect(document.activeElement).toBe(finance);
  });
});

describe("U-40 (cluster rule): past the threshold the ring collapses to honest department clusters", () => {
  it(`over ${PEOPLE_CLUSTER_THRESHOLD} people: clusters with in-scope counts, no person nodes`, () => {
    render(
      <OrgGraph graph={bigGraph(PEOPLE_CLUSTER_THRESHOLD + 2)} onSelectNode={() => {}} onFocusDept={() => {}} />,
    );
    expect(screen.queryAllByTestId("graph-person").length).toBe(0);
    const clusters = screen.getAllByTestId("graph-people-cluster");
    expect(clusters.length).toBe(2);
    const counts = clusters.map((c) => Number(c.getAttribute("data-count"))).sort((a, b) => a - b);
    expect(counts[0] + counts[1]).toBe(PEOPLE_CLUSTER_THRESHOLD + 2);
    expect(clusters[0].textContent).toContain("people in scope");
  });

  it(`at or under ${PEOPLE_CLUSTER_THRESHOLD} people: every person renders individually`, () => {
    render(
      <OrgGraph graph={bigGraph(PEOPLE_CLUSTER_THRESHOLD)} onSelectNode={() => {}} onFocusDept={() => {}} />,
    );
    expect(screen.getAllByTestId("graph-person").length).toBe(PEOPLE_CLUSTER_THRESHOLD);
    expect(screen.queryAllByTestId("graph-people-cluster").length).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// U-41 EDGES + U-42 SOURCES
// ---------------------------------------------------------------------------

describe("U-41: edges are curved paths ranked by kind", () => {
  it("renders every default edge as a quadratic path carrying its kind, incl. system_of", () => {
    renderGraph();
    const edges = screen.getAllByTestId("graph-edge");
    expect(edges.length).toBe(defaultRenderedEdges(GRAPH));
    for (const e of edges) {
      expect(e.tagName.toLowerCase()).toBe("path");
      expect(e.getAttribute("d") ?? "").toMatch(/^M.*Q/);
    }
    const kinds = new Set(edges.map((e) => e.getAttribute("data-kind")));
    for (const k of ["reports_to", "member_of", "owns_agent", "system_of"]) {
      expect(kinds.has(k)).toBe(true);
    }
  });
});

describe("U-42: the real systems of record ride the graph", () => {
  it("renders one node per source with its label", () => {
    renderGraph();
    const sources = screen.getAllByTestId("graph-source");
    expect(sources.map((s) => s.getAttribute("data-id")).sort()).toEqual(["docstore", "wiki"]);
    expect(within(sources.find((s) => s.getAttribute("data-id") === "docstore")!).getByText("Document store")).toBeTruthy();
  });
});

// ---------------------------------------------------------------------------
// U-43 FILTERS PRESERVE LAYOUT
// ---------------------------------------------------------------------------

describe("U-43: a type filter hides nodes without disturbing the layout", () => {
  it("hiding agents removes them but keeps every person's position", () => {
    const { rerender } = renderGraph({ hiddenKinds: [] });
    const before = personNode("p061").getAttribute("transform");
    rerender(<OrgGraph graph={GRAPH} onSelectNode={() => {}} onFocusDept={() => {}} hiddenKinds={["agents"]} />);
    expect(screen.queryAllByTestId("graph-tool").length).toBe(0);
    expect(personNode("p061").getAttribute("transform")).toBe(before);
    expect(screen.getAllByTestId("graph-person").length).toBe(GRAPH.people.length);
  });
});

// ---------------------------------------------------------------------------
// U-44 SEARCH + U-45 FOCUS MODE (deterministic emphasis)
// ---------------------------------------------------------------------------

describe("U-44: search lights matches and dims the rest", () => {
  it("a query emphasizes the match and reveals its name; others dim", () => {
    renderGraph({ query: "Flores" }); // unique to Ana Flores (p061)
    expect(personNode("p061").getAttribute("opacity")).toBe("1");
    expect(within(personNode("p061")).getByTestId("graph-person-name").textContent).toBe("Ana Flores");
    expect(personNode("p075").getAttribute("opacity")).toBe(String(GEOMETRY.graphDimOpacity));
  });
});

describe("U-44b: project/capability traces reveal only when relevant", () => {
  it("reveals a real capability through search and emits a project selection", () => {
    const onSelectNode = vi.fn();
    renderGraph({ query: "Access Review", onSelectNode });
    const project = screen.getByTestId("graph-project");
    expect(project.getAttribute("data-id")).toBe("cap31");
    expect(within(project).getByTestId("graph-project-label").textContent).toBe("Access Review 31");
    const kinds = new Set(screen.getAllByTestId("graph-edge").map((edge) => edge.getAttribute("data-kind")));
    expect(kinds.has("works_on")).toBe(true);
    expect(kinds.has("involves_department")).toBe(true);
    fireEvent.click(project);
    expect(onSelectNode).toHaveBeenCalledWith({ id: "cap31", kind: "project", label: "Capability: Access Review 31" });
  });

  it("selecting a capability traces assigned people and involved departments", () => {
    renderGraph({ selectedId: "cap31" });
    expect(screen.getByTestId("graph-project")).toBeTruthy();
    expect(personNode("p060").getAttribute("opacity")).toBe("1");
    expect(personNode("p075").getAttribute("opacity")).toBe("1");
    expect(personNode("p061").getAttribute("opacity")).toBe(String(GEOMETRY.graphDimOpacity));
  });
});

describe("U-45: focus mode ghosts the rest and names the department", () => {
  it("a focused department is full + named; other nodes are ghosted", () => {
    renderGraph({ focusDept: "Finance" });
    expect(personNode("p061").getAttribute("opacity")).toBe("1");
    // Finance member name revealed by focus (deterministic, not zoom-dependent).
    expect(within(personNode("p061")).getByTestId("graph-person-name").textContent).toBe("Ana Flores");
    // IT member ghosted.
    expect(personNode("p075").getAttribute("opacity")).toBe(String(GEOMETRY.graphGhostOpacity));
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
    expect(screen.getByTestId("graph-room").textContent ?? "").not.toMatch(/signals unavailable|permissions unavailable/i);
    expect(screen.queryByTestId("graph-signal-ring")).toBeNull();
    expect(screen.queryByTestId("graph-permission-ring")).toBeNull();
    const stat = (label: string) =>
      screen.getAllByTestId("sidebar-stat").find((s) => s.getAttribute("data-key") === label)!;
    await waitFor(() =>
      expect(within(stat("People")).getByTestId("sidebar-stat-value").textContent).toBe("120"),
    );

    // Select a person -> the inspector opens with its governance.
    fireEvent.click(screen.getAllByTestId("graph-person").find((p) => p.getAttribute("data-id") === "p074")!);
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
