import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import type {
  AccessGrantRecord,
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
  sources: [
    { id: "source_slack", kind: "source", label: "Slack" },
    { id: "source_sharepoint", kind: "source", label: "SharePoint" },
  ],
  tools: [{ id: "tool_github", kind: "tool", label: "GitHub" }],
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
    "Work Identity record is present",
    "reporting line has 2 direct reports",
    "sensitive surfaces remain disallowed by this contract",
  ],
  team_scope: {
    direct_report_count: 2,
    has_team_scope: true,
  },
};

const EMPLOYEE_LENS: LensResponse = {
  ...LENS,
  actor: {
    ...LENS.actor,
    title: "Finance Analyst",
  },
  subject_human: {
    ...LENS.subject_human!,
    manages: [],
    seniority: "Associate",
    title: "Finance Analyst",
  },
};

const EMPLOYEE_ROLE_SCOPE: RoleScopeSummary = {
  ...ROLE_SCOPE,
  approval_scope: {
    has_approval_scope: false,
    pending_count: 0,
  },
  confidence: "high",
  department_scope: {
    ...ROLE_SCOPE.department_scope,
    seniority: "Associate",
  },
  derived_level: "employee",
  project_scope: {
    capability_ids: ["cap31"],
    project_count: 1,
  },
  reasons: [
    "Work Identity record is present",
    "project scope has 1 visible capability assignments",
    "sensitive surfaces remain disallowed by this contract",
  ],
  team_scope: {
    direct_report_count: 0,
    has_team_scope: false,
  },
};

const EXECUTIVE_CANDIDATE_ROLE_SCOPE: RoleScopeSummary = {
  ...EMPLOYEE_ROLE_SCOPE,
  confidence: "medium",
  department_scope: {
    band: 7,
    department_id: "Executive",
    seniority: "Executive",
  },
  derived_level: "executive_candidate",
  reasons: [
    "Work Identity record is present",
    "executive-like title/department is only a candidate signal",
    "sensitive surfaces remain disallowed by this contract",
  ],
};

const TEAM_LEAD_ROLE_SCOPE: RoleScopeSummary = {
  ...EMPLOYEE_ROLE_SCOPE,
  approval_scope: {
    has_approval_scope: true,
    pending_count: 1,
  },
  derived_level: "team_lead",
  reasons: [
    "Work Identity record is present",
    "reporting line has 1 direct report",
    "sensitive surfaces remain disallowed by this contract",
  ],
  team_scope: {
    direct_report_count: 1,
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
    {
      approver_id: "p060",
      created_ordinal: 1,
      grant_id: "ag_revoke",
      grantee_id: "p061",
      permission: "read",
      reason: "manager_approved",
      request_id: "ar_to_revoke",
      snapshot_version: "snap",
      status: "active",
      target: { kind: "project", capability_id: "cap31" },
    },
  ],
  snapshot_version: "snap",
};

const INACTIVE_GRANTS: AccessGrantsResponse = {
  ...GRANTS,
  grants: [
    {
      ...GRANTS.grants[0],
      grant_id: "ag_revoked",
      revoked_by: "p001",
      revoked_ordinal: 2,
      status: "revoked",
    },
    {
      ...GRANTS.grants[0],
      expires_at: "snap",
      grant_id: "ag_expired",
      status: "expired",
    },
  ],
};

const REVOKED_GRANT: AccessGrantRecord = {
  approver_id: "p060",
  created_ordinal: 1,
  grant_id: "ag_revoke",
  grantee_id: "p061",
  permission: "read",
  reason: "manager_approved",
  request_id: "ar_to_revoke",
  revocation_reason: "approver_revoked",
  revoked_by: "p060",
  revoked_ordinal: 2,
  snapshot_version: "snap",
  status: "revoked",
  target: { kind: "project", capability_id: "cap31" },
};

const INBOX: AccessRequestsResponse = {
  actor_id: "p060",
  demo_identity_mode: true,
  requests: [],
  snapshot_version: "snap",
};

const INBOX_WITH_REQUEST: AccessRequestsResponse = {
  ...INBOX,
  requests: [
    {
      approver_id: "p060",
      created_ordinal: 2,
      justification: "Needs approved project context.",
      request_id: "ar_team_approval",
      request_key: "team-key",
      requester_id: "p061",
      snapshot_version: "snap",
      status: "pending",
      target: { kind: "project", capability_id: "cap31" },
    },
  ],
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

const QUIET_WORKFLOW: ProjectWorkflowResponse = {
  ...WORKFLOW,
  items: [
    {
      ...WORKFLOW.items[0],
      item_id: "box_active_quiet",
      status: "active",
    },
  ],
};

function stubDashboardFetch({
  grants = GRANTS,
  graph = GRAPH,
  inbox = INBOX,
  lens = LENS,
  requests = REQUESTS,
  roleScope = ROLE_SCOPE,
  summary = SUMMARY,
  workflow = WORKFLOW,
}: {
  grants?: AccessGrantsResponse;
  graph?: GraphResponse;
  inbox?: AccessRequestsResponse;
  lens?: LensResponse;
  requests?: AccessRequestsResponse;
  roleScope?: RoleScopeSummary;
  summary?: NodeSummary;
  workflow?: ProjectWorkflowResponse;
} = {}) {
  const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (url.endsWith("/access-grants/ag_revoke/revoke") && init?.method === "POST") {
      return new Response(
        JSON.stringify({
          demo_identity_mode: true,
          grant: REVOKED_GRANT,
          snapshot_version: "snap",
        }),
        { status: 200 },
      );
    }
    if (url.endsWith("/lens/p060")) return new Response(JSON.stringify(lens), { status: 200 });
    if (url.endsWith("/graph")) return new Response(JSON.stringify(graph), { status: 200 });
    if (url.endsWith("/node/p060/summary")) return new Response(JSON.stringify(summary), { status: 200 });
    if (url.endsWith("/me/scope")) return new Response(JSON.stringify(roleScope), { status: 200 });
    if (url.endsWith("/access-requests/inbox")) return new Response(JSON.stringify(inbox), { status: 200 });
    if (url.endsWith("/access-grants")) return new Response(JSON.stringify(grants), { status: 200 });
    if (url.endsWith("/access-requests")) return new Response(JSON.stringify(requests), { status: 200 });
    if (url.includes("/workflow/project/cap31")) return new Response(JSON.stringify(workflow), { status: 200 });
    return new Response("{\"demo_identity_mode\":true,\"error\":\"not found\"}", { status: 404 });
  });
  vi.stubGlobal(
    "fetch",
    fetcher,
  );
  return fetcher;
}

describe("EmployeeDashboard", () => {
  it("renders the daily work surface from real-shaped API responses", async () => {
    const fetcher = stubDashboardFetch();
    const { container } = render(<EmployeeDashboard actor="p060" />);
    await waitFor(() => expect(screen.getByTestId("employee-dashboard")).toBeTruthy());

    expect(screen.getByTestId("dashboard-user-name").textContent).toBe("Felix Osei");
    expect(screen.getByTestId("dashboard-ask-link").getAttribute("href")).toBe("/ask?as=p060");
    expect(screen.getByTestId("dashboard-compact-cockpit")).toBeTruthy();
    expect(screen.getByTestId("dashboard-panel-tabs").textContent).toContain("Workspace");
    expect(screen.getByTestId("dashboard-panel-tabs").textContent).toContain("Settings");
    // Profile pill removed; Profile opens from the identity strip (avatar + name).
    expect(screen.getByTestId("dashboard-panel-tabs").textContent).not.toContain("Profile");
    expect(screen.queryByTestId("dashboard-profile-panel-trigger")).toBeNull();
    expect(screen.getByTestId("dashboard-identity-open-profile")).toBeTruthy();
    expect(screen.queryByTestId("dashboard-workspace")).toBeNull();

    const today = screen.getByTestId("dashboard-today-cockpit");
    expect(today.textContent).toContain("Today");
    expect(today.textContent).toContain("What needs attention?");
    expect(today.textContent).toContain("Workflow waiting");
    expect(today.textContent).toContain("Continue active workflow");
    expect(today.textContent).toContain("Waiting on approval");
    expect(today.textContent).toContain("Manager Context");
    expect(today.textContent).toContain("Team context");
    expect(today.textContent).toContain("Department workflow summary");
    expect(screen.getByTestId("dashboard-today-needs-attention").textContent).not.toContain("Approval queue");
    expect(today.textContent).not.toMatch(/unread|overdue|risk score|bursar|governance/i);

    fireEvent.click(screen.getByTestId("dashboard-workspace-panel-trigger"));
    const workspace = await screen.findByTestId("dashboard-workspace");
    const notifications = screen.getByTestId("dashboard-notification-center");
    expect(notifications.textContent).toContain("Action summary");
    expect(notifications.textContent).toContain("Request status");
    expect(notifications.textContent).toContain("Approval queue");
    expect(notifications.textContent).toContain("Workflow attention");
    expect(notifications.textContent).toContain("Grant ledger events");
    expect(notifications.textContent).toContain("Team scope available");
    expect(notifications.textContent).toContain("Department context available");
    expect(notifications.textContent).not.toMatch(/unread/i);

    const commandLayer = screen.getByTestId("dashboard-workflow-command");
    expect(commandLayer.textContent).toContain("Workflow Command");
    expect(commandLayer.textContent).toContain("Requests");
    expect(commandLayer.textContent).toContain("Approvals");
    expect(commandLayer.textContent).toContain("Department Updates");
    const approvalCommand = screen
      .getAllByTestId("dashboard-workflow-command-item")
      .find((item) => item.textContent?.includes("Approvals"));
    expect(approvalCommand?.textContent).toContain("Approval scope exists");
    expect(approvalCommand?.textContent).not.toContain("1");

    expect(workspace.textContent).toContain("Workspace");
    expect(workspace.textContent).toContain("Work needing a decision");
    expect(workspace.textContent).toContain("Requests and approvals");
    expect(workspace.textContent).toContain("Granted Knowledge");
    expect(workspace.textContent).toContain("Workflow alerts");
    expect(workspace.textContent).toContain("Team and department context");

    const askPod = screen.getByTestId("dashboard-ask-agent-card");
    expect(askPod.textContent).toContain("Ask AI agent");
    expect(askPod.textContent).toContain("Ask with your current access");
    expect(askPod.getAttribute("href")).toBe("/ask?as=p060&grant=ag_123&cap=cap31");
    const project = screen.getByTestId("dashboard-project");
    expect(project.textContent).toContain("Capability: Access Review 31");
    expect(project.getAttribute("href")).toBe("/project?cap=cap31&as=p060");

    const roleWorkflow = screen.getByTestId("dashboard-role-aware-workflow");
    expect(roleWorkflow.textContent).toContain("My work plus visible leadership context");
    expect(screen.getByTestId("dashboard-employee-workflow-layer").textContent).toContain("2 workflow items");
    expect(screen.getByTestId("dashboard-team-workflow-layer").textContent).toContain("2 direct reports");
    expect(screen.getByTestId("dashboard-department-workflow-layer").textContent).toContain("Finance");
    expect(screen.queryByTestId("dashboard-approval-workflow-layer")).toBeNull();

    // /me shows a compact digest (no lane board); the full board lives on Workflow Command.
    expect(screen.queryAllByTestId("dashboard-workflow-group").length).toBe(0);
    expect(screen.getAllByTestId("dashboard-workflow-item").length).toBe(2);
    expect(screen.getByTestId("dashboard-workflow-open").getAttribute("href")).toBe("/project?as=p060");

    expect(screen.getByTestId("dashboard-request").textContent).toContain("pending");
    const grants = screen.getAllByTestId("dashboard-grant");
    const grant = grants.find((row) => row.textContent?.includes("ag_123"));
    expect(grant).toBeTruthy();
    expect(grant?.textContent).toContain("read grant");
    expect(grant?.textContent).toContain("active");
    expect(within(grant!).getByRole("link").getAttribute("href")).toBe("/project?cap=cap31&as=p060");

    const grantedKnowledge = screen.getByTestId("dashboard-granted-knowledge");
    expect(grantedKnowledge.textContent).toContain("ag_123");
    expect(grantedKnowledge.textContent).not.toContain("ag_revoke");
    expect(screen.getByTestId("dashboard-open-grant-ask").getAttribute("href")).toBe(
      "/ask?as=p060&grant=ag_123&cap=cap31",
    );

    const revokeButton = screen.getByTestId("dashboard-grant-revoke");
    expect(revokeButton.textContent).toBe("Revoke");
    fireEvent.click(revokeButton);
    await waitFor(() => expect(screen.getByText("revoked by p060")).toBeTruthy());
    expect(fetcher).toHaveBeenCalledWith(
      expect.stringContaining("/access-grants/ag_revoke/revoke"),
      expect.objectContaining({
        body: JSON.stringify({ reason_code: "approver_revoked" }),
        method: "POST",
      }),
    );

    fireEvent.click(screen.getByTestId("dashboard-identity-open-profile"));
    const profile = await screen.findByTestId("dashboard-profile-panel");
    expect(profile.textContent).toContain("Profile");
    expect(profile.textContent).toContain("Identity");
    expect(profile.textContent).toContain("Role");
    expect(profile.textContent).toContain("Department");
    expect(profile.textContent).toContain("Manager");
    expect(profile.textContent).toContain("Security");
    expect(profile.textContent).toContain("Audit Activity");
    expect(profile.textContent).toContain("Ingrid Cohen");
    // Connected Systems / Preferences moved to Settings (IA pass): not on Profile.
    expect(profile.textContent).not.toContain("Connected Systems");
    expect(screen.queryByTestId("dashboard-connected-systems")).toBeNull();
    expect(screen.getByTestId("dashboard-agent").textContent).toContain("Finance analysis assistant");

    const scope = screen.getByTestId("dashboard-scope");
    expect(scope.textContent).toContain("permission preview");
    expect(scope.textContent).toContain("Role posture");
    expect(scope.textContent).toContain("Department head signal");
    expect(scope.textContent).toContain("Department context");
    expect(scope.textContent).toContain("Leadership");
    expect(scope.textContent).toContain("Team scope");
    expect(scope.textContent).toContain("Project scope");
    expect(scope.textContent).toContain("Read grants");
    expect(scope.textContent).toContain("Surface limits");
    expect(scope.textContent).toContain("Identity boundary");
    expect(scope.textContent).toContain("Production identity binding");

    const roleExperience = screen.getByTestId("dashboard-role-experience");
    expect(roleExperience.textContent).toContain("Department head");
    expect(roleExperience.textContent).toContain("Team scope");
    expect(roleExperience.textContent).toContain("Approval scope");
    expect(roleExperience.textContent).toContain("Granted knowledge");
    expect(roleExperience.textContent).toContain("Surface boundary");
    expect(roleExperience.textContent).toContain("permission preview");
    expect(roleExperience.textContent).not.toMatch(/bursar|governance/i);
    expect(screen.getByTestId("dashboard-knowledge").textContent).toContain("Visible rows");

    fireEvent.click(screen.getByTestId("dashboard-settings-panel-trigger"));
    const settings = await screen.findByTestId("dashboard-settings-panel");
    expect(settings.textContent).toContain("Settings");
    expect(settings.textContent).toContain("Light or dark mode");
    // Connected Systems / Preferences / Agent Preferences relocated here (IA pass).
    expect(settings.textContent).toContain("Connected Systems");
    expect(settings.textContent).toContain("Preferences");
    expect(settings.textContent).toContain("Agent Preferences");
    expect(settings.textContent).toContain("Demo Identity Mode");
    expect(screen.getAllByTestId("theme-toggle").length).toBeGreaterThan(0);
    const settingsSystems = screen.getByTestId("dashboard-connected-systems");
    expect(settingsSystems.textContent).toContain("Slack");
    expect(settingsSystems.textContent).toContain("SharePoint");
    expect(settingsSystems.textContent).toContain("GitHub");
    expect(settingsSystems.textContent).toContain("Available");
    expect(settingsSystems.textContent).not.toContain("Gmail");

    const text = container.textContent ?? "";
    expect(text).not.toContain("document_id");
    expect(text).not.toContain("d0196");
    expect(text).not.toMatch(/denied count|hidden/i);
    expect(text).not.toMatch(/bursar|governance/i);
    expect(text).not.toMatch(/supplier|invoice|procurement|spend total|token total/i);
    expect(container.querySelector("[data-testid='bursar-surface']")).toBeNull();
    expect(container.querySelector("a[href='/admin/bursar']")).toBeNull();
  });

  it("keeps ordinary employee pods to employee-safe surfaces only", async () => {
    stubDashboardFetch({
      grants: INACTIVE_GRANTS,
      inbox: { ...INBOX, requests: [] },
      lens: EMPLOYEE_LENS,
      requests: { ...REQUESTS, requests: [] },
      roleScope: EMPLOYEE_ROLE_SCOPE,
      summary: { ...SUMMARY, agents_owned: [] },
    });
    const { container } = render(<EmployeeDashboard actor="p060" />);
    await waitFor(() => expect(screen.getByTestId("employee-dashboard")).toBeTruthy());

    expect(screen.getByTestId("dashboard-compact-cockpit")).toBeTruthy();
    expect(screen.getByTestId("dashboard-main-cockpit").textContent).toContain("My Projects");
    expect(screen.getByTestId("dashboard-main-cockpit").textContent).toContain("My Workflow");
    expect(screen.getByTestId("dashboard-ask-agent-card").textContent).toContain("Ask AI agent");
    expect(screen.queryByTestId("dashboard-workspace")).toBeNull();

    const today = screen.getByTestId("dashboard-today-cockpit");
    expect(today.textContent).toContain("Today");
    expect(today.textContent).toContain("Needs Attention");
    expect(today.textContent).not.toContain("Manager Context");
    expect(screen.queryByTestId("dashboard-today-manager-context")).toBeNull();
    expect(today.textContent).not.toMatch(/unread|overdue|risk score|bursar|governance/i);
    fireEvent.click(screen.getByTestId("dashboard-workspace-panel-trigger"));
    await screen.findByTestId("dashboard-workspace");
    const notifications = screen.getByTestId("dashboard-notification-center");
    expect(notifications.textContent).not.toContain("Team scope available");
    expect(notifications.textContent).not.toContain("Department context available");
    expect(notifications.textContent).not.toContain("Approval queue");

    expect(screen.getByTestId("dashboard-workflow-command").textContent).not.toMatch(/Team Updates|Department Updates/);
    expect(screen.getByTestId("dashboard-employee-workflow-layer").textContent).toContain("Employee Layer");
    expect(screen.queryByTestId("dashboard-team-workflow-layer")).toBeNull();
    expect(screen.queryByTestId("dashboard-department-workflow-layer")).toBeNull();
    expect(screen.queryByTestId("dashboard-approval-workflow-layer")).toBeNull();
    expect(screen.queryByTestId("dashboard-executive-workflow-label")).toBeNull();

    fireEvent.click(screen.getByTestId("dashboard-identity-open-profile"));
    const roleExperience = screen.getByTestId("dashboard-role-experience");
    expect(roleExperience.textContent).toContain("Employee baseline");
    expect(roleExperience.textContent).toContain("Surface boundary");
    expect(roleExperience.textContent).toContain("permission preview");
    expect(roleExperience.textContent).not.toContain("Team scope");
    expect(roleExperience.textContent).not.toContain("Department head");
    expect(roleExperience.textContent).not.toContain("Approval queue");

    expect(screen.queryByTestId("dashboard-open-grant-ask")).toBeNull();
    const text = container.textContent ?? "";
    expect(text).not.toMatch(/bursar|governance/i);
    expect(text).not.toMatch(/supplier|invoice|procurement|spend total|token total/i);
    expect(container.querySelector("[data-testid='bursar-surface']")).toBeNull();
    expect(container.querySelector("a[href='/admin/bursar']")).toBeNull();
  });

  it("shows an honest Today empty state when no real attention rows exist", async () => {
    stubDashboardFetch({
      grants: { ...GRANTS, grants: [] },
      inbox: { ...INBOX, requests: [] },
      lens: EMPLOYEE_LENS,
      requests: { ...REQUESTS, requests: [] },
      roleScope: EMPLOYEE_ROLE_SCOPE,
      summary: { ...SUMMARY, agents_owned: [] },
      workflow: QUIET_WORKFLOW,
    });
    const { container } = render(<EmployeeDashboard actor="p060" />);
    await waitFor(() => expect(screen.getByTestId("employee-dashboard")).toBeTruthy());

    const today = screen.getByTestId("dashboard-today-cockpit");
    const needsAttention = screen.getByTestId("dashboard-today-needs-attention");
    expect(today.textContent).toContain("0 attention rows");
    expect(needsAttention.textContent).toContain("Nothing waiting.");
    expect(today.textContent).toContain("Continue active workflow");
    expect(today.textContent).toContain("No requests waiting.");
    expect(today.textContent).not.toMatch(/unread|notification count|overdue|risk score|bursar|governance/i);
    expect(container.querySelector("[data-testid='bursar-surface']")).toBeNull();
    expect(container.querySelector("a[href='/admin/bursar']")).toBeNull();
  });

  it("adds team lead cockpit value only from real inbox and team scope rows", async () => {
    stubDashboardFetch({
      grants: { ...GRANTS, grants: [] },
      inbox: INBOX_WITH_REQUEST,
      lens: EMPLOYEE_LENS,
      requests: { ...REQUESTS, requests: [] },
      roleScope: TEAM_LEAD_ROLE_SCOPE,
      summary: { ...SUMMARY, agents_owned: [] },
    });
    const { container } = render(<EmployeeDashboard actor="p060" />);
    await waitFor(() => expect(screen.getByTestId("employee-dashboard")).toBeTruthy());

    const today = screen.getByTestId("dashboard-today-cockpit");
    const managerContext = screen.getByTestId("dashboard-today-manager-context");
    expect(today.textContent).toContain("Approval queue");
    expect(today.textContent).toContain("Approve");
    expect(managerContext.textContent).toContain("Team requests");
    expect(managerContext.textContent).toContain("Team context");
    expect(managerContext.textContent).toContain("Team workflow rows are not modeled here");
    expect(managerContext.textContent).not.toContain("Department workflow summary");
    fireEvent.click(screen.getByTestId("dashboard-workspace-panel-trigger"));
    await screen.findByTestId("dashboard-workspace");
    expect(screen.getByTestId("dashboard-employee-workflow-layer").textContent).toContain("Employee Layer");
    expect(screen.getByTestId("dashboard-team-workflow-layer").textContent).toContain("1 direct report");
    expect(screen.queryByTestId("dashboard-department-workflow-layer")).toBeNull();
    expect(today.textContent).not.toMatch(/unread|notification count|overdue|risk score|bursar|governance/i);
    expect(container.querySelector("[data-testid='bursar-surface']")).toBeNull();
    expect(container.querySelector("a[href='/admin/bursar']")).toBeNull();
  });

  it("labels executive candidates without unlocking elevated dashboard pods", async () => {
    stubDashboardFetch({
      grants: { ...GRANTS, grants: [] },
      inbox: { ...INBOX, requests: [] },
      lens: EMPLOYEE_LENS,
      requests: { ...REQUESTS, requests: [] },
      roleScope: EXECUTIVE_CANDIDATE_ROLE_SCOPE,
      summary: { ...SUMMARY, agents_owned: [] },
    });
    const { container } = render(<EmployeeDashboard actor="p060" />);
    await waitFor(() => expect(screen.getByTestId("employee-dashboard")).toBeTruthy());

    expect(screen.getByTestId("dashboard-compact-cockpit")).toBeTruthy();
    expect(screen.getByTestId("dashboard-main-cockpit").textContent).toContain("My Projects");
    expect(screen.getByTestId("dashboard-main-cockpit").textContent).toContain("My Workflow");
    expect(screen.getByTestId("dashboard-main-cockpit").textContent).not.toContain("Executive Candidate");
    const today = screen.getByTestId("dashboard-today-cockpit");
    expect(today.textContent).toContain("Today");
    expect(today.textContent).not.toContain("Manager Context");
    expect(today.textContent).not.toMatch(/admin|bursar|governance|unread|notification count/i);
    fireEvent.click(screen.getByTestId("dashboard-workspace-panel-trigger"));
    await screen.findByTestId("dashboard-workspace");
    expect(screen.getByTestId("dashboard-employee-workflow-layer").textContent).toContain("Employee Layer");
    expect(screen.getByTestId("dashboard-executive-workflow-label").textContent).toContain("label only");
    expect(screen.queryByTestId("dashboard-team-workflow-layer")).toBeNull();
    expect(screen.queryByTestId("dashboard-department-workflow-layer")).toBeNull();
    expect(screen.queryByTestId("dashboard-approval-workflow-layer")).toBeNull();

    fireEvent.click(screen.getByTestId("dashboard-identity-open-profile"));
    const roleExperience = screen.getByTestId("dashboard-role-experience");
    expect(roleExperience.textContent).toContain("Executive candidate");
    expect(roleExperience.textContent).toContain("label only");
    expect(roleExperience.textContent).toContain("Surface boundary");
    expect(container.textContent ?? "").not.toMatch(/bursar|governance/i);
    expect(container.textContent ?? "").not.toMatch(/supplier|invoice|procurement|spend total|token total/i);
    expect(container.querySelector("[data-testid='bursar-surface']")).toBeNull();
    expect(container.querySelector("a[href='/admin/bursar']")).toBeNull();
  });
});
