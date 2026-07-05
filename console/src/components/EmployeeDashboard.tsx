"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import * as api from "@/lib/api";
import type {
  AccessGrantRecord,
  AccessRequestRecord,
  GraphProject,
  GraphResponse,
  LensResponse,
  NodeSummary,
  ProjectRecord,
  ProjectWorkflowResponse,
  RoleScopeSummary,
  WorkflowItem,
} from "@/lib/api";
import { TYPE } from "@/lib/tokens";
import { useModalDialogFocus } from "./A11yDialog";
import { MotionAnchor, MotionArticle, MotionSection } from "./MotionPrimitives";
import { PersonAvatar } from "./PersonAvatar";
import { Skeleton } from "./Skeleton";
import { ThemeToggle } from "./ThemeToggle";

const WORKFLOW_GROUPS = [
  { label: "In Progress", statuses: ["active"] },
  { label: "Next", statuses: ["candidate", "planned"] },
  { label: "Waiting", statuses: ["pending"] },
  { label: "Blocked", statuses: ["blocked", "denied", "cancelled", "expired", "dismissed"] },
  { label: "Done", statuses: ["done", "approved"] },
];

interface ScopeBadge {
  detail: string;
  label: string;
  source: string;
}

interface NotificationItem {
  category:
    | "Requests"
    | "Approvals"
    | "Workflow Alerts"
    | "Grant Expiry"
    | "Grant Events"
    | "Team Updates"
    | "Department Updates";
  detail: string;
  href: string;
  metric?: string;
  source: string;
  title: string;
}

interface ConnectedSystem {
  name: string;
  source: string;
  status: "Available";
}

type DashboardPanelMode = "workspace" | "profile" | "settings";

const CONNECTOR_NAMES = ["Gmail", "Outlook", "Teams", "Slack", "Jira", "GitHub", "SharePoint"];

function workflowGroup(status: string): string {
  for (const group of WORKFLOW_GROUPS) {
    if (group.statuses.includes(status)) return group.label;
  }
  return "Next";
}

function workflowStatusLabel(status: string): string {
  switch (status.toLowerCase()) {
    case "active":
      return "In progress";
    case "pending":
      return "Waiting";
    case "blocked":
      return "Blocked";
    case "denied":
      return "Denied";
    case "cancelled":
      return "Cancelled";
    case "expired":
      return "Expired";
    case "dismissed":
      return "Dismissed";
    case "done":
      return "Done";
    case "approved":
      return "Approved";
    case "planned":
      return "Planned";
    case "candidate":
    default:
      return "Next";
  }
}

// B2: dashboard panels are SOLID elevation — glass belongs to overlays only.
function dashboardPanelStyle(): React.CSSProperties {
  return {
    background: "var(--surface-1)",
    boxShadow: "var(--shadow-1)",
  };
}

function Chip({ children, mono = false }: { children: React.ReactNode; mono?: boolean }) {
  return (
    <span
      className={`ap-chip ${mono ? "ap-register-evidence" : "ap-register-chrome"} rounded-lg px-1.5 py-0.5`}
      style={{ fontSize: TYPE.scale.xs }}
    >
      {children}
    </span>
  );
}

function roleLabel(level: RoleScopeSummary["derived_level"]): string {
  switch (level) {
    case "department_head":
      return "Department head signal";
    case "executive_candidate":
      return "Executive candidate signal";
    case "super_admin_candidate":
      return "Restricted-surface candidate signal";
    case "team_lead":
      return "Team lead signal";
    case "employee":
    default:
      return "Employee view";
  }
}

function isExecutiveCandidate(level: RoleScopeSummary["derived_level"] | null | undefined): boolean {
  return level === "executive_candidate" || level === "super_admin_candidate";
}

function isDepartmentHead(level: RoleScopeSummary["derived_level"] | null | undefined): boolean {
  return level === "department_head";
}

function scopeModeLabel(roleScope: RoleScopeSummary | null | undefined): string {
  return roleScope ? "permission preview" : "scope unavailable";
}

function activeKnowledgeGrants(grants: AccessGrantRecord[], actor: string): AccessGrantRecord[] {
  return grants.filter((grant) => grant.status === "active" && grant.grantee_id === actor);
}

function buildNotificationItems({
  actor,
  grants,
  inbox,
  requests,
  roleScope,
  workflowItems,
}: {
  actor: string;
  grants: AccessGrantRecord[];
  inbox: AccessRequestRecord[];
  requests: AccessRequestRecord[];
  roleScope: RoleScopeSummary | null;
  workflowItems: WorkflowItem[];
}): NotificationItem[] {
  const notifications: NotificationItem[] = [];
  const pendingRequests = requests.filter((request) => request.status === "pending");
  const workflowAlerts = workflowItems.filter((item) =>
    ["pending", "blocked", "denied", "cancelled", "expired", "dismissed"].includes(item.status.toLowerCase()),
  );
  const visibleGrants = grants.filter((grant) => grant.grantee_id === actor || grant.approver_id === actor);
  const grantExpiry = visibleGrants.filter((grant) => grant.status === "expired" || Boolean(grant.expires_at));

  if (requests.length > 0) {
    notifications.push({
      category: "Requests",
      detail:
        pendingRequests.length > 0
          ? `${pendingRequests.length} submitted access ${pendingRequests.length === 1 ? "request is" : "requests are"} pending.`
          : "Submitted access requests have ledger status.",
      href: "#dashboard-requests",
      metric: `${requests.length}`,
      source: "access request ledger",
      title: "Request status",
    });
  }

  if (inbox.length > 0 || roleScope?.approval_scope.has_approval_scope) {
    notifications.push({
      category: "Approvals",
      detail:
        inbox.length > 0
          ? "Requests assigned to this Work Identity are ready for review."
          : "Approval scope exists, but no request inbox rows are visible here.",
      href: "#dashboard-requests",
      metric: inbox.length > 0 ? `${inbox.length}` : undefined,
      source: inbox.length > 0 ? "approval inbox" : "server scope",
      title: "Approval queue",
    });
  }

  if (workflowAlerts.length > 0) {
    notifications.push({
      category: "Workflow Alerts",
      detail: "Waiting or blocked workflow rows are projected from real project workflow data.",
      href: "#dashboard-workflow",
      metric: `${workflowAlerts.length}`,
      source: "workflow projection",
      title: "Workflow attention",
    });
  }

  if (grantExpiry.length > 0) {
    notifications.push({
      category: "Grant Expiry",
      detail: "Grant rows with expiry status or expiry metadata are present in the grant ledger.",
      href: "#dashboard-requests",
      metric: `${grantExpiry.length}`,
      source: "grant ledger",
      title: "Grant expiry status",
    });
  }

  if (visibleGrants.length > 0) {
    notifications.push({
      category: "Grant Events",
      detail: "Read grant records visible to this Work Identity are available for status review.",
      href: "#dashboard-granted-knowledge",
      metric: `${visibleGrants.length}`,
      source: "grant ledger",
      title: "Grant ledger events",
    });
  }

  if (roleScope?.team_scope.has_team_scope) {
    notifications.push({
      category: "Team Updates",
      detail: "Team scope exists from reporting-line facts. A team update event stream is not modeled yet.",
      href: "#dashboard-scope",
      source: "reporting line",
      title: "Team scope available",
    });
  }

  if (isDepartmentHead(roleScope?.derived_level) && roleScope?.department_scope.department_id) {
    notifications.push({
      category: "Department Updates",
      detail: "Department posture is derived from server scope. A department update event stream is not modeled yet.",
      href: "#dashboard-role-aware-workflow",
      source: "server scope",
      title: "Department context available",
    });
  }

  return notifications;
}

function deriveConnectedSystems(graph: GraphResponse | null): ConnectedSystem[] {
  const records = [
    ...(graph?.sources.map((source) => ({ id: source.id, label: source.label })) ?? []),
    ...(graph?.tools.map((tool) => ({ id: tool.id, label: tool.label })) ?? []),
  ];

  return CONNECTOR_NAMES.flatMap((name) => {
    const match = records.find((record) => record.label.toLowerCase().includes(name.toLowerCase()));
    if (!match) return [];
    return [{ name, source: match.id, status: "Available" as const }];
  });
}

function deriveScopeBadges({
  grants,
  human,
  inbox,
  requests,
  roleScope,
  summary,
  subjectDepartment,
}: {
  grants: AccessGrantRecord[];
  human: LensResponse["subject_human"];
  inbox: AccessRequestRecord[];
  requests: AccessRequestRecord[];
  roleScope: RoleScopeSummary | null;
  summary: NodeSummary | null;
  subjectDepartment: string | null | undefined;
}): ScopeBadge[] {
  if (roleScope) {
    const badges: ScopeBadge[] = [
      {
        detail: roleLabel(roleScope.derived_level),
        label: "Role posture",
        source: "server scope",
      },
      {
        detail: roleScope.department_scope.department_id,
        label: "Department context",
        source: "server scope",
      },
      {
        detail: roleScope.department_scope.seniority,
        label: "Seniority signal",
        source: "people record",
      },
      {
        detail: `${roleScope.project_scope.project_count} assigned ${
          roleScope.project_scope.project_count === 1 ? "project" : "projects"
        }`,
        label: "Project scope",
        source: "server scope",
      },
    ];

    if (roleScope.team_scope.has_team_scope) {
      badges.push({
        detail: `${roleScope.team_scope.direct_report_count} direct ${
          roleScope.team_scope.direct_report_count === 1 ? "report" : "reports"
        }`,
        label: "Team scope",
        source: "reporting line",
      });
    }
    if (summary?.agents_owned?.length) {
      badges.push({
        detail: `${summary.agents_owned.length} owned ${
          summary.agents_owned.length === 1 ? "agent" : "agents"
        }`,
        label: "Agent scope",
        source: "node summary",
      });
    }
    if (roleScope.approval_scope.pending_count > 0) {
      badges.push({
        detail: `${roleScope.approval_scope.pending_count} pending ${
          roleScope.approval_scope.pending_count === 1 ? "approval" : "approvals"
        }`,
        label: "Approver queue",
        source: "request inbox",
      });
    }
    if (requests.length > 0) {
      badges.push({
        detail: `${requests.length} submitted ${requests.length === 1 ? "request" : "requests"}`,
        label: "Request status",
        source: "request ledger",
      });
    }
    if (grants.length > 0) {
      badges.push({
        detail: `${grants.length} active read ${grants.length === 1 ? "grant" : "grants"}`,
        label: "Read grants",
        source: "grant ledger",
      });
    }
    badges.push({
      detail: "restricted surfaces unavailable",
      label: "Surface limits",
      source: "scope contract",
    });

    return badges;
  }

  const department = human?.department_label ?? subjectDepartment ?? null;
  const directReports = human?.manages.length ?? summary?.manages ?? 0;
  const projectCount = human?.projects.length ?? 0;
  const agentCount = summary?.agents_owned?.length ?? 0;
  const badges: ScopeBadge[] = [
    {
      detail: "daily work surface",
      label: "Employee view",
      source: "selected Work Identity",
    },
  ];

  if (department) {
    badges.push({
      detail: department,
      label: "Department context",
      source: "people record",
    });
  }
  if (human?.seniority) {
    badges.push({
      detail: human.seniority,
      label: "Seniority signal",
      source: "people record",
    });
  }
  if (directReports > 0) {
    badges.push({
      detail: `${directReports} direct ${directReports === 1 ? "report" : "reports"}`,
      label: "Team lead signal",
      source: "reporting line",
    });
  }
  if (projectCount > 0) {
    badges.push({
      detail: `${projectCount} assigned ${projectCount === 1 ? "project" : "projects"}`,
      label: "Project scope",
      source: "Work Identity projects",
    });
  }
  if (agentCount > 0) {
    badges.push({
      detail: `${agentCount} owned ${agentCount === 1 ? "agent" : "agents"}`,
      label: "Agent scope",
      source: "node summary",
    });
  }
  if (inbox.length > 0) {
    badges.push({
      detail: `${inbox.length} pending ${inbox.length === 1 ? "approval" : "approvals"}`,
      label: "Approver queue",
      source: "request inbox",
    });
  }
  if (requests.length > 0) {
    badges.push({
      detail: `${requests.length} submitted ${requests.length === 1 ? "request" : "requests"}`,
      label: "Request status",
      source: "request ledger",
    });
  }
  if (grants.length > 0) {
    badges.push({
      detail: `${grants.length} active read ${grants.length === 1 ? "grant" : "grants"}`,
      label: "Read grants",
      source: "grant ledger",
    });
  }

  return badges;
}

function Panel({
  action,
  children,
  delayIndex = 0,
  title,
}: {
  action?: React.ReactNode;
  children: React.ReactNode;
  delayIndex?: number;
  title: string;
}) {
  return (
    <MotionSection className="ap-card rounded-2xl p-4" delayIndex={delayIndex} style={dashboardPanelStyle()}>
      <div className="mb-3 flex items-baseline justify-between gap-3">
        <h2 className="ap-register-chrome" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
          {title}
        </h2>
        {action}
      </div>
      {children}
    </MotionSection>
  );
}

const TODAY_WORKFLOW_STATUSES = new Set(["pending", "blocked"]);

type CockpitAction = "Review" | "Open" | "Ask" | "Request" | "Approve" | "Continue";

interface CockpitItem {
  action: CockpitAction;
  detail: string;
  href: string;
  metric: string;
  source: string;
  title: string;
  tone: "attention" | "steady" | "ask" | "waiting" | "manager";
}

interface TodayCockpitModel {
  askWithContext: CockpitItem[];
  continueWork: CockpitItem[];
  managerRows: CockpitItem[];
  needsAttention: CockpitItem[];
  waitingOn: CockpitItem[];
}

function plural(count: number, singular: string, pluralLabel = `${singular}s`): string {
  return `${count} ${count === 1 ? singular : pluralLabel}`;
}

function capabilityTitle(capabilityId: string, projectById: Map<string, GraphProject>): string {
  return projectById.get(capabilityId)?.label.replace(/^Capability:\s*/i, "") ?? capabilityId;
}

function projectHref(actor: string, capabilityId: string): string {
  return `/project?cap=${encodeURIComponent(capabilityId)}&as=${encodeURIComponent(actor)}`;
}

function askGrantHref(actor: string, grant: AccessGrantRecord): string {
  return `/ask?as=${encodeURIComponent(actor)}&grant=${encodeURIComponent(
    grant.grant_id,
  )}&cap=${encodeURIComponent(grant.target.capability_id)}`;
}

function buildTodayCockpit({
  actor,
  grants,
  inbox,
  projectById,
  projects,
  requests,
  roleScope,
  workflowItems,
}: {
  actor: string;
  grants: AccessGrantRecord[];
  inbox: AccessRequestRecord[];
  projectById: Map<string, GraphProject>;
  projects: ProjectRecord[];
  requests: AccessRequestRecord[];
  roleScope: RoleScopeSummary | null;
  workflowItems: WorkflowItem[];
}): TodayCockpitModel {
  const activeGrants = activeKnowledgeGrants(grants, actor);
  const visibleGrants = grants.filter((grant) => grant.grantee_id === actor || grant.approver_id === actor);
  const grantStatusRows = visibleGrants.filter((grant) => grant.status !== "active" || Boolean(grant.expires_at));
  const pendingRequests = requests.filter((request) => request.status === "pending");
  const waitingWorkflowItems = workflowItems.filter((item) => TODAY_WORKFLOW_STATUSES.has(item.status.toLowerCase()));
  const activeWorkflowItems = workflowItems.filter((item) => item.status.toLowerCase() === "active");
  const departmentId = roleScope?.department_scope.department_id ?? null;
  const departmentCapabilityIds = new Set(
    projects.flatMap((project) => {
      const graphProject = projectById.get(project.capability_id);
      if (!departmentId || !graphProject) return [];
      return graphProject.departments.includes(departmentId) || graphProject.primary_department_id === departmentId
        ? [project.capability_id]
        : [];
    }),
  );
  const departmentWorkflowItems = workflowItems.filter((item) => departmentCapabilityIds.has(item.capability_id));

  const needsAttention: CockpitItem[] = [];
  if (inbox.length > 0) {
    const firstRequest = inbox[0];
    needsAttention.push({
      action: "Approve",
      detail: "Requests assigned to this Work Identity are ready for review.",
      href: "#dashboard-requests",
      metric: plural(inbox.length, "request"),
      source: "approval inbox",
      title: "Approval queue",
      tone: "attention",
    });
    if (firstRequest) {
      needsAttention[needsAttention.length - 1].detail = `Review ${capabilityTitle(
        firstRequest.target.capability_id,
        projectById,
      )} and other assigned request rows.`;
    }
  }
  if (waitingWorkflowItems.length > 0) {
    const firstItem = waitingWorkflowItems[0];
    needsAttention.push({
      action: "Open",
      detail: `${firstItem.title} is waiting or blocked in real project workflow data.`,
      href: projectHref(actor, firstItem.capability_id),
      metric: plural(waitingWorkflowItems.length, "workflow row"),
      source: "workflow projection",
      title: "Workflow waiting",
      tone: "attention",
    });
  }
  if (grantStatusRows.length > 0) {
    needsAttention.push({
      action: "Review",
      detail: "Grant rows with expiry, revoked, or non-active status are present for this Work Identity.",
      href: "#dashboard-requests",
      metric: plural(grantStatusRows.length, "grant row"),
      source: "grant ledger",
      title: "Grant status",
      tone: "attention",
    });
  }

  const continueWork: CockpitItem[] = [];
  if (activeWorkflowItems.length > 0) {
    const firstItem = activeWorkflowItems[0];
    continueWork.push({
      action: "Continue",
      detail: `${firstItem.title} is active in your assigned workflow.`,
      href: projectHref(actor, firstItem.capability_id),
      metric: plural(activeWorkflowItems.length, "active row"),
      source: "workflow projection",
      title: "Continue active workflow",
      tone: "steady",
    });
  }
  if (projects.length > 0) {
    const firstProject = projects[0];
    continueWork.push({
      action: "Open",
      detail: `${firstProject.capability_name} is assigned to this Work Identity.`,
      href: projectHref(actor, firstProject.capability_id),
      metric: plural(projects.length, "project"),
      source: "Work Identity projects",
      title: "Open project work",
      tone: "steady",
    });
  }

  const askWithContext: CockpitItem[] = [];
  if (activeGrants.length > 0) {
    const firstGrant = activeGrants[0];
    askWithContext.push({
      action: "Ask",
      detail: `${capabilityTitle(firstGrant.target.capability_id, projectById)} is available through an active read grant.`,
      href: askGrantHref(actor, firstGrant),
      metric: plural(activeGrants.length, "active grant"),
      source: "grant ledger",
      title: "Ask with granted knowledge",
      tone: "ask",
    });
  }

  const waitingOn: CockpitItem[] = [];
  if (pendingRequests.length > 0) {
    waitingOn.push({
      action: "Review",
      detail: "Submitted access requests are still pending approval.",
      href: "#dashboard-requests",
      metric: plural(pendingRequests.length, "request"),
      source: "request ledger",
      title: "Waiting on approval",
      tone: "waiting",
    });
  }

  const managerRows: CockpitItem[] = [];
  if (inbox.length > 0) {
    managerRows.push({
      action: "Approve",
      detail: "Only loaded request inbox rows become manager review actions.",
      href: "#dashboard-requests",
      metric: plural(inbox.length, "inbox row"),
      source: "approval inbox",
      title: "Team requests",
      tone: "manager",
    });
  }
  if (roleScope?.team_scope.has_team_scope) {
    managerRows.push({
      action: "Open",
      detail: "Team scope is derived from reporting-line facts. Team workflow rows are not modeled here.",
      href: "#dashboard-scope",
      metric: plural(roleScope.team_scope.direct_report_count, "direct report"),
      source: "reporting line",
      title: "Team context",
      tone: "manager",
    });
  }
  if (isDepartmentHead(roleScope?.derived_level) && departmentId) {
    managerRows.push({
      action: "Open",
      detail: "Department workflow summary is limited to projects already visible to this Work Identity.",
      href: "#dashboard-role-aware-workflow",
      metric: `${departmentId} / ${plural(departmentWorkflowItems.length, "workflow row")}`,
      source: "server scope",
      title: "Department workflow summary",
      tone: "manager",
    });
  }

  return {
    askWithContext,
    continueWork,
    managerRows,
    needsAttention,
    waitingOn,
  };
}

interface CommandPodModel {
  callToAction?: string;
  detail: string;
  href: string;
  kind:
    | "work"
    | "project"
    | "team"
    | "department"
    | "approval"
    | "request"
    | "grant"
    | "agent"
    | "ask"
    | "executive";
  metric: string;
  title: string;
}

function buildCommandPods({
  actor,
  grants,
  inbox,
  projects,
  requests,
  roleScope,
  summary,
  workflowItems,
}: {
  actor: string;
  grants: AccessGrantRecord[];
  inbox: AccessRequestRecord[];
  projects: ProjectRecord[];
  requests: AccessRequestRecord[];
  roleScope: RoleScopeSummary | null;
  summary: NodeSummary | null;
  workflowItems: WorkflowItem[];
}): CommandPodModel[] {
  const projectCount = roleScope?.project_scope.project_count ?? projects.length;
  const firstCapabilityId = roleScope?.project_scope.capability_ids[0] ?? projects[0]?.capability_id;
  const department = roleScope?.department_scope.department_id ?? null;
  const pendingApprovals = inbox.length;
  const agentCount = summary?.agents_owned?.length ?? 0;
  const activeGrants = activeKnowledgeGrants(grants, actor);
  const pods: CommandPodModel[] = [
    {
      detail: "Projected from assigned project workflow items.",
      href: "#dashboard-workflow",
      kind: "work",
      metric: `${workflowItems.length} ${workflowItems.length === 1 ? "item" : "items"}`,
      title: "My Work",
    },
  ];

  if (firstCapabilityId && projectCount > 0) {
    pods.push({
      detail: "Open the project surface with graph and workflow views.",
      href: `/project?cap=${encodeURIComponent(firstCapabilityId)}&as=${encodeURIComponent(actor)}`,
      kind: "project",
      metric: `${projectCount} ${projectCount === 1 ? "project" : "projects"}`,
      title: "Project Context",
    });
  }

  if (activeGrants.length > 0) {
    pods.push({
      detail: "Open only the capabilities unlocked by active read grants.",
      href: "#dashboard-granted-knowledge",
      kind: "grant",
      metric: `${activeGrants.length} active ${activeGrants.length === 1 ? "grant" : "grants"}`,
      title: "Granted Knowledge",
    });
  }

  pods.push({
    callToAction: "Start Conversation",
    detail: "Use this Work Identity and its permission scope in Ask.",
    href: `/ask?as=${encodeURIComponent(actor)}`,
    kind: "ask",
    metric: "permission scoped",
    title: "Ask a Question",
  });

  if (roleScope?.team_scope.has_team_scope) {
    pods.push({
      detail: "Derived from real reporting-line facts.",
      href: "#dashboard-scope",
      kind: "team",
      metric: `${roleScope.team_scope.direct_report_count} direct ${
        roleScope.team_scope.direct_report_count === 1 ? "report" : "reports"
      }`,
      title: "Team Context",
    });
  }

  if (isDepartmentHead(roleScope?.derived_level) && department) {
    pods.push({
      detail: "Department-head signal from real reporting and title facts.",
      href: "#dashboard-scope",
      kind: "department",
      metric: department,
      title: "Department Context",
    });
  }

  if (pendingApprovals > 0) {
    pods.push({
      detail: "Requests assigned to this Work Identity for review.",
      href: "#dashboard-requests",
      kind: "approval",
      metric: `${pendingApprovals} pending`,
      title: "Approval Queue",
    });
  }

  if (isExecutiveCandidate(roleScope?.derived_level)) {
    pods.push({
      detail: "Candidate signal only. No restricted surface is unlocked.",
      href: "#dashboard-role-experience",
      kind: "executive",
      metric: "candidate only",
      title: "Executive Candidate",
    });
  }

  pods.push({
    detail: "Track submitted requests, assigned reviews, and active read grants.",
    href: "#dashboard-requests",
    kind: "request",
    metric: `${requests.length} mine / ${inbox.length} inbox / ${grants.length} grants`,
    title: "Access Requests",
  });

  if (agentCount > 0) {
    pods.push({
      detail: "Launch the existing ask route with visible agent context.",
      href: `/ask?as=${encodeURIComponent(actor)}`,
      kind: "agent",
      metric: `${agentCount} ${agentCount === 1 ? "agent" : "agents"}`,
      title: "Agent Assist",
    });
  }

  return pods;
}

function CommandPods({ pods }: { pods: CommandPodModel[] }) {
  return (
    <section className="mb-4 grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-4" data-testid="dashboard-command-pods">
      {pods.map((pod, index) => (
        <CommandPod key={`${pod.kind}:${pod.title}`} delayIndex={index} pod={pod} />
      ))}
    </section>
  );
}

function CommandPod({ delayIndex, pod }: { delayIndex: number; pod: CommandPodModel }) {
  const isAsk = pod.kind === "ask";
  const content = (
    <>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            {pod.metric}
          </p>
          <h2
            className="ap-register-chrome mt-1"
            style={{ fontSize: isAsk ? TYPE.scale.lg : TYPE.scale.sm, fontWeight: 600 }}
          >
            {pod.title}
          </h2>
        </div>
        <span
          aria-hidden="true"
          className="ap-hairline grid shrink-0 place-items-center rounded-full border"
          style={{ height: 30, width: 30 }}
        >
          <span className="ap-register-evidence" style={{ fontSize: TYPE.scale.xs }}>
            {pod.kind.slice(0, 1).toUpperCase()}
          </span>
        </span>
      </div>
      <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
        {pod.detail}
      </p>
      {pod.callToAction && (
        <span
          className="ap-affordance-button ap-register-chrome mt-4 inline-flex rounded-lg px-3 py-2"
          style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
        >
          {pod.callToAction}
        </span>
      )}
    </>
  );

  return (
    <MotionAnchor
      href={pod.href}
      className="ap-card ap-washable block rounded-lg p-3"
      delayIndex={delayIndex}
      data-pod-kind={pod.kind}
      data-testid={isAsk ? "dashboard-ask-pod" : `dashboard-pod-${pod.kind}`}
      style={{
        ...dashboardPanelStyle(),
        minHeight: isAsk ? 178 : 136,
      }}
    >
      {content}
    </MotionAnchor>
  );
}

function DashboardPanelTabs({
  active,
  onSelect,
}: {
  active: DashboardPanelMode | null;
  onSelect: (mode: DashboardPanelMode) => void;
}) {
  // Profile opens from the identity strip (avatar + name); only Workspace and
  // Settings remain as header pill triggers.
  const tabs: { label: string; mode: DashboardPanelMode }[] = [
    { label: "Workspace", mode: "workspace" },
    { label: "Settings", mode: "settings" },
  ];

  return (
    <div
      className="ap-card flex flex-wrap gap-1 rounded-full p-1"
      data-testid="dashboard-panel-tabs"
      aria-label="Open cockpit panels"
    >
      {tabs.map((tab) => {
        const selected = active === tab.mode;
        return (
          <button
            key={tab.mode}
            type="button"
            className={`${selected ? "ap-affordance-button" : "ap-washable"} ap-register-chrome inline-flex min-h-10 items-center gap-1.5 rounded-full px-3 py-2`}
            style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
            onClick={() => onSelect(tab.mode)}
            aria-haspopup="dialog"
            aria-expanded={selected}
            data-active={selected ? "true" : "false"}
            data-testid={`dashboard-${tab.mode}-panel-trigger`}
          >
            {/* Side-panel glyph: marks this as a control that OPENS a panel,
                not a route tab that swaps content in place (no-affordance contract). */}
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
              <rect x="2.5" y="3" width="11" height="10" rx="1.5" />
              <line x1="10" y1="3" x2="10" y2="13" />
            </svg>
            {tab.label}
          </button>
        );
      })}
    </div>
  );
}

function AskAgentCard({ actor, grants }: { actor: string; grants: AccessGrantRecord[] }) {
  const activeGrants = activeKnowledgeGrants(grants, actor);
  const grant = activeGrants[0];
  const href = grant
    ? `/ask?as=${encodeURIComponent(actor)}&grant=${encodeURIComponent(grant.grant_id)}&cap=${encodeURIComponent(grant.target.capability_id)}`
    : `/ask?as=${encodeURIComponent(actor)}`;

  return (
    <MotionAnchor
      href={href}
      className="ap-card ap-washable block h-full rounded-2xl p-4"
      data-testid="dashboard-ask-agent-card"
    >
      <div className="flex h-full flex-col justify-between gap-4">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Ask AI agent
          </p>
          <h2 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
            Ask with your current access
          </h2>
          <p className="ap-soft mt-1 max-w-2xl" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            Get an answer scoped to what this identity may see.
          </p>
        </div>
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div className="flex flex-wrap gap-1.5">
            <Chip>{activeGrants.length > 0 ? `${activeGrants.length} read grants` : "identity scope"}</Chip>
            <Chip mono>{actor}</Chip>
          </div>
          <span
            className="ap-affordance-button ap-register-chrome rounded-full px-4 py-2"
            style={{ fontSize: TYPE.scale.sm, fontWeight: 700 }}
          >
            Ask
          </span>
        </div>
      </div>
    </MotionAnchor>
  );
}

function TodayCockpit({ model }: { model: TodayCockpitModel }) {
  const attentionCount = model.needsAttention.length;
  return (
    <MotionSection
      className="ap-card rounded-2xl p-3"
      data-testid="dashboard-today-cockpit"
    >
      <div className="mb-2 flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Today
          </p>
          <h2 className="ap-register-chrome" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
            What needs attention?
          </h2>
        </div>
        <Chip>{plural(attentionCount, "attention row")}</Chip>
      </div>

      <div className="grid grid-cols-1 gap-2 md:grid-cols-3">
        <CockpitSection
          emptyLabel="Nothing waiting."
          items={model.needsAttention}
          testId="dashboard-today-needs-attention"
          title="Needs Attention"
        />
        <CockpitSection
          emptyLabel="No active workflow rows."
          items={model.continueWork}
          testId="dashboard-today-continue-work"
          title="Continue Work"
        />
        <CockpitSection
          emptyLabel="No requests waiting."
          items={model.waitingOn}
          testId="dashboard-today-waiting-on"
          title="Waiting On"
        />
      </div>

      {model.managerRows.length > 0 && (
        <section className="mt-2" data-testid="dashboard-today-manager-context">
          <div className="mb-2 flex items-center justify-between gap-3">
            <h3 className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
              Manager Context
            </h3>
            <span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
              real scope only
            </span>
          </div>
          <div className="grid grid-cols-1 gap-2 lg:grid-cols-3">
            {model.managerRows.map((item, index) => (
              <CockpitRow item={item} key={`${item.title}:${item.metric}`} delayIndex={index} />
            ))}
          </div>
        </section>
      )}
    </MotionSection>
  );
}

function CockpitSection({
  emptyLabel,
  items,
  testId,
  title,
}: {
  emptyLabel: string;
  items: CockpitItem[];
  testId: string;
  title: string;
}) {
  const visibleItems = items.slice(0, 2);
  return (
    <section className="ap-card rounded-2xl border p-2" data-testid={testId}>
      <div className="flex items-center justify-between gap-3">
        <h3 className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
          {title}
        </h3>
        <span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
          {items.length}
        </span>
      </div>
      {items.length === 0 ? (
        <EmptyLine>{emptyLabel}</EmptyLine>
      ) : (
        <div className="mt-2 space-y-2">
          {visibleItems.map((item, index) => (
            <CockpitRow compact item={item} key={`${item.title}:${item.metric}`} delayIndex={index} />
          ))}
          {items.length > visibleItems.length && (
            <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
              {items.length - visibleItems.length} more in Workspace
            </p>
          )}
        </div>
      )}
    </section>
  );
}

function CockpitRow({
  compact = false,
  delayIndex,
  item,
}: {
  compact?: boolean;
  delayIndex: number;
  item: CockpitItem;
}) {
  return (
    <MotionAnchor
      href={item.href}
      className="ap-card ap-washable block rounded-2xl border p-2"
      delayIndex={delayIndex}
      data-cockpit-action={item.action.toLowerCase()}
      data-cockpit-tone={item.tone}
      data-testid="dashboard-today-row"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            {item.metric}
          </p>
          <h4 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            {item.title}
          </h4>
        </div>
        <span
          className="ap-affordance-button ap-register-chrome rounded-lg px-2 py-1"
          style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
        >
          {item.action}
        </span>
      </div>
      {!compact && (
        <>
          <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            {item.detail}
          </p>
          <p className="ap-register-evidence ap-soft mt-2" style={{ fontSize: TYPE.scale.xs }}>
            {item.source}
          </p>
        </>
      )}
    </MotionAnchor>
  );
}

interface RoleExperienceCard {
  detail: string;
  label: string;
  metric: string;
  source: string;
  tone: "default" | "active" | "candidate" | "limited";
}

function buildRoleExperienceCards({
  actor,
  grants,
  inbox,
  requests,
  roleScope,
  workflowItems,
}: {
  actor: string;
  grants: AccessGrantRecord[];
  inbox: AccessRequestRecord[];
  requests: AccessRequestRecord[];
  roleScope: RoleScopeSummary | null;
  workflowItems: WorkflowItem[];
}): RoleExperienceCard[] {
  const cards: RoleExperienceCard[] = [
    {
      detail: "Daily execution stays scoped to this Work Identity.",
      label: "Employee baseline",
      metric: `${workflowItems.length} workflow ${workflowItems.length === 1 ? "item" : "items"}`,
      source: "workflow projection",
      tone: "default",
    },
  ];
  const activeGrants = activeKnowledgeGrants(grants, actor);
  if (activeGrants.length > 0) {
    cards.push({
      detail: "Active read grants can open approved capability context in Ask.",
      label: "Granted knowledge",
      metric: `${activeGrants.length} active`,
      source: "grant ledger",
      tone: "active",
    });
  }
  if (roleScope?.team_scope.has_team_scope) {
    cards.push({
      detail: "Direct-report count is derived from the people record.",
      label: "Team scope",
      metric: `${roleScope.team_scope.direct_report_count} direct ${
        roleScope.team_scope.direct_report_count === 1 ? "report" : "reports"
      }`,
      source: "reporting line",
      tone: "active",
    });
  }
  if (isDepartmentHead(roleScope?.derived_level)) {
    cards.push({
      detail: "Department context is previewed from visible Work Identity scope.",
      label: "Department head",
      metric: roleScope?.department_scope.department_id ?? "department",
      source: "server scope",
      tone: "active",
    });
  }
  if (inbox.length > 0) {
    cards.push({
      detail: "Only requests assigned to this Work Identity appear here.",
      label: "Approval queue",
      metric: `${inbox.length} pending`,
      source: "request inbox",
      tone: "active",
    });
  } else if (roleScope?.approval_scope.has_approval_scope) {
    cards.push({
      detail: "Approval posture exists, but no request inbox rows are visible.",
      label: "Approval scope",
      metric: "no rows",
      source: "server scope",
      tone: "limited",
    });
  }
  if (isExecutiveCandidate(roleScope?.derived_level)) {
    cards.push({
      detail: "Candidate signal is shown honestly and unlocks nothing.",
      label: "Executive candidate",
      metric: "label only",
      source: "server scope",
      tone: "candidate",
    });
  }
  cards.push({
    detail: "Restricted surfaces remain unavailable in this dashboard.",
    label: "Surface boundary",
    metric: scopeModeLabel(roleScope),
    source: "scope contract",
    tone: "limited",
  });
  if (requests.length > 0) {
    cards.push({
      detail: "Submitted requests stay visible as status, not access grants.",
      label: "Request status",
      metric: `${requests.length} submitted`,
      source: "request ledger",
      tone: "default",
    });
  }
  return cards;
}

function RoleExperienceSummary({ cards }: { cards: RoleExperienceCard[] }) {
  return (
    <div
      className="grid grid-cols-1 gap-2"
      data-testid="dashboard-role-experience"
      id="dashboard-role-experience"
    >
      {cards.map((card, index) => (
        <MotionArticle
          key={`${card.label}:${card.metric}`}
          className="ap-card rounded-lg border p-3"
          delayIndex={index}
          data-role-tone={card.tone}
          data-testid={`dashboard-role-card-${card.tone}`}
        >
          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0">
              <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
                {card.label}
              </p>
              <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
                {card.detail}
              </p>
            </div>
            <Chip>{card.metric}</Chip>
          </div>
          <p className="ap-register-evidence ap-soft mt-2" style={{ fontSize: TYPE.scale.xs }}>
            {card.source}
          </p>
        </MotionArticle>
      ))}
    </div>
  );
}

function NotificationCenter({ items }: { items: NotificationItem[] }) {
  return (
    <details
      className="group relative"
      data-testid="dashboard-notification-center"
      onMouseEnter={(event) => {
        event.currentTarget.open = true;
      }}
      onMouseLeave={(event) => {
        event.currentTarget.open = false;
      }}
    >
      <summary
        className="ap-card ap-washable flex min-h-10 cursor-pointer list-none items-center gap-2 rounded-lg px-3 py-2"
        data-testid="dashboard-notification-trigger"
        style={dashboardPanelStyle()}
      >
        <span className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
          Notifications
        </span>
        <span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
          {items.length > 0 ? "real data" : "no active rows"}
        </span>
      </summary>
      <div
        className="ap-card absolute right-0 z-20 mt-2 w-[min(380px,calc(100vw-2rem))] rounded-lg border p-3 shadow-lg"
        data-testid="dashboard-notification-dropdown"
        style={dashboardPanelStyle()}
      >
        <div className="mb-2 flex items-baseline justify-between gap-3">
          <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            Notification Center
          </p>
          <Chip>{items.length > 0 ? "derived" : "empty"}</Chip>
        </div>
        {items.length === 0 ? (
          <EmptyLine>No request, approval, workflow, grant, or team notification rows are visible.</EmptyLine>
        ) : (
          <div className="space-y-2">
            {items.map((item) => (
              <a
                key={`${item.category}:${item.title}`}
                href={item.href}
                className="ap-washable block rounded-lg border px-3 py-2"
                data-testid="dashboard-notification-item"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
                      {item.category}
                    </p>
                    <p className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
                      {item.title}
                    </p>
                  </div>
                  {item.metric && <Chip mono>{item.metric}</Chip>}
                </div>
                <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
                  {item.detail}
                </p>
                <p className="ap-register-evidence ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
                  {item.source}
                </p>
              </a>
            ))}
          </div>
        )}
      </div>
    </details>
  );
}

function WorkflowCommandSubbar({ items }: { items: NotificationItem[] }) {
  return (
    <MotionSection
      className="ap-card mb-4 rounded-lg border p-2"
      data-testid="dashboard-workflow-command"
    >
      <details className="group">
        <summary
          className="ap-washable flex min-h-10 cursor-pointer list-none flex-wrap items-center justify-between gap-3 rounded-lg px-3 py-2"
          data-testid="dashboard-workflow-command-trigger"
        >
          <div className="min-w-0">
            <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
              Workflow Command
            </p>
            <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
              Requests, approvals, workflow alerts, grants, and role-scope updates.
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-1.5">
            {items.length === 0 ? (
              <Chip>empty</Chip>
            ) : (
              items.slice(0, 5).map((item) => (
                <span
                  key={`${item.category}:${item.title}:chip`}
                  className="ap-chip ap-register-chrome rounded-lg px-2 py-1"
                  style={{ fontSize: TYPE.scale.xs }}
                  data-testid="dashboard-workflow-command-category"
                >
                  {item.category}
                  {item.metric ? ` ${item.metric}` : ""}
                </span>
              ))
            )}
          </div>
        </summary>
        <div
          className="mt-2 grid grid-cols-1 gap-2 md:grid-cols-2 xl:grid-cols-4"
          data-testid="dashboard-workflow-command-menu"
        >
          {items.length === 0 ? (
            <MotionArticle className="ap-card rounded-lg border p-3">
              <EmptyLine compact>No command categories are backed by current request, workflow, or grant rows.</EmptyLine>
            </MotionArticle>
          ) : (
            items.map((item, index) => (
              <MotionAnchor
                key={`${item.category}:${item.title}:command`}
                href={item.href}
                className="ap-card ap-washable block rounded-lg border p-3"
                delayIndex={index}
                data-testid="dashboard-workflow-command-item"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
                      {item.category}
                    </p>
                    <p className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
                      {item.title}
                    </p>
                  </div>
                  {item.metric && <Chip mono>{item.metric}</Chip>}
                </div>
                <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
                  {item.detail}
                </p>
                <p className="ap-register-evidence ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
                  {item.source}
                </p>
              </MotionAnchor>
            ))
          )}
        </div>
      </details>
    </MotionSection>
  );
}

function WorkspaceNotifications({ items }: { items: NotificationItem[] }) {
  return (
    <section className="ap-card rounded-lg border p-2.5" data-testid="dashboard-notification-center">
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <div>
          <h3 className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            Action summary
          </h3>
          <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
            Requests, approvals, grants, and workflow attention from real rows.
          </p>
        </div>
        <Chip>{items.length > 0 ? `${items.length} categories` : "empty"}</Chip>
      </div>
      {items.length === 0 ? (
        <EmptyLine>No request, approval, workflow, grant, or team rows are visible.</EmptyLine>
      ) : (
        <div className="grid grid-cols-1 gap-1.5">
          {items.map((item, index) => (
            <MotionAnchor
              key={`${item.category}:${item.title}`}
              href={item.href}
              className="ap-card ap-washable block rounded-lg border px-2 py-1.5"
              delayIndex={index}
              data-testid="dashboard-notification-item"
            >
              <div className="flex items-center justify-between gap-2">
                <div className="min-w-0">
                  <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
                    {item.category}
                  </p>
                  <p className="ap-register-chrome mt-0.5 truncate" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
                    {item.title}
                  </p>
                </div>
                {item.metric && <Chip mono>{item.metric}</Chip>}
              </div>
            </MotionAnchor>
          ))}
        </div>
      )}
    </section>
  );
}

function WorkspacePanel({
  actor,
  grantError,
  grants,
  inbox,
  notificationItems,
  onRevokeGrant,
  projectById,
  projects,
  requests,
  revokingGrantId,
  roleScope,
  workflowItems,
}: {
  actor: string;
  grantError: string | null;
  grants: AccessGrantRecord[];
  inbox: AccessRequestRecord[];
  notificationItems: NotificationItem[];
  onRevokeGrant: (grantId: string) => void;
  projectById: Map<string, GraphProject>;
  projects: ProjectRecord[];
  requests: AccessRequestRecord[];
  revokingGrantId: string | null;
  roleScope: RoleScopeSummary | null;
  workflowItems: WorkflowItem[];
}) {
  const waitingWorkflowItems = workflowItems.filter((item) =>
    ["pending", "blocked", "denied", "cancelled", "expired", "dismissed"].includes(item.status.toLowerCase()),
  );

  return (
    <MotionSection
      className="ap-card rounded-2xl p-3"
      data-testid="dashboard-workspace"
      id="dashboard-workspace"
    >
      <div className="mb-3 flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Workspace
          </p>
          <h2 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
            Work needing a decision
          </h2>
        </div>
        <Chip>{scopeModeLabel(roleScope)}</Chip>
      </div>

      <div className="space-y-3">
        <WorkspaceNotifications items={notificationItems} />
        <WorkflowCommandSubbar items={notificationItems} />

        <WorkspaceBlock title="Requests and approvals" defaultOpen>
          <RequestsList
            actor={actor}
            grantError={grantError}
            grants={grants}
            inbox={inbox}
            onRevokeGrant={onRevokeGrant}
            projectById={projectById}
            requests={requests}
            revokingGrantId={revokingGrantId}
          />
        </WorkspaceBlock>

        <WorkspaceBlock title="Granted Knowledge" defaultOpen>
          <GrantedKnowledgeList actor={actor} grants={grants} projectById={projectById} />
        </WorkspaceBlock>

        <WorkspaceBlock title="Workflow alerts">
          {waitingWorkflowItems.length === 0 ? (
            <EmptyLine compact>No waiting or blocked workflow rows are visible.</EmptyLine>
          ) : (
            <div className="space-y-2">
              {waitingWorkflowItems.slice(0, 5).map((item, index) => (
                <MotionAnchor
                  key={item.item_id}
                  href={`/project?cap=${encodeURIComponent(item.capability_id)}&as=${encodeURIComponent(actor)}`}
                  className="ap-card ap-washable block rounded-lg p-2"
                  delayIndex={index}
                  data-testid="dashboard-workspace-workflow-alert"
                >
                  <div className="flex items-start justify-between gap-2">
                    <p className="ap-register-chrome min-w-0" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
                      {item.title}
                    </p>
                    <Chip>{item.status}</Chip>
                  </div>
                  <p className="ap-register-evidence ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
                    {item.capability_id}
                  </p>
                </MotionAnchor>
              ))}
            </div>
          )}
        </WorkspaceBlock>

        {(roleScope?.team_scope.has_team_scope || isDepartmentHead(roleScope?.derived_level)) && (
          <WorkspaceBlock title="Team and department context">
            {roleScope?.team_scope.has_team_scope && (
              <WorkspaceFact
                label="Team requests"
                source="reporting line"
                value={`${roleScope.team_scope.direct_report_count} direct ${roleScope.team_scope.direct_report_count === 1 ? "report" : "reports"}`}
              />
            )}
            {isDepartmentHead(roleScope?.derived_level) && roleScope?.department_scope.department_id && (
              <WorkspaceFact
                label="Department context"
                source="server scope"
                value={roleScope.department_scope.department_id}
              />
            )}
            <EmptyLine compact>Team workflow rows appear only when the API exposes them.</EmptyLine>
          </WorkspaceBlock>
        )}

        <WorkspaceBlock title="Visible workflow layers">
          <RoleAwareWorkflowLayer
            actor={actor}
            inbox={inbox}
            projectById={projectById}
            projects={projects}
            requests={requests}
            roleScope={roleScope}
            workflowItems={workflowItems}
          />
        </WorkspaceBlock>
      </div>
    </MotionSection>
  );
}

function ProfilePanel({
  actor,
  grants,
  human,
  inbox,
  lens,
  projectById,
  requests,
  roleExperienceCards,
  roleScope,
  scopeBadges,
  summary,
}: {
  actor: string;
  grants: AccessGrantRecord[];
  human: LensResponse["subject_human"];
  inbox: AccessRequestRecord[];
  lens: LensResponse;
  projectById: Map<string, GraphProject>;
  requests: AccessRequestRecord[];
  roleExperienceCards: RoleExperienceCard[];
  roleScope: RoleScopeSummary | null;
  scopeBadges: ScopeBadge[];
  summary: NodeSummary | null;
}) {
  const directReports = roleScope?.team_scope.direct_report_count ?? human?.manages.length ?? 0;
  const knowledgeSections = lens.holdings.length;
  const visibleKnowledgeRows = lens.holdings.reduce((sum, section) => sum + section.docs.length, 0);
  const auditRows = [
    ...requests.map((request) => ({
      id: request.request_id,
      label: "Request",
      status: request.status,
      target: request.target.capability_id,
    })),
    ...inbox.map((request) => ({
      id: request.request_id,
      label: "Approval",
      status: request.status,
      target: request.target.capability_id,
    })),
    ...grants.map((grant) => ({
      id: grant.grant_id,
      label: "Grant",
      status: grant.status,
      target: grant.target.capability_id,
    })),
  ];

  return (
    <section
      className="ap-card rounded-2xl p-3"
      data-testid="dashboard-profile-panel"
      id="dashboard-profile"
    >
      <div className="mb-3 flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Profile
          </p>
          <h2 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
            Identity, access, and knowledge
          </h2>
        </div>
        <Chip>{scopeModeLabel(roleScope)}</Chip>
      </div>

      {/* A4: the page's single demo-status line is the shell's notice — the
          dashboard no longer stacks its own copy. */}
      <div className="grid grid-cols-1 gap-3">
        <div className="grid grid-cols-1 gap-3">
          <WorkspaceBlock title="Identity" defaultOpen>
            <WorkspaceFact label="Identity ID" source="selected Work Identity" value={actor} />
            <WorkspaceFact label="Name" source="Work Identity" value={human?.display_name ?? lens.subject.name} />
            <WorkspaceFact label="Title" source="people record" value={human?.title ?? "Role unavailable"} />
          </WorkspaceBlock>

          <WorkspaceBlock title="Role" defaultOpen>
            <WorkspaceFact
              label="Role posture"
              source="server scope"
              value={roleScope ? roleLabel(roleScope.derived_level) : "Role posture unavailable"}
            />
            <WorkspaceFact
              label="Confidence"
              source="server scope"
              value={roleScope?.confidence ?? "unavailable"}
            />
            <WorkspaceFact
              label="Surface access"
              source="scope contract"
              value={roleScope?.admin_surface_allowed ? "restricted preview candidate" : "daily work only"}
            />
          </WorkspaceBlock>

          <WorkspaceBlock title="Department">
            <WorkspaceFact
              label="Department"
              source="Work Identity"
              value={human?.department_label ?? lens.subject.department ?? "Department unavailable"}
            />
            <WorkspaceFact label="Manager" source="people record" value={human?.reports_to ?? "Manager unavailable"} />
            <WorkspaceFact
              label="Team"
              source="reporting line"
              value={directReports > 0 ? `${directReports} direct ${directReports === 1 ? "report" : "reports"}` : "No team scope"}
            />
          </WorkspaceBlock>

          <WorkspaceBlock title="Security">
            <WorkspaceFact label="Band" source="scope" value={String(lens.subject.band ?? "unavailable")} />
            <WorkspaceFact label="Groups" source="scope" value={lens.subject.groups.join(", ") || "No groups visible"} />
            <WorkspaceFact label="Sites" source="scope" value={lens.subject.sites.join(", ") || "No sites visible"} />
            <WorkspaceFact label="Restricted surfaces" source="scope contract" value="Unavailable on this dashboard" />
          </WorkspaceBlock>
        </div>

        <div className="space-y-3">
          <WorkspaceBlock title="Audit Activity">
            <div className="space-y-2" data-testid="dashboard-audit-activity">
              {auditRows.length === 0 ? (
                <EmptyLine compact>No request, approval, or grant ledger rows are visible.</EmptyLine>
              ) : (
                auditRows.slice(0, 5).map((row) => {
                  const project = projectById.get(row.target);
                  return (
                    <div key={`${row.label}:${row.id}`} className="ap-card rounded-lg p-2">
                      <div className="flex items-start justify-between gap-2">
                        <div className="min-w-0">
                          <p className="ap-register-chrome truncate" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
                            {row.label} {row.id}
                          </p>
                          <p className="ap-soft mt-1 truncate" style={{ fontSize: TYPE.scale.xs }}>
                            {project?.label.replace(/^Capability:\s*/i, "") ?? row.target}
                          </p>
                        </div>
                        <Chip>{row.status}</Chip>
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </WorkspaceBlock>

          <WorkspaceBlock title="Role Experience">
            <RoleExperienceSummary cards={roleExperienceCards} />
          </WorkspaceBlock>

          <WorkspaceBlock title="Scope Posture">
            <ScopePosture badges={scopeBadges} />
          </WorkspaceBlock>

          <WorkspaceBlock title="My Agents">
            <AgentsList agents={summary?.agents_owned ?? []} />
          </WorkspaceBlock>

          <WorkspaceBlock title="My Knowledge">
            <KnowledgeSummary
              sections={knowledgeSections}
              rows={visibleKnowledgeRows}
              holdings={lens.holdings.map((section) => ({
                count: section.docs.length,
                sentence: section.sentence,
              }))}
            />
          </WorkspaceBlock>
        </div>
      </div>
    </section>
  );
}

function SettingsPanel({
  graph,
  roleScope,
  summary,
}: {
  graph: GraphResponse | null;
  roleScope: RoleScopeSummary | null;
  summary: NodeSummary | null;
}) {
  const systems = deriveConnectedSystems(graph);
  return (
    <MotionSection
      className="ap-card rounded-2xl p-3"
      data-testid="dashboard-settings-panel"
      id="dashboard-settings"
    >
      <div className="mb-3 flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Settings
          </p>
          <h2 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
            Display, systems, and preferences
          </h2>
        </div>
        <Chip>{scopeModeLabel(roleScope)}</Chip>
      </div>

      <div className="grid grid-cols-1 gap-3">
        <WorkspaceBlock title="Theme" defaultOpen>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="min-w-0">
              <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
                Light or dark mode
              </p>
              <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
                Choose the display mode for this browser. Dark remains the default for the demo.
              </p>
            </div>
            <ThemeToggle compact />
          </div>
        </WorkspaceBlock>

        <WorkspaceBlock title="Connected Systems">
          <div className="grid grid-cols-1 gap-2" data-testid="dashboard-connected-systems">
            {systems.length === 0 ? (
              <EmptyLine compact>No supported connected systems are visible through this graph.</EmptyLine>
            ) : (
              systems.map((system) => (
                <div
                  key={`${system.name}:${system.source}`}
                  className="ap-card flex items-center justify-between gap-3 rounded-lg p-2"
                  data-testid="dashboard-connected-system"
                >
                  <div className="min-w-0">
                    <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
                      {system.name}
                    </p>
                    <p className="ap-register-evidence ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
                      {system.source}
                    </p>
                  </div>
                  <Chip>{system.status}</Chip>
                </div>
              ))
            )}
          </div>
        </WorkspaceBlock>

        <WorkspaceBlock title="Preferences">
          <WorkspaceFact label="Workspace preferences" source="not modeled" value="Unavailable" />
        </WorkspaceBlock>

        <WorkspaceBlock title="Agent Preferences">
          <WorkspaceFact
            label="Owned agents"
            source="node summary"
            value={`${summary?.agents_owned?.length ?? 0} owned agents visible`}
          />
          <WorkspaceFact
            label="Agent behavior"
            source="not connected"
            value="Not in this build."
          />
        </WorkspaceBlock>
      </div>
    </MotionSection>
  );
}

function DashboardPanelDrawer({
  children,
  mode,
  onClose,
}: {
  children: React.ReactNode;
  mode: DashboardPanelMode | null;
  onClose: () => void;
}) {
  const shouldReduce = useReducedMotion() ?? false;
  const title = mode === "workspace" ? "Workspace" : mode === "profile" ? "Profile" : "Settings";
  // B6: the drawer's focus management (focus-in on open, Tab trap, Escape
  // close, focus-restore on close) now comes from the SHARED primitive this
  // pattern was extracted into — one implementation for every drawer.
  const { dialogRef: asideRef, onKeyDown } = useModalDialogFocus({
    open: mode !== null,
    onClose,
  });

  return (
    <AnimatePresence>
      {mode !== null && (
        <>
          <motion.button
            type="button"
            className="ap-glass-scrim fixed inset-0 z-40 cursor-default"
            aria-label={`Close ${title}`}
            tabIndex={-1}
            data-testid="dashboard-drawer-scrim"
            onClick={onClose}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: shouldReduce ? 0 : 0.18 }}
          />
          <motion.aside
            ref={asideRef}
            role="dialog"
            aria-modal="true"
            aria-label={`${title} panel`}
            tabIndex={-1}
            onKeyDown={onKeyDown}
            className="ap-glass-popover fixed bottom-3 right-3 top-3 z-50 w-[min(456px,calc(100vw-24px))] overflow-y-auto rounded-2xl p-3"
            data-testid="dashboard-active-drawer"
            initial={{ opacity: 0, x: shouldReduce ? 0 : 42 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: shouldReduce ? 0 : 28 }}
            transition={{ duration: shouldReduce ? 0 : 0.18, ease: [0.16, 1, 0.3, 1] }}
          >
            <div className="mb-3 flex items-center justify-between gap-3">
              <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
                {title}
              </p>
              <button
                type="button"
                className="ap-washable ap-register-chrome min-h-10 rounded-full border px-3 py-2"
                style={{ borderColor: "var(--hairline)", fontSize: TYPE.scale.xs, fontWeight: 700 }}
                onClick={onClose}
                data-testid="dashboard-drawer-close"
              >
                Close
              </button>
            </div>
            {children}
          </motion.aside>
        </>
      )}
    </AnimatePresence>
  );
}

function WorkspaceBlock({
  children,
  defaultOpen = false,
  title,
}: {
  children: React.ReactNode;
  defaultOpen?: boolean;
  title: string;
}) {
  return (
    <details className="ap-card rounded-lg border p-2.5" open={defaultOpen ? true : undefined}>
      <summary className="ap-washable flex cursor-pointer list-none items-center justify-between gap-3 rounded-lg px-1 py-0.5">
        <h3 className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
          {title}
        </h3>
        <span aria-hidden="true" className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
          Details
        </span>
      </summary>
      <div className="mt-2 space-y-2">{children}</div>
    </details>
  );
}

function WorkspaceFact({ label, source, value }: { label: string; source: string; value: string }) {
  return (
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0">
        <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
          {label}
        </p>
        <p className="ap-soft mt-1 break-words" style={{ fontSize: TYPE.scale.xs }}>
          {value}
        </p>
      </div>
      <span className="ap-register-evidence ap-soft shrink-0" style={{ fontSize: TYPE.scale.xs }}>
        {source}
      </span>
    </div>
  );
}

function RoleAwareWorkflowLayer({
  actor,
  inbox,
  projectById,
  projects,
  requests,
  roleScope,
  workflowItems,
}: {
  actor: string;
  inbox: AccessRequestRecord[];
  projectById: Map<string, GraphProject>;
  projects: ProjectRecord[];
  requests: AccessRequestRecord[];
  roleScope: RoleScopeSummary | null;
  workflowItems: WorkflowItem[];
}) {
  const departmentId = roleScope?.department_scope.department_id ?? null;
  const departmentCapabilityIds = new Set(
    projects.flatMap((project) => {
      const graphProject = projectById.get(project.capability_id);
      if (!departmentId || !graphProject) return [];
      return graphProject.departments.includes(departmentId) || graphProject.primary_department_id === departmentId
        ? [project.capability_id]
        : [];
    }),
  );
  const departmentWorkflowItems = workflowItems.filter((item) => departmentCapabilityIds.has(item.capability_id));
  const hasTeamLayer = Boolean(roleScope?.team_scope.has_team_scope);
  const hasDepartmentLayer = isDepartmentHead(roleScope?.derived_level) && departmentId !== null;
  const hasExecutiveSignal = isExecutiveCandidate(roleScope?.derived_level);

  return (
    <section
      className="ap-card ap-elevated mb-4 rounded-lg border p-4"
      data-testid="dashboard-role-aware-workflow"
      id="dashboard-role-aware-workflow"
    >
      <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Workflow context
          </p>
          <h2 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.lg, fontWeight: 600 }}>
            My work plus visible leadership context
          </h2>
        </div>
        <Chip>{scopeModeLabel(roleScope)}</Chip>
      </div>

      <div className="grid grid-cols-1 gap-3 xl:grid-cols-4">
        <WorkflowLayerCard
          detail="Personal execution remains visible for every Work Identity."
          metric={`${workflowItems.length} workflow ${workflowItems.length === 1 ? "item" : "items"}`}
          testId="dashboard-employee-workflow-layer"
          title="Employee Layer"
        >
          <div className="mt-3 flex flex-wrap gap-1.5">
            <Chip>{projects.length} projects</Chip>
            <Chip>{requests.length} requests</Chip>
            <Chip mono>identity {actor}</Chip>
          </div>
        </WorkflowLayerCard>

        {hasTeamLayer && (
          <WorkflowLayerCard
            detail="Team posture is derived from reporting-line facts."
            metric={`${roleScope?.team_scope.direct_report_count ?? 0} direct ${
              roleScope?.team_scope.direct_report_count === 1 ? "report" : "reports"
            }`}
            testId="dashboard-team-workflow-layer"
            title="Team Layer"
          >
            <p
              className="ap-soft mt-3 rounded-lg border px-2 py-2"
              data-testid="dashboard-leadership-empty"
              style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
            >
              Team workflow rows are not exposed by the current API.
            </p>
          </WorkflowLayerCard>
        )}

        {hasDepartmentLayer && (
          <WorkflowLayerCard
            detail="Department context is limited to projects already visible to this Work Identity."
            metric={departmentId}
            testId="dashboard-department-workflow-layer"
            title="Department Layer"
          >
            <div className="mt-3 flex flex-wrap gap-1.5">
              <Chip>{departmentCapabilityIds.size} visible projects</Chip>
              <Chip>{departmentWorkflowItems.length} workflow rows</Chip>
            </div>
            {departmentCapabilityIds.size === 0 && (
              <p
                className="ap-soft mt-3 rounded-lg border px-2 py-2"
                data-testid="dashboard-leadership-empty"
                style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
              >
                No department project rows are visible for this Work Identity.
              </p>
            )}
          </WorkflowLayerCard>
        )}

        {inbox.length > 0 && (
          <WorkflowLayerCard
            detail="Only loaded request inbox rows become approval workflow actions."
            metric={`${inbox.length} pending`}
            testId="dashboard-approval-workflow-layer"
            title="Approval Layer"
          >
            <div className="mt-3 space-y-1.5">
              {inbox.slice(0, 3).map((request) => (
                <a
                  key={request.request_id}
                  href={`/project?cap=${encodeURIComponent(request.target.capability_id)}&as=${encodeURIComponent(actor)}`}
                  className="ap-washable block rounded-lg border px-2 py-1"
                  data-testid="dashboard-approval-workflow-row"
                >
                  <span className="ap-register-chrome block truncate" style={{ fontSize: TYPE.scale.xs }}>
                    {request.target.capability_id}
                  </span>
                  <span className="ap-register-evidence ap-soft block truncate" style={{ fontSize: TYPE.scale.xs }}>
                    {request.status}
                  </span>
                </a>
              ))}
            </div>
          </WorkflowLayerCard>
        )}

        {hasExecutiveSignal && (
          <WorkflowLayerCard
            detail="Candidate signal is displayed without unlocking restricted workflows."
            metric="label only"
            testId="dashboard-executive-workflow-label"
            title="Executive Candidate"
          >
            <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
              Elevated workflow surfaces require production authority binding.
            </p>
          </WorkflowLayerCard>
        )}
      </div>
    </section>
  );
}

function WorkflowLayerCard({
  children,
  detail,
  metric,
  testId,
  title,
}: {
  children: React.ReactNode;
  detail: string;
  metric: string;
  testId: string;
  title: string;
}) {
  return (
    <MotionArticle className="ap-card rounded-lg border p-3" data-testid={testId}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            {title}
          </p>
          <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            {detail}
          </p>
        </div>
        <Chip>{metric}</Chip>
      </div>
      {children}
    </MotionArticle>
  );
}

export function EmployeeDashboard({ actor }: { actor: string | null }) {
  const [lens, setLens] = useState<LensResponse | null>(null);
  const [graph, setGraph] = useState<GraphResponse | null>(null);
  const [summary, setSummary] = useState<NodeSummary | null>(null);
  const [roleScope, setRoleScope] = useState<RoleScopeSummary | null>(null);
  const [requests, setRequests] = useState<AccessRequestRecord[]>([]);
  const [grants, setGrants] = useState<AccessGrantRecord[]>([]);
  const [inbox, setInbox] = useState<AccessRequestRecord[]>([]);
  const [workflows, setWorkflows] = useState<ProjectWorkflowResponse[]>([]);
  const [grantError, setGrantError] = useState<string | null>(null);
  const [revokingGrantId, setRevokingGrantId] = useState<string | null>(null);
  const [activePanel, setActivePanel] = useState<DashboardPanelMode | null>(null);
  const [loading, setLoading] = useState(false);
  const [available, setAvailable] = useState(true);

  useEffect(() => {
    if (actor === null) {
      setLens(null);
      setGraph(null);
      setSummary(null);
      setRoleScope(null);
      setRequests([]);
      setGrants([]);
      setInbox([]);
      setWorkflows([]);
      setGrantError(null);
      setRevokingGrantId(null);
      setAvailable(true);
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setAvailable(true);

    Promise.all([
      api.getLens(actor, actor),
      api.getGraph(actor),
      api.getNodeSummary(actor, actor),
      api.getRoleScope(actor),
      api.getAccessRequests(actor),
      api.getAccessGrants(actor),
      api.getAccessRequestInbox(actor),
    ])
      .then(async ([
        lensResponse,
        graphResponse,
        summaryResponse,
        roleScopeResponse,
        requestResponse,
        grantResponse,
        inboxResponse,
      ]) => {
        if (cancelled) return;
        setLens(lensResponse);
        setGraph(graphResponse);
        setSummary(summaryResponse);
        setRoleScope(roleScopeResponse);
        setRequests(requestResponse?.requests ?? []);
        setGrants(grantResponse?.grants ?? []);
        setInbox(inboxResponse?.requests ?? []);
        setGrantError(null);
        setAvailable(lensResponse !== null);

        const projects = lensResponse?.subject_human?.projects ?? [];
        const workflowResponses = await Promise.all(
          projects.map((project) => api.getProjectWorkflow(actor, project.capability_id)),
        );
        if (!cancelled) {
          setWorkflows(
            workflowResponses.filter(
              (workflow): workflow is ProjectWorkflowResponse => workflow !== null,
            ),
          );
        }
      })
      .catch(() => {
        if (!cancelled) {
          setLens(null);
          setGraph(null);
          setSummary(null);
          setRoleScope(null);
          setRequests([]);
          setGrants([]);
          setInbox([]);
          setWorkflows([]);
          setGrantError(null);
          setRevokingGrantId(null);
          setAvailable(false);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [actor]);

  const projectById = useMemo(() => {
    const map = new Map<string, GraphProject>();
    for (const project of graph?.projects ?? []) map.set(project.id, project);
    return map;
  }, [graph]);

  if (actor === null) {
    return (
      <main className="min-w-0 flex-1" data-testid="employee-dashboard-empty">
        <MotionSection className="ap-card rounded-lg p-4">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Work Identity
          </p>
          <h1 className="ap-register-chrome mt-2" style={{ fontSize: TYPE.scale.lg, fontWeight: 600 }}>
            Choose a Work Identity to begin.
          </h1>
          <p className="ap-soft mt-2 max-w-2xl" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
            No employee is selected yet, so Enterprise Brain has no permission scope for work,
            access requests, Granted Knowledge, or Ask. Selecting a Work Identity shows only the
            data available to that identity.
          </p>
          <div className="mt-4 flex flex-wrap gap-2">
            <MotionAnchor
              href="/me?as=p060"
              className="ap-affordance-button ap-register-chrome rounded-lg px-3 py-2"
              style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
              data-testid="employee-empty-start-link"
            >
              Open demo Work Identity
            </MotionAnchor>
            <MotionAnchor
              href="/ask?as=p060"
              className="ap-washable ap-register-chrome rounded-lg border px-3 py-2"
              style={{ borderColor: "var(--hairline)", fontSize: TYPE.scale.xs, fontWeight: 600 }}
              data-testid="employee-empty-ask-link"
            >
              Open Ask with that identity
            </MotionAnchor>
          </div>
        </MotionSection>
      </main>
    );
  }

  if (loading) {
    return (
      <main className="min-w-0 flex-1" data-testid="employee-dashboard-loading">
        <div className="ap-card rounded-lg p-4">
          <Skeleton lines={8} />
        </div>
      </main>
    );
  }

  if (!available || lens === null) {
    return (
      <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="employee-dashboard-unavailable">
        This Work Identity is not available in the current permission scope.
      </p>
    );
  }

  const human = lens.subject_human;
  const projects = human?.projects ?? [];
  const workflowItems = workflows.flatMap((workflow) => workflow.items);
  const scopeBadges = deriveScopeBadges({
    grants,
    human,
    inbox,
    requests,
    roleScope,
    summary,
    subjectDepartment: lens.subject.department,
  });
  const roleExperienceCards = buildRoleExperienceCards({
    actor,
    grants,
    inbox,
    requests,
    roleScope,
    workflowItems,
  });
  const notificationItems = buildNotificationItems({
    actor,
    grants,
    inbox,
    requests,
    roleScope,
    workflowItems,
  });
  const todayCockpit = buildTodayCockpit({
    actor,
    grants,
    inbox,
    projectById,
    projects,
    requests,
    roleScope,
    workflowItems,
  });

  async function revokeGrant(grantId: string) {
    if (!actor) return;
    const actorId = actor;
    setRevokingGrantId(grantId);
    setGrantError(null);
    try {
      const response = await api.postAccessGrantRevoke(actorId, grantId, "approver_revoked");
      setGrants((current) =>
        current.map((grant) => (grant.grant_id === grantId ? response.grant : grant)),
      );
    } catch {
      setGrantError("Grant revoke failed.");
    } finally {
      setRevokingGrantId(null);
    }
  }

  return (
    <main className="min-w-0 flex-1" data-testid="employee-dashboard">
      <header
        className="ap-nav sticky top-2 z-20 mb-3 rounded-2xl px-3 py-2"
        data-layout="compact-strip"
        data-testid="dashboard-cockpit-header"
      >
        <div className="flex flex-wrap items-center gap-3">
          <div
            role="button"
            tabIndex={0}
            onClick={() => setActivePanel("profile")}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                setActivePanel("profile");
              }
            }}
            aria-label="Open profile"
            data-testid="dashboard-identity-open-profile"
            className="ap-washable inline-flex min-w-0 flex-1 cursor-pointer items-center gap-3 rounded-2xl px-1.5 py-1 text-left"
          >
            <PersonAvatar
              principalId={actor}
              displayName={human?.display_name ?? lens.subject.name}
              department={human?.department_label ?? lens.subject.department ?? null}
              size={40}
            />
            <div className="min-w-0 flex-1">
              <h1
                className="ap-register-chrome mt-1"
                style={{ fontSize: TYPE.scale.md, fontWeight: 700, lineHeight: TYPE.line.display }}
                data-testid="dashboard-user-name"
              >
                {human?.display_name ?? lens.subject.name}
              </h1>
              <p className="ap-soft truncate" style={{ fontSize: TYPE.scale.xs }}>
                {human?.title ?? "Role unavailable"}
                {human?.department_label ? ` / ${human.department_label}` : ""}
              </p>
            </div>
            {/* Trailing chevron: the no-affordance cue that this opens the Profile drawer. */}
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true" className="ap-soft shrink-0">
              <path d="M6 4l4 4-4 4" />
            </svg>
          </div>
          <Chip mono>{actor}</Chip>
          <Chip>Demo Identity Mode</Chip>
          <div className="flex flex-wrap items-center justify-end gap-2" data-testid="dashboard-identity-strip">
            <DashboardPanelTabs active={activePanel} onSelect={(mode) => setActivePanel((current) => (current === mode ? null : mode))} />
            <a
              href={`/ask?as=${encodeURIComponent(actor)}`}
              className="ap-affordance-button ap-register-chrome min-h-10 rounded-full px-3 py-2"
              style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
              data-testid="dashboard-ask-link"
            >
              Ask
            </a>
            <ThemeToggle compact />
          </div>
        </div>
      </header>

      <section
        className="space-y-3"
        data-testid="dashboard-compact-cockpit"
      >
        <div className="min-w-0 space-y-3" data-testid="dashboard-main-cockpit">
          <TodayCockpit model={todayCockpit} />
          <div className="grid grid-cols-1 gap-3 lg:grid-cols-[minmax(0,0.95fr)_minmax(0,1.15fr)_minmax(280px,0.72fr)]">
            <Panel
              title="My Projects"
              action={
                <a
                  className="ap-register-chrome ap-washable rounded-lg px-2 py-1"
                  href={`/project?as=${encodeURIComponent(actor)}`}
                  style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
                >
                  Open board / {projects.length}
                </a>
              }
            >
              <ProjectsList actor={actor} projects={projects} projectById={projectById} />
            </Panel>

            <Panel
              title="My Workflow"
              action={<span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>{workflowItems.length}</span>}
            >
              <WorkflowSummary actor={actor} items={workflowItems} />
            </Panel>
            <AskAgentCard actor={actor} grants={grants} />
          </div>
        </div>
      </section>

      <DashboardPanelDrawer mode={activePanel} onClose={() => setActivePanel(null)}>
        {activePanel === "workspace" ? (
            <WorkspacePanel
              actor={actor}
              grantError={grantError}
              grants={grants}
              inbox={inbox}
              notificationItems={notificationItems}
              onRevokeGrant={revokeGrant}
              projectById={projectById}
              projects={projects}
              requests={requests}
              revokingGrantId={revokingGrantId}
              roleScope={roleScope}
              workflowItems={workflowItems}
            />
          ) : activePanel === "profile" ? (
            <ProfilePanel
              actor={actor}
              grants={grants}
              human={human}
              inbox={inbox}
              lens={lens}
              projectById={projectById}
              requests={requests}
              roleExperienceCards={roleExperienceCards}
              roleScope={roleScope}
              scopeBadges={scopeBadges}
              summary={summary}
            />
          ) : activePanel === "settings" ? (
            <SettingsPanel graph={graph} roleScope={roleScope} summary={summary} />
          ) : null}
      </DashboardPanelDrawer>
    </main>
  );
}

function GrantedKnowledgeList({
  actor,
  grants,
  projectById,
}: {
  actor: string;
  grants: AccessGrantRecord[];
  projectById: Map<string, GraphProject>;
}) {
  const activeGrants = activeKnowledgeGrants(grants, actor);
  if (activeGrants.length === 0) {
    return <EmptyLine>No active granted knowledge is available for this Work Identity.</EmptyLine>;
  }
  return (
    <div className="space-y-2" data-testid="dashboard-granted-knowledge" id="dashboard-granted-knowledge">
      {activeGrants.map((grant, index) => {
        const capabilityId = grant.target.capability_id;
        const project = projectById.get(capabilityId);
        const title = project?.label.replace(/^Capability:\s*/i, "") ?? capabilityId;
        const href = `/ask?as=${encodeURIComponent(actor)}&grant=${encodeURIComponent(
          grant.grant_id,
        )}&cap=${encodeURIComponent(capabilityId)}`;
        return (
          <MotionArticle
            key={grant.grant_id}
            className="ap-card rounded-lg border p-3"
            delayIndex={index}
            data-testid="dashboard-granted-knowledge-card"
            style={dashboardPanelStyle()}
          >
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="min-w-0">
                <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
                  {grant.target.kind} read grant
                </p>
                <h3 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
                  {title}
                </h3>
              </div>
              <Chip>{grant.status}</Chip>
            </div>
            <div className="mt-2 flex flex-wrap gap-1.5">
              <Chip mono>{grant.grant_id}</Chip>
              <Chip mono>request {grant.request_id}</Chip>
              <Chip mono>approver {grant.approver_id}</Chip>
            </div>
            <a
              href={href}
              className="ap-affordance-button ap-register-chrome mt-3 inline-flex min-h-10 items-center rounded-lg px-3 py-2"
              style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
              data-testid="dashboard-open-grant-ask"
            >
              Open in Ask
            </a>
          </MotionArticle>
        );
      })}
    </div>
  );
}

function ScopePosture({ badges }: { badges: ScopeBadge[] }) {
  return (
    <div data-testid="dashboard-scope" id="dashboard-scope">
      <p className="ap-soft" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
        Derived from visible Work Identity, reporting, project, agent, and request facts. Server-side
        authorization is still handled by the existing scoped APIs; this label is derived, not
        enforced.
      </p>
      <div className="mt-3 grid grid-cols-1 gap-2">
        {badges.map((badge, index) => (
          <MotionArticle key={`${badge.label}:${badge.detail}`} className="ap-card rounded-lg p-2" delayIndex={index}>
            <div className="flex items-start justify-between gap-2">
              <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
                {badge.label}
              </p>
              <span className="ap-register-evidence ap-soft shrink-0" style={{ fontSize: TYPE.scale.xs }}>
                {badge.source}
              </span>
            </div>
            <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
              {badge.detail}
            </p>
          </MotionArticle>
        ))}
      </div>
    </div>
  );
}

function ProjectsList({
  actor,
  projectById,
  projects,
}: {
  actor: string;
  projectById: Map<string, GraphProject>;
  projects: ProjectRecord[];
}) {
  if (projects.length === 0) {
    return <EmptyLine>No assigned projects for this Work Identity.</EmptyLine>;
  }
  const visibleProjects = projects.slice(0, 2);
  return (
    <div className="grid grid-cols-1 gap-2 md:grid-cols-2" data-testid="dashboard-projects">
      {visibleProjects.map((project, index) => {
        const graphProject = projectById.get(project.capability_id);
        return (
          <MotionAnchor
            key={project.capability_id}
            href={`/project?cap=${encodeURIComponent(project.capability_id)}&as=${encodeURIComponent(actor)}`}
            className="ap-card ap-washable rounded-lg p-2.5"
            delayIndex={index}
            data-testid="dashboard-project"
          >
            <div className="flex items-start justify-between gap-2">
              <div className="min-w-0">
                <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
                  {project.capability_name}
                </p>
                <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
                  {project.workflow_name}
                </p>
              </div>
              <Chip>{project.status}</Chip>
            </div>
            <div className="mt-2 flex flex-wrap gap-1.5">
              <Chip>{project.role}</Chip>
              <Chip mono>{project.capability_id}</Chip>
            </div>
            {graphProject && Object.keys(graphProject.status_counts).length > 0 && (
              <p className="ap-soft mt-1.5 truncate" style={{ fontSize: TYPE.scale.xs }}>
                {Object.entries(graphProject.status_counts)
                  .map(([status, count]) => `${status}: ${count}`)
                  .join(" / ")}
              </p>
            )}
          </MotionAnchor>
        );
      })}
    </div>
  );
}

function WorkflowSummary({ actor, items }: { actor: string; items: WorkflowItem[] }) {
  if (items.length === 0) {
    return <EmptyLine>No workflow items are projected for your assigned projects.</EmptyLine>;
  }
  // Cockpit digest: only the items actually in flight (everything except Done),
  // in lane-priority order, capped at five. The full five-lane board lives on the
  // Workflow Command surface (/project), reachable via "Open workflow".
  const ACTIVE_ORDER = ["In Progress", "Next", "Waiting", "Blocked"];
  const active = items
    .filter((item) => workflowGroup(item.status) !== "Done")
    .sort(
      (a, b) =>
        ACTIVE_ORDER.indexOf(workflowGroup(a.status)) - ACTIVE_ORDER.indexOf(workflowGroup(b.status)),
    );
  const digest = active.slice(0, 5);
  return (
    <div className="space-y-2" data-testid="dashboard-workflow" id="dashboard-workflow">
      {digest.length === 0 ? (
        <EmptyLine>No active workflow items in flight. Completed work lives in Workflow Command.</EmptyLine>
      ) : (
        digest.map((item, index) => (
          <MotionAnchor
            key={item.item_id}
            href={`/project?cap=${encodeURIComponent(item.capability_id)}&as=${encodeURIComponent(actor)}`}
            className="ap-card ap-washable flex items-center justify-between gap-3 rounded-lg border px-2.5 py-2"
            delayIndex={index}
            data-testid="dashboard-workflow-item"
          >
            <span className="ap-register-chrome min-w-0 truncate" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
              {item.title}
            </span>
            <span
              className="ap-soft shrink-0"
              style={{ fontSize: TYPE.scale.xs }}
              data-testid="dashboard-workflow-item-status"
            >
              {workflowStatusLabel(item.status)}
            </span>
          </MotionAnchor>
        ))
      )}
      <a
        href={`/project?as=${encodeURIComponent(actor)}`}
        className="ap-affordance-text ap-register-chrome inline-flex min-h-10 items-center gap-1 px-1 py-1"
        style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
        data-testid="dashboard-workflow-open"
      >
        Open workflow
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
          <path d="M6 4l4 4-4 4" />
        </svg>
      </a>
    </div>
  );
}

function AgentsList({ agents }: { agents: NonNullable<NodeSummary["agents_owned"]> }) {
  if (agents.length === 0) {
    return <EmptyLine>No owned agents are visible for this Work Identity.</EmptyLine>;
  }
  return (
    <div className="space-y-2" data-testid="dashboard-agents">
      {agents.map((agent, index) => (
        <MotionArticle key={agent.id} className="ap-card rounded-lg p-2" delayIndex={index} data-testid="dashboard-agent">
          <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            {agent.name}
          </p>
          <p className="ap-register-evidence ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
            {agent.id}
          </p>
        </MotionArticle>
      ))}
    </div>
  );
}

function RequestsList({
  actor,
  grantError,
  grants,
  inbox,
  onRevokeGrant,
  projectById,
  requests,
  revokingGrantId,
}: {
  actor: string;
  grantError: string | null;
  grants: AccessGrantRecord[];
  inbox: AccessRequestRecord[];
  onRevokeGrant: (grantId: string) => void;
  projectById: Map<string, GraphProject>;
  requests: AccessRequestRecord[];
  revokingGrantId: string | null;
}) {
  const rows = [
    ...requests.map((request) => ({ label: "Mine", request })),
    ...inbox.map((request) => ({ label: "Approval", request })),
  ];
  if (rows.length === 0 && grants.length === 0) {
    return <EmptyLine>No access requests are active for this Work Identity.</EmptyLine>;
  }
  return (
    <div className="space-y-2" data-testid="dashboard-requests" id="dashboard-requests">
      {grantError && (
        <p className="ap-soft rounded-lg border px-2 py-1" style={{ fontSize: TYPE.scale.xs }} role="alert">
          {grantError}
        </p>
      )}
      {grants.length > 0 && (
        <div className="grid grid-cols-1 gap-2">
          {grants.map((grant, index) => {
            const project = projectById.get(grant.target.capability_id);
            const canRevoke = grant.approver_id === actor && grant.status === "active";
            const isRevoking = revokingGrantId === grant.grant_id;
            return (
              <MotionArticle
                key={grant.grant_id}
                className="ap-card rounded-lg p-2"
                delayIndex={index}
                data-testid="dashboard-grant"
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0">
                    <a
                      href={`/project?cap=${encodeURIComponent(grant.target.capability_id)}&as=${encodeURIComponent(actor)}`}
                      className="ap-register-chrome ap-washable block truncate rounded-lg px-1 py-0.5"
                      style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
                    >
                      {project?.label.replace(/^Capability:\s*/i, "") ?? grant.target.capability_id}
                    </a>
                    <p className="ap-register-evidence ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
                      {grant.grant_id}
                    </p>
                  </div>
                  <Chip>{grant.status}</Chip>
                </div>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  <Chip>{grant.permission} grant</Chip>
                  <Chip mono>request {grant.request_id}</Chip>
                  <Chip mono>approver {grant.approver_id}</Chip>
                  {grant.revoked_by && <Chip mono>revoked by {grant.revoked_by}</Chip>}
                </div>
                {canRevoke && (
                  <button
                    type="button"
                    className="ap-washable ap-register-chrome ap-soft mt-2 rounded-lg border px-2 py-1"
                    style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
                    disabled={isRevoking}
                    onClick={() => onRevokeGrant(grant.grant_id)}
                    data-testid="dashboard-grant-revoke"
                  >
                    {isRevoking ? "Revoking" : "Revoke"}
                  </button>
                )}
              </MotionArticle>
            );
          })}
        </div>
      )}
      {rows.map(({ label, request }, index) => {
        const project = projectById.get(request.target.capability_id);
        return (
          <MotionArticle
            key={`${label}:${request.request_id}`}
            className="ap-card rounded-lg p-2"
            delayIndex={index}
            data-testid="dashboard-request"
          >
            <div className="flex items-start justify-between gap-2">
              <div className="min-w-0">
                <p className="ap-register-chrome truncate" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
                  {project?.label.replace(/^Capability:\s*/i, "") ?? request.target.capability_id}
                </p>
                <p className="ap-register-evidence ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
                  {request.request_id}
                </p>
              </div>
              <Chip>{request.status}</Chip>
            </div>
            <div className="mt-2 flex flex-wrap gap-1.5">
              <Chip>{label}</Chip>
              <Chip mono>requester {request.requester_id}</Chip>
              <Chip mono>approver {request.approver_id}</Chip>
            </div>
          </MotionArticle>
        );
      })}
    </div>
  );
}

function KnowledgeSummary({
  holdings,
  rows,
  sections,
}: {
  holdings: { count: number; sentence: string }[];
  rows: number;
  sections: number;
}) {
  return (
    <div data-testid="dashboard-knowledge">
      <div className="grid grid-cols-2 gap-2">
        <Metric label="Reason groups" value={sections} />
        <Metric label="Visible rows" value={rows} />
      </div>
      <div className="mt-3 space-y-1.5">
        {holdings.slice(0, 4).map((section) => (
          <div key={section.sentence} className="flex items-baseline justify-between gap-3">
            <span className="ap-soft min-w-0 truncate" style={{ fontSize: TYPE.scale.xs }}>
              {section.sentence}
            </span>
            <span className="ap-register-evidence ap-soft shrink-0" style={{ fontSize: TYPE.scale.xs }}>
              {section.count}
            </span>
          </div>
        ))}
        {holdings.length === 0 && <EmptyLine compact>No knowledge rows for this Work Identity.</EmptyLine>}
      </div>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: number }) {
  return (
    <div className="ap-card rounded-lg p-2">
      <p className="ap-register-evidence" style={{ fontSize: TYPE.scale.lg, fontWeight: 600 }}>
        {value}
      </p>
      <p className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
        {label}
      </p>
    </div>
  );
}

function EmptyLine({
  children,
  compact = false,
}: {
  children: React.ReactNode;
  compact?: boolean;
}) {
  return (
    <p
      className="ap-soft"
      style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body, padding: compact ? 0 : 8 }}
      data-testid="dashboard-empty-line"
    >
      {children}
    </p>
  );
}
