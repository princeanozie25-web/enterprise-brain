import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import type { GraphResponse, ProjectWorkflowResponse } from "@/lib/api";
import { ProjectSurface } from "@/components/ProjectSurface";
import { WorkflowView } from "@/components/WorkflowView";

afterEach(() => {
  vi.unstubAllGlobals();
});

const WORKFLOW: ProjectWorkflowResponse = {
  actor_id: "p060",
  capability_id: "cap31",
  demo_identity_mode: true,
  items: [
    {
      capability_id: "cap31",
      dependencies: [],
      item_id: "box_active",
      kind: "lane_box",
      owner_id: "p060",
      provenance: {
        capability: { id: "cap31", name: "Access Review 31" },
        initiative: { id: "init03", name: "Strengthen Workforce Capability" },
        strategy: { id: "strat01", name: "Workforce Capability" },
        workflow: { id: "wf11", name: "Goods-In Verification 31" },
      },
      snapshot_version: "snap",
      status: "active",
      title: "Access Review 31",
    },
    {
      approver_id: "p001",
      capability_id: "cap31",
      dependencies: [],
      item_id: "ar_request",
      kind: "access_request",
      provenance: {
        capability: { id: "cap31", name: "Access Review 31" },
        initiative: { id: "init03", name: "Strengthen Workforce Capability" },
        strategy: { id: "strat01", name: "Workforce Capability" },
        workflow: { id: "wf11", name: "Goods-In Verification 31" },
      },
      requester_id: "p060",
      snapshot_version: "snap",
      status: "pending",
      title: "Access request for Access Review 31",
    },
    {
      agent_id: "agent_finance_analyst",
      capability_id: "cap31",
      dependencies: ["box_active"],
      item_id: "accepted_agent_projection",
      kind: "accepted_agent_box",
      owner_id: "p060",
      provenance: {
        capability: { id: "cap31", name: "Access Review 31" },
        initiative: { id: "init03", name: "Strengthen Workforce Capability" },
        strategy: { id: "strat01", name: "Workforce Capability" },
        workflow: { id: "wf11", name: "Goods-In Verification 31" },
      },
      snapshot_version: "snap",
      status: "done",
      title: "Review accepted agent proposal",
    },
  ],
  provenance: {
    capability: { id: "cap31", name: "Access Review 31" },
    initiative: { id: "init03", name: "Strengthen Workforce Capability" },
    strategy: { id: "strat01", name: "Workforce Capability" },
    workflow: { id: "wf11", name: "Goods-In Verification 31" },
  },
  snapshot_version: "snap",
};

const GRAPH: GraphResponse = {
  actor_id: "p060",
  center: { id: "org", label: "Bryremead Distribution Ltd" },
  departments: [
    { id: "Finance", label: "Finance", tint_key: "Finance" },
    { id: "IT", label: "IT", tint_key: "IT" },
  ],
  edges: [
    { from: "p060", kind: "works_on", to: "cap31" },
    { from: "cap31", kind: "involves_department", to: "Finance" },
    { from: "cap31", kind: "involves_department", to: "IT" },
  ],
  people: [
    {
      avatar_ref: "faces/p060.jpg",
      department_id: "Finance",
      display_name: "Felix Osei",
      id: "p060",
      is_self: true,
      ring: "anchor",
      title: "Head of Finance",
    },
  ],
  projects: [
    {
      departments: ["Finance", "IT"],
      id: "cap31",
      initiative_name: "Strengthen Workforce Capability",
      label: "Capability: Access Review 31",
      people: 2,
      primary_department_id: "Finance",
      status_counts: { Active: 1, Done: 1 },
      strategy_name: "Workforce Capability",
      workflow_name: "Goods-In Verification 31",
    },
  ],
  snapshot_version: "snap",
  sources: [],
  tools: [],
};

function stubProjectFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/workflow/project/cap31")) {
        return new Response(JSON.stringify(WORKFLOW), { status: 200 });
      }
      if (url.endsWith("/graph")) {
        return new Response(JSON.stringify(GRAPH), { status: 200 });
      }
      return new Response("{\"demo_identity_mode\":true,\"error\":\"not found\"}", {
        status: 404,
      });
    }),
  );
}

describe("workflow projection UI", () => {
  it("groups real workflow items by status without evidence rows", () => {
    const { container } = render(<WorkflowView workflow={WORKFLOW} />);
    const groups = screen.getAllByTestId("workflow-group");
    expect(groups.length).toBe(5);
    expect(within(groups[0]).getAllByTestId("workflow-item").length).toBe(1);
    expect(within(groups[2]).getAllByTestId("workflow-item").length).toBe(1);
    expect(within(groups[4]).getAllByTestId("workflow-item").length).toBe(1);
    expect(screen.getByText("Access request for Access Review 31")).toBeTruthy();
    expect(screen.getByText("agent agent_finance_analyst")).toBeTruthy();
    expect(container.textContent ?? "").not.toContain("document_id");
    expect(container.textContent ?? "").not.toContain("evidence");
  });

  it("renders the project surface with Graph View and Workflow View tabs", async () => {
    stubProjectFetch();
    render(<ProjectSurface actor="p060" capabilityId="cap31" />);
    await waitFor(() => expect(screen.getByTestId("project-title").textContent).toBe("Access Review 31"));
    expect(screen.getByTestId("workflow-view")).toBeTruthy();

    fireEvent.click(screen.getAllByTestId("project-tab")[0]);
    expect(screen.getByTestId("project-graph-view")).toBeTruthy();
    expect(screen.getByText("2 people")).toBeTruthy();
    expect(screen.getByText("3 edges")).toBeTruthy();

    fireEvent.click(screen.getAllByTestId("project-tab")[1]);
    expect(screen.getByTestId("workflow-view")).toBeTruthy();
  });
});
