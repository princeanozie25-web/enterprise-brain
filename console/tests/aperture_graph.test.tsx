/**
 * AR-2 Org Graph tests U-33..U-37. Fully offline: small typed /graph fixtures,
 * OrgGraph rendered directly (d3-force computes the layout, React owns the
 * DOM). No fetch, no sockets.
 */
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";

import type { GraphResponse } from "@/lib/api";
import { OrgGraph } from "@/components/OrgGraph";
import { lensHref } from "@/components/GraphRoom";

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
  tools: [
    { id: "agent_finance_analyst", label: "Finance analysis assistant", kind: "agent", department_id: "Finance" },
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
  edges: [{ from: "p046", kind: "member_of", to: "Pharmacy Services" }],
  snapshot_version: "snap",
};

function personNode(id: string): HTMLElement {
  return screen
    .getAllByTestId("graph-person")
    .find((el) => el.getAttribute("data-id") === id)!;
}

// ---------------------------------------------------------------------------
// U-33 STRUCTURE
// ---------------------------------------------------------------------------

describe("U-33: the graph renders hubs, anchors, members, and edges", () => {
  it("renders department hubs, anchor + member people, and the exact edge count", () => {
    render(<OrgGraph graph={GRAPH} onSelectPerson={() => {}} />);
    expect(screen.getByTestId("org-graph")).toBeTruthy();
    expect(screen.getAllByTestId("graph-dept").length).toBe(GRAPH.departments.length);

    const people = screen.getAllByTestId("graph-person");
    expect(people.length).toBe(GRAPH.people.length);
    const anchors = people.filter((p) => p.getAttribute("data-ring") === "anchor");
    const members = people.filter((p) => p.getAttribute("data-ring") === "member");
    expect(anchors.length).toBe(2);
    expect(members.length).toBe(2);

    expect(screen.getAllByTestId("graph-edge").length).toBe(GRAPH.edges.length);
    // Every person carries an avatar (the AR-1 PersonAvatar).
    expect(screen.getAllByTestId("person-avatar-img").length).toBe(GRAPH.people.length);
  });
});

// ---------------------------------------------------------------------------
// U-34 ANCHOR LABELS vs MEMBER HOVER-REVEAL
// ---------------------------------------------------------------------------

describe("U-34: anchors are labelled; members reveal their name on hover", () => {
  it("labels anchors always and members only on hover", () => {
    render(<OrgGraph graph={GRAPH} onSelectPerson={() => {}} />);

    // Anchor: name shown immediately.
    expect(within(personNode("p060")).getByTestId("graph-person-name").textContent).toBe(
      "Felix Osei",
    );
    // Member: avatar only — no name until hover.
    const member = personNode("p061");
    expect(within(member).queryByTestId("graph-person-name")).toBeNull();
    expect(within(member).getByTestId("person-avatar-img")).toBeTruthy();
    fireEvent.mouseEnter(member);
    expect(within(member).getByTestId("graph-person-name").textContent).toBe("Ana Flores");
    fireEvent.mouseLeave(member);
    expect(within(personNode("p061")).queryByTestId("graph-person-name")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// U-35 CLICK-TO-LENS (cross-lens, audited route)
// ---------------------------------------------------------------------------

describe("U-35: clicking a person flies into their lens", () => {
  it("invokes the select handler and builds the audited cross-lens route", () => {
    const onSelect = vi.fn();
    render(<OrgGraph graph={GRAPH} onSelectPerson={onSelect} />);
    fireEvent.click(personNode("p074"));
    expect(onSelect).toHaveBeenCalledWith("p074");
    // The route the room navigates to: actor stays, subject becomes the click
    // (cross-lens => audited server-side, the audited-view line shows).
    expect(lensHref("p060", "p074")).toBe("/lens?as=p060&subject=p074");
  });
});

// ---------------------------------------------------------------------------
// U-36 HONEST DARK
// ---------------------------------------------------------------------------

describe("U-36: a minimal world is a small graph, never padded", () => {
  it("renders the small graph with no ghost nodes and no +N hidden", () => {
    const { container } = render(<OrgGraph graph={MINIMAL} onSelectPerson={() => {}} />);
    expect(screen.getByTestId("org-graph")).toBeTruthy();
    expect(screen.getAllByTestId("graph-person").length).toBe(1);
    expect(screen.getAllByTestId("graph-dept").length).toBe(1);
    // Honest dark: nothing teased.
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
    render(<OrgGraph graph={GRAPH} onSelectPerson={() => {}} />);
    const self = personNode("p060");
    expect(self.getAttribute("data-self")).toBe("true");
    expect(within(self).getByTestId("graph-self-marker")).toBeTruthy();
    // No one else is "here".
    for (const id of ["p061", "p074", "p075"]) {
      const node = personNode(id);
      expect(node.getAttribute("data-self")).toBe("false");
      expect(within(node).queryByTestId("graph-self-marker")).toBeNull();
    }
  });
});
