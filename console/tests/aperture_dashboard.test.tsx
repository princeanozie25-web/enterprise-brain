import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";

import type {
  AccessGrantsResponse,
  AccessRequestsResponse,
  GraphResponse,
  LensResponse,
  NodeSummary,
  ProjectWorkflowResponse,
  RoleScopeSummary,
} from "@/lib/api";
import { EmployeeDashboard } from "@/components/EmployeeDashboard";

afterEach(() => {
  vi.unstubAllGlobals();
});

const LENS: LensResponse = {
  actor: {
    avatar_ref: "faces/p060.jpg",
    department_label: "Finance",
    display_name: "Felix Osei",
    id: "p060",
    title: "Head of Finance",
  },
  actor_id: "p060",
  agents: [],
  cross_lens: false,
  holdings: [
    {
      docs: [
        {
          also_via: [],
          document_id: "d0196",
          sensitivity: "confidential",
          title: "Notice: aggregate financial position",
        },
        {
          also_via: [],
          document_id: "d0197",
          sensitivity: "internal",
          title: "Finance working note",
        },
      ],
      reason: "REBAC:grp_finance",
      sentence: "You see this because you are in grp_finance.",
    },
  ],
  snapshot_version: "snap",
  subject: {
    band: 5,
    department: "Finance",
    groups: ["grp_finance"],
    id: "p060",
    kind: "human",
    name: "Felix Osei",
    sites: ["site_keldonbury"],
  },
  subject_human: {
    avatar_ref: "faces/p060.jpg",
    bio: "Head of Finance at Bryremead Distribution Ltd.",
    department_label: "Finance",
    display_name: "Felix Osei",
    id: "p060",
    location: "Keldonbury, UK",
    manages: ["p061", "p062"],
    personality_tag: "ENFJ",
    projects: [
      {
        capability_id: "cap31",
        capability_name: "Capability: Access Review 31",
        initiative_name: "Strengthen Workforce Capability",
        role: "Lead",
        status: "Active",
        strategy_name: "Workforce Capability",
        workflow_name: "Goods-In Verification 31",
      },
    ],
    reports_to: "Ingrid Cohen",
    seniority: "Leadership",
    title: "Head of Finance",
    work_style: "Hybrid",
  },
};

const GRAPH: GraphResponse = {
  actor_id: "p060",
  center: { id: "org", label: "Bryremead Distribution Ltd" },
  departments: [{ id: "Finance", label: "Finance", tint_key: "Finance" }],
  edges: [{ from: "p060", kind: "works_on", to: "cap31" }],
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
      departments: ["Finance"],
      id: "cap31",
      initiative_name: "Strengthen Workforce Capability",
      label: "Capability: Access Review 31",
      people: 2,
      primary_department_id: "Finance",
      status_counts: { Active: 1, Planned: 1 },
      strategy_name: "Workforce Capability",
      workflow_name: "Goods-In Verification 31",
    },
  ],
  snapshot_version: "snap",
  sources: [],
  tools: [],
};

const SUMMARY: NodeSummary = {
  agents_owned: [{ id: "agent_finance_analyst", name: "Finance analysis assistant" }],
  demo_identity_mode: true,
  id: "p060",
  kind: "human",
  name: "Felix Osei",
  title: "Head of Finance",
};

const ROLE_SCOPE: RoleScopeSummary = {
  actor_id: "p060",
  admin_surface_allowed: false,
  approval_scope: {
    has_approval_scope: true,
    pending_count: 1,
  },
  bursar_surface_allowed: false,
  confidence: "high",
  demo_identity_mode: true,
  department_scope: {
    band: 5,
    department_id: "Finance",
    seniority: "Leadership",
  },
  derived_level: "department_head",
  enforcement: "derived_only",
  governance_surface_allowed: false,
  project_scope: {
    capability_ids: ["cap31"],
    project_count: 1,
  },
  reasons: [
    "humanized actor profile is present",
    "reporting line has 2 direct reports",
    "sensitive surfaces remain disallowed by this contract",
  ],
  team_scope: {
    direct_report_count: 2,
    has_team_scope: true,
  },
};

const REQUESTS: AccessRequestsResponse = {
  actor_id: "p060",
  demo_identity_mode: true,
  requests: [
    {
      approver_id: "p001",
      created_ordinal: 0,
      justification: "Need context for assigned work.",
      request_id: "ar_123",
      request_key: "key",
      requester_id: "p060",
      snapshot_version: "snap",
      status: "pending",
      target: { kind: "project", capability_id: "cap31" },
    },
  ],
  snapshot_version: "snap",
};

const GRANTS: AccessGrantsResponse = {
  actor_id: "p060",
  demo_identity_mode: true,
  grants: [
    {
      approver_id: "p001",
      created_ordinal: 0,
      grant_id: "ag_123",
      grantee_id: "p060",
      permission: "read",
      reason: "manager_approved",
      request_id: "ar_approved",
      snapshot_version: "snap",
      status: "active",
      target: { kind: "project", capability_id: "cap31" },
    },
  ],
  snapshot_version: "snap",
};

const INBOX: AccessRequestsResponse = {
  actor_id: "p060",
  demo_identity_mode: true,
  requests: [],
  snapshot_version: "snap",
};

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
      item_id: "ar_123",
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
  ],
  provenance: {
    capability: { id: "cap31", name: "Access Review 31" },
    initiative: { id: "init03", name: "Strengthen Workforce Capability" },
    strategy: { id: "strat01", name: "Workforce Capability" },
    workflow: { id: "wf11", name: "Goods-In Verification 31" },
  },
  snapshot_version: "snap",
};

function stubDashboardFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/lens/p060")) return new Response(JSON.stringify(LENS), { status: 200 });
      if (url.endsWith("/graph")) return new Response(JSON.stringify(GRAPH), { status: 200 });
      if (url.endsWith("/node/p060/summary")) return new Response(JSON.stringify(SUMMARY), { status: 200 });
      if (url.endsWith("/me/scope")) return new Response(JSON.stringify(ROLE_SCOPE), { status: 200 });
      if (url.endsWith("/access-requests/inbox")) return new Response(JSON.stringify(INBOX), { status: 200 });
      if (url.endsWith("/access-grants")) return new Response(JSON.stringify(GRANTS), { status: 200 });
      if (url.endsWith("/access-requests")) return new Response(JSON.stringify(REQUESTS), { status: 200 });
      if (url.includes("/workflow/project/cap31")) return new Response(JSON.stringify(WORKFLOW), { status: 200 });
      return new Response("{\"demo_identity_mode\":true,\"error\":\"not found\"}", { status: 404 });
    }),
  );
}

describe("EmployeeDashboard", () => {
  it("renders the daily work surface from real-shaped API responses", async () => {
    stubDashboardFetch();
    const { container } = render(<EmployeeDashboard actor="p060" />);
    await waitFor(() => expect(screen.getByTestId("employee-dashboard")).toBeTruthy());

    expect(screen.getByTestId("dashboard-user-name").textContent).toBe("Felix Osei");
    expect(screen.getByTestId("dashboard-ask-link").getAttribute("href")).toBe("/ask?as=p060");
    expect(screen.getByTestId("dashboard-command-pods").textContent).toContain("My Work Pod");
    expect(screen.getByTestId("dashboard-command-pods").textContent).toContain("Project Context Pod");
    expect(screen.getByTestId("dashboard-command-pods").textContent).toContain("Team Lead Pod");
    expect(screen.getByTestId("dashboard-command-pods").textContent).toContain("Approval Queue Pod");
    const askPod = screen.getByTestId("dashboard-ask-pod");
    expect(askPod.textContent).toContain("Ask a Question");
    expect(askPod.textContent).toContain("Start Conversation");
    expect(askPod.getAttribute("href")).toBe("/ask?as=p060");
    const project = screen.getByTestId("dashboard-project");
    expect(project.textContent).toContain("Capability: Access Review 31");
    expect(project.getAttribute("href")).toBe("/project?cap=cap31&as=p060");

    const scope = screen.getByTestId("dashboard-scope");
    expect(scope.textContent).toContain("derived, not enforced");
    expect(scope.textContent).toContain("Role posture");
    expect(scope.textContent).toContain("Department head signal");
    expect(scope.textContent).toContain("Department context");
    expect(scope.textContent).toContain("Leadership");
    expect(scope.textContent).toContain("Team scope");
    expect(scope.textContent).toContain("Project scope");
    expect(scope.textContent).toContain("Read grants");
    expect(scope.textContent).toContain("Surface limits");
    expect(scope.textContent).toContain("Enforcement status");

    const workflowGroups = screen.getAllByTestId("dashboard-workflow-group");
    expect(within(workflowGroups[0]).getAllByTestId("dashboard-workflow-item").length).toBe(1);
    expect(within(workflowGroups[2]).getAllByTestId("dashboard-workflow-item").length).toBe(1);

    expect(screen.getByTestId("dashboard-agent").textContent).toContain("Finance analysis assistant");
    expect(screen.getByTestId("dashboard-request").textContent).toContain("pending");
    const grant = screen.getByTestId("dashboard-grant");
    expect(grant.textContent).toContain("read grant");
    expect(grant.textContent).toContain("active");
    expect(grant.getAttribute("href")).toBe("/project?cap=cap31&as=p060");
    expect(screen.getByTestId("dashboard-knowledge").textContent).toContain("Visible rows");

    const text = container.textContent ?? "";
    expect(text).not.toContain("document_id");
    expect(text).not.toContain("d0196");
    expect(text).not.toMatch(/denied count|hidden/i);
    expect(text).not.toMatch(/bursar|governance/i);
  });
});
