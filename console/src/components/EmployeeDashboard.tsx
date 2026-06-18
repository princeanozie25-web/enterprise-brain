"use client";

import { useEffect, useMemo, useState } from "react";
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
import { PersonAvatar } from "./PersonAvatar";
import { Skeleton } from "./Skeleton";

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

const CONNECTOR_NAMES = ["Gmail", "Outlook", "Teams", "Slack", "Jira", "GitHub", "SharePoint"];

function workflowGroup(status: string): string {
  for (const group of WORKFLOW_GROUPS) {
    if (group.statuses.includes(status)) return group.label;
  }
  return "Next";
}

function dashboardPanelStyle(): React.CSSProperties {
  return {
    backdropFilter: "blur(18px)",
    background: "color-mix(in srgb, var(--paper) 86%, transparent)",
    boxShadow: "inset 0 1px 0 color-mix(in srgb, var(--ink) 8%, transparent)",
  };
}

function Chip({ children, mono = false }: { children: React.ReactNode; mono?: boolean }) {
  return (
    <span
      className={`ap-hairline ${mono ? "ap-register-evidence" : "ap-register-chrome"} ap-soft rounded border px-1.5 py-0.5`}
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
      return "Super admin candidate signal";
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
          ? "Requests assigned to this actor are ready for review."
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
      detail: "Read grant records visible to this actor are available for status review.",
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
      detail: "restricted admin-domain surfaces unavailable",
      label: "Surface limits",
      source: roleScope.enforcement,
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
      source: "current actor",
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
  title,
}: {
  action?: React.ReactNode;
  children: React.ReactNode;
  title: string;
}) {
  return (
    <section className="ap-card rounded p-3" style={dashboardPanelStyle()}>
      <div className="mb-3 flex items-baseline justify-between gap-3">
        <h2 className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
          {title}
        </h2>
        {action}
      </div>
      {children}
    </section>
  );
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
      detail: "Projected from your assigned project workflow items.",
      href: "#dashboard-workflow",
      kind: "work",
      metric: `${workflowItems.length} ${workflowItems.length === 1 ? "item" : "items"}`,
      title: "My Work Pod",
    },
  ];

  if (firstCapabilityId && projectCount > 0) {
    pods.push({
      detail: "Open the project surface with graph and workflow views.",
      href: `/project?cap=${encodeURIComponent(firstCapabilityId)}&as=${encodeURIComponent(actor)}`,
      kind: "project",
      metric: `${projectCount} ${projectCount === 1 ? "project" : "projects"}`,
      title: "Project Context Pod",
    });
  }

  if (activeGrants.length > 0) {
    pods.push({
      detail: "Open only the capabilities unlocked by active read grants.",
      href: "#dashboard-granted-knowledge",
      kind: "grant",
      metric: `${activeGrants.length} active ${activeGrants.length === 1 ? "grant" : "grants"}`,
      title: "Granted Knowledge Pod",
    });
  }

  pods.push({
    callToAction: "Start Conversation",
    detail: "Use your current actor context in the existing ask surface.",
    href: `/ask?as=${encodeURIComponent(actor)}`,
    kind: "ask",
    metric: "actor scoped",
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
      title: "Team Lead Pod",
    });
  }

  if (isDepartmentHead(roleScope?.derived_level) && department) {
    pods.push({
      detail: "Department-head signal from real reporting and title facts.",
      href: "#dashboard-scope",
      kind: "department",
      metric: department,
      title: "Department Context Pod",
    });
  }

  if (pendingApprovals > 0) {
    pods.push({
      detail: "Requests assigned to this actor for review.",
      href: "#dashboard-requests",
      kind: "approval",
      metric: `${pendingApprovals} pending`,
      title: "Approval Queue Pod",
    });
  }

  if (isExecutiveCandidate(roleScope?.derived_level)) {
    pods.push({
      detail: "Candidate signal only. No restricted surface is unlocked.",
      href: "#dashboard-role-experience",
      kind: "executive",
      metric: "candidate only",
      title: "Executive Candidate Pod",
    });
  }

  pods.push({
    detail: "Track submitted requests, assigned reviews, and active read grants.",
    href: "#dashboard-requests",
    kind: "request",
    metric: `${requests.length} mine / ${inbox.length} inbox / ${grants.length} grants`,
    title: "Access Request Pod",
  });

  if (agentCount > 0) {
    pods.push({
      detail: "Launch the existing ask route with visible agent context.",
      href: `/ask?as=${encodeURIComponent(actor)}`,
      kind: "agent",
      metric: `${agentCount} ${agentCount === 1 ? "agent" : "agents"}`,
      title: "Agent Assist Pod",
    });
  }

  return pods;
}

function CommandPods({ pods }: { pods: CommandPodModel[] }) {
  return (
    <section className="mb-4 grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-4" data-testid="dashboard-command-pods">
      {pods.map((pod) => (
        <CommandPod key={`${pod.kind}:${pod.title}`} pod={pod} />
      ))}
    </section>
  );
}

function CommandPod({ pod }: { pod: CommandPodModel }) {
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
          className="ap-affordance-button ap-register-chrome mt-4 inline-flex rounded px-3 py-2"
          style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
        >
          {pod.callToAction}
        </span>
      )}
    </>
  );

  return (
    <a
      href={pod.href}
      className="ap-card ap-washable block rounded p-3"
      data-pod-kind={pod.kind}
      data-testid={isAsk ? "dashboard-ask-pod" : `dashboard-pod-${pod.kind}`}
      style={{
        ...dashboardPanelStyle(),
        minHeight: isAsk ? 178 : 136,
      }}
    >
      {content}
    </a>
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
      detail: "Daily execution stays scoped to this actor.",
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
      detail: "Department context is label-only until server enforcement exists.",
      label: "Department head",
      metric: roleScope?.department_scope.department_id ?? "department",
      source: "server scope",
      tone: "active",
    });
  }
  if (inbox.length > 0) {
    cards.push({
      detail: "Only requests assigned to this actor appear here.",
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
      metric: "not enforced",
      source: "server scope",
      tone: "candidate",
    });
  }
  cards.push({
    detail: "Restricted surfaces remain unavailable in this dashboard.",
    label: "Surface boundary",
    metric: roleScope?.enforcement ?? "derived only",
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
      className="grid grid-cols-1 gap-2 md:grid-cols-2"
      data-testid="dashboard-role-experience"
      id="dashboard-role-experience"
    >
      {cards.map((card) => (
        <div
          key={`${card.label}:${card.metric}`}
          className="ap-card rounded border p-3"
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
        </div>
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
        className="ap-card ap-washable flex min-h-10 cursor-pointer list-none items-center gap-2 rounded px-3 py-2"
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
        className="ap-card absolute right-0 z-20 mt-2 w-[min(380px,calc(100vw-2rem))] rounded border p-3 shadow-lg"
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
                className="ap-washable block rounded border px-3 py-2"
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
    <section
      className="ap-card mb-4 rounded border p-2"
      data-testid="dashboard-workflow-command"
      style={dashboardPanelStyle()}
    >
      <details className="group">
        <summary
          className="ap-washable flex min-h-10 cursor-pointer list-none flex-wrap items-center justify-between gap-3 rounded px-3 py-2"
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
                  className="ap-hairline ap-register-chrome ap-soft rounded border px-2 py-1"
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
            <div className="ap-card rounded border p-3">
              <EmptyLine compact>No command categories are backed by current request, workflow, or grant rows.</EmptyLine>
            </div>
          ) : (
            items.map((item) => (
              <a
                key={`${item.category}:${item.title}:command`}
                href={item.href}
                className="ap-washable block rounded border p-3"
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
              </a>
            ))
          )}
        </div>
      </details>
    </section>
  );
}

function WorkspaceLayer({
  actor,
  graph,
  grants,
  human,
  inbox,
  lens,
  projectById,
  requests,
  roleScope,
  summary,
}: {
  actor: string;
  graph: GraphResponse | null;
  grants: AccessGrantRecord[];
  human: LensResponse["subject_human"];
  inbox: AccessRequestRecord[];
  lens: LensResponse;
  projectById: Map<string, GraphProject>;
  requests: AccessRequestRecord[];
  roleScope: RoleScopeSummary | null;
  summary: NodeSummary | null;
}) {
  const systems = deriveConnectedSystems(graph);
  const directReports = roleScope?.team_scope.direct_report_count ?? human?.manages.length ?? 0;
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
      className="ap-card mb-4 rounded border p-4"
      data-testid="dashboard-workspace"
      id="dashboard-workspace"
      style={dashboardPanelStyle()}
    >
      <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Workspace Layer
          </p>
          <h2 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.lg, fontWeight: 600 }}>
            Work Identity
          </h2>
        </div>
        <Chip>{roleScope?.enforcement ?? "derived only"}</Chip>
      </div>

      <div className="grid grid-cols-1 gap-3 xl:grid-cols-[1.1fr_0.9fr]">
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <WorkspaceBlock title="Identity">
            <WorkspaceFact label="Actor" source="current actor" value={actor} />
            <WorkspaceFact label="Name" source="lens" value={human?.display_name ?? lens.subject.name} />
            <WorkspaceFact label="Title" source="people record" value={human?.title ?? "Role unavailable"} />
          </WorkspaceBlock>

          <WorkspaceBlock title="Role">
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
              value={roleScope?.admin_surface_allowed ? "Admin candidate" : "daily work only"}
            />
          </WorkspaceBlock>

          <WorkspaceBlock title="Department">
            <WorkspaceFact
              label="Department"
              source="lens"
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
          <WorkspaceBlock title="Connected Systems">
            <div className="grid grid-cols-1 gap-2" data-testid="dashboard-connected-systems">
              {systems.length === 0 ? (
                <EmptyLine compact>No supported connected systems are visible through this graph.</EmptyLine>
              ) : (
                systems.map((system) => (
                  <div
                    key={`${system.name}:${system.source}`}
                    className="ap-card flex items-center justify-between gap-3 rounded p-2"
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
            <WorkspaceFact label="Owned agents" source="node summary" value={`${summary?.agents_owned?.length ?? 0} owned agents visible`} />
          </WorkspaceBlock>

          <WorkspaceBlock title="Audit Activity">
            <div className="space-y-2" data-testid="dashboard-audit-activity">
              {auditRows.length === 0 ? (
                <EmptyLine compact>No request, approval, or grant ledger rows are visible.</EmptyLine>
              ) : (
                auditRows.slice(0, 5).map((row) => {
                  const project = projectById.get(row.target);
                  return (
                    <div key={`${row.label}:${row.id}`} className="ap-card rounded p-2">
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
        </div>
      </div>
    </section>
  );
}

function WorkspaceBlock({ children, title }: { children: React.ReactNode; title: string }) {
  return (
    <section className="ap-card rounded border p-3">
      <h3 className="ap-register-chrome mb-2" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
        {title}
      </h3>
      <div className="space-y-2">{children}</div>
    </section>
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
      className="ap-card mb-4 rounded border p-4"
      data-testid="dashboard-role-aware-workflow"
      id="dashboard-role-aware-workflow"
      style={dashboardPanelStyle()}
    >
      <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Additive Workflow Experience
          </p>
          <h2 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.lg, fontWeight: 600 }}>
            Employee Layer + Leadership Layer
          </h2>
        </div>
        <Chip>{roleScope?.enforcement ?? "derived only"}</Chip>
      </div>

      <div className="grid grid-cols-1 gap-3 xl:grid-cols-4">
        <WorkflowLayerCard
          detail="Personal execution remains visible for every actor."
          metric={`${workflowItems.length} workflow ${workflowItems.length === 1 ? "item" : "items"}`}
          testId="dashboard-employee-workflow-layer"
          title="Employee Layer"
        >
          <div className="mt-3 flex flex-wrap gap-1.5">
            <Chip>{projects.length} projects</Chip>
            <Chip>{requests.length} requests</Chip>
            <Chip mono>actor {actor}</Chip>
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
              className="ap-soft mt-3 rounded border px-2 py-2"
              data-testid="dashboard-leadership-empty"
              style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
            >
              Team workflow rows are not exposed by the current API.
            </p>
          </WorkflowLayerCard>
        )}

        {hasDepartmentLayer && (
          <WorkflowLayerCard
            detail="Department context is limited to projects already visible to this actor."
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
                className="ap-soft mt-3 rounded border px-2 py-2"
                data-testid="dashboard-leadership-empty"
                style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
              >
                No department project rows are visible through this actor lens.
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
                  className="ap-washable block rounded border px-2 py-1"
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
            detail="Candidate signal is displayed without unlocking restricted admin-domain workflows."
            metric="label only"
            testId="dashboard-executive-workflow-label"
            title="Executive Candidate"
          >
            <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
              Elevated workflow surfaces require explicit server-enforced privileges.
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
    <article className="ap-card rounded border p-3" data-testid={testId}>
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
    </article>
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
      <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="employee-dashboard-empty">
        Select a lens to open Work Identity.
      </p>
    );
  }

  if (loading) {
    return (
      <main className="min-w-0 flex-1" data-testid="employee-dashboard-loading">
        <div className="ap-card rounded p-4">
          <Skeleton lines={8} />
        </div>
      </main>
    );
  }

  if (!available || lens === null) {
    return (
      <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="employee-dashboard-unavailable">
        Work Identity is not available through this lens.
      </p>
    );
  }

  const human = lens.subject_human;
  const projects = human?.projects ?? [];
  const workflowItems = workflows.flatMap((workflow) => workflow.items);
  const knowledgeSections = lens.holdings.length;
  const visibleKnowledgeRows = lens.holdings.reduce((sum, section) => sum + section.docs.length, 0);
  const scopeBadges = deriveScopeBadges({
    grants,
    human,
    inbox,
    requests,
    roleScope,
    summary,
    subjectDepartment: lens.subject.department,
  });
  const commandPods = buildCommandPods({
    actor,
    grants,
    inbox,
    projects,
    requests,
    roleScope,
    summary,
    workflowItems,
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
      <header className="ap-card mb-4 overflow-hidden rounded p-4" style={dashboardPanelStyle()}>
        <div className="flex flex-wrap items-center gap-3">
          <PersonAvatar
            principalId={actor}
            displayName={human?.display_name ?? lens.subject.name}
            department={human?.department_label ?? lens.subject.department ?? null}
            size={48}
          />
          <div className="min-w-0 flex-1">
            <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
              Current actor {actor}
            </p>
            <h1
              className="ap-register-chrome mt-1"
              style={{ fontSize: TYPE.scale.xl, fontWeight: 600, lineHeight: TYPE.line.display }}
              data-testid="dashboard-user-name"
            >
              {human?.display_name ?? lens.subject.name}
            </h1>
            <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.sm }}>
              {human?.title ?? "Role unavailable"}
              {human?.department_label ? ` / ${human.department_label}` : ""}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <NotificationCenter items={notificationItems} />
            <a
              href={`/ask?as=${encodeURIComponent(actor)}`}
              className="ap-affordance-button ap-register-chrome min-h-10 rounded px-3 py-2"
              style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
              data-testid="dashboard-ask-link"
            >
              Ask Enterprise Brain
            </a>
          </div>
        </div>
      </header>

      <CommandPods pods={commandPods} />

      <WorkflowCommandSubbar items={notificationItems} />

      <RoleAwareWorkflowLayer
        actor={actor}
        inbox={inbox}
        projectById={projectById}
        projects={projects}
        requests={requests}
        roleScope={roleScope}
        workflowItems={workflowItems}
      />

      <WorkspaceLayer
        actor={actor}
        graph={graph}
        grants={grants}
        human={human}
        inbox={inbox}
        lens={lens}
        projectById={projectById}
        requests={requests}
        roleScope={roleScope}
        summary={summary}
      />

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-[1.15fr_0.85fr]">
        <div className="space-y-4">
          <Panel
            title="Role Experience"
            action={<Chip>{roleScope ? roleLabel(roleScope.derived_level) : "derived posture unavailable"}</Chip>}
          >
            <RoleExperienceSummary cards={roleExperienceCards} />
          </Panel>

          <Panel
            title="Scope Posture"
            action={<Chip>derived, not enforced</Chip>}
          >
            <ScopePosture badges={scopeBadges} />
          </Panel>

          <Panel
            title="My Projects"
            action={<span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>{projects.length}</span>}
          >
            <ProjectsList actor={actor} projects={projects} projectById={projectById} />
          </Panel>

          <Panel
            title="My Workflow"
            action={<span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>{workflowItems.length}</span>}
          >
            <WorkflowSummary actor={actor} items={workflowItems} />
          </Panel>
        </div>

        <div className="space-y-4">
          <Panel
            title="My Agents"
            action={<span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>{summary?.agents_owned?.length ?? 0}</span>}
          >
            <AgentsList agents={summary?.agents_owned ?? []} />
          </Panel>

          <Panel
            title="My Requests"
            action={<span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>{requests.length + inbox.length + grants.length}</span>}
          >
            <RequestsList
              actor={actor}
              grantError={grantError}
              grants={grants}
              requests={requests}
              inbox={inbox}
              onRevokeGrant={revokeGrant}
              projectById={projectById}
              revokingGrantId={revokingGrantId}
            />
          </Panel>

          <Panel
            title="Granted Knowledge"
            action={
              <span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
                {grants.filter((grant) => grant.status === "active" && grant.grantee_id === actor).length}
              </span>
            }
          >
            <GrantedKnowledgeList actor={actor} grants={grants} projectById={projectById} />
          </Panel>

          <Panel title="My Knowledge">
            <KnowledgeSummary
              sections={knowledgeSections}
              rows={visibleKnowledgeRows}
              holdings={lens.holdings.map((section) => ({
                count: section.docs.length,
                sentence: section.sentence,
              }))}
            />
          </Panel>
        </div>
      </div>
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
    return <EmptyLine>No active granted knowledge is available for this actor.</EmptyLine>;
  }
  return (
    <div className="space-y-2" data-testid="dashboard-granted-knowledge" id="dashboard-granted-knowledge">
      {activeGrants.map((grant) => {
        const capabilityId = grant.target.capability_id;
        const project = projectById.get(capabilityId);
        const title = project?.label.replace(/^Capability:\s*/i, "") ?? capabilityId;
        const href = `/ask?as=${encodeURIComponent(actor)}&grant=${encodeURIComponent(
          grant.grant_id,
        )}&cap=${encodeURIComponent(capabilityId)}`;
        return (
          <article
            key={grant.grant_id}
            className="ap-card rounded border p-3 transition-transform duration-200 hover:-translate-y-px"
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
              className="ap-affordance-button ap-register-chrome mt-3 inline-flex min-h-10 items-center rounded px-3 py-2"
              style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
              data-testid="dashboard-open-grant-ask"
            >
              Open in Ask
            </a>
          </article>
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
      <div className="mt-3 grid grid-cols-1 gap-2 md:grid-cols-2">
        {badges.map((badge) => (
          <div key={`${badge.label}:${badge.detail}`} className="ap-card rounded p-2">
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
          </div>
        ))}
      </div>
      <div className="ap-card mt-3 rounded p-2">
        <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
          Enforcement status
        </p>
        <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
          This dashboard labels the current actor posture only. It does not create admin access,
          expose restricted admin-domain data, or replace server-side graph/lens/workflow filtering.
        </p>
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
    return <EmptyLine>No assigned projects through this lens.</EmptyLine>;
  }
  return (
    <div className="grid grid-cols-1 gap-2 md:grid-cols-2" data-testid="dashboard-projects">
      {projects.map((project) => {
        const graphProject = projectById.get(project.capability_id);
        return (
          <a
            key={project.capability_id}
            href={`/project?cap=${encodeURIComponent(project.capability_id)}&as=${encodeURIComponent(actor)}`}
            className="ap-card ap-washable rounded p-3"
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
            <div className="mt-3 flex flex-wrap gap-1.5">
              <Chip mono>{project.capability_id}</Chip>
              <Chip>{project.role}</Chip>
              {graphProject && <Chip>{graphProject.people} people</Chip>}
            </div>
            {graphProject && Object.keys(graphProject.status_counts).length > 0 && (
              <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs }}>
                {Object.entries(graphProject.status_counts)
                  .map(([status, count]) => `${status}: ${count}`)
                  .join(" / ")}
              </p>
            )}
          </a>
        );
      })}
    </div>
  );
}

function WorkflowSummary({ actor, items }: { actor: string; items: WorkflowItem[] }) {
  if (items.length === 0) {
    return <EmptyLine>No workflow items are projected for your assigned projects.</EmptyLine>;
  }
  const grouped = new Map<string, WorkflowItem[]>();
  for (const group of WORKFLOW_GROUPS) grouped.set(group.label, []);
  for (const item of items) grouped.get(workflowGroup(item.status))?.push(item);
  return (
    <div className="grid grid-cols-1 gap-2 lg:grid-cols-5" data-testid="dashboard-workflow" id="dashboard-workflow">
      {WORKFLOW_GROUPS.map((group) => {
        const groupItems = grouped.get(group.label) ?? [];
        return (
          <div key={group.label} className="ap-card rounded p-2" data-testid="dashboard-workflow-group">
            <div className="mb-2 flex items-center justify-between gap-2">
              <span className="ap-register-chrome" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
                {group.label}
              </span>
              <span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
                {groupItems.length}
              </span>
            </div>
            <div className="space-y-1.5">
              {groupItems.slice(0, 3).map((item) => (
                <a
                  key={item.item_id}
                  href={`/project?cap=${encodeURIComponent(item.capability_id)}&as=${encodeURIComponent(actor)}`}
                  className="ap-washable block rounded px-2 py-1"
                  data-testid="dashboard-workflow-item"
                >
                  <span className="ap-register-chrome block truncate" style={{ fontSize: TYPE.scale.xs }}>
                    {item.title}
                  </span>
                  <span className="ap-register-evidence ap-soft block truncate" style={{ fontSize: TYPE.scale.xs }}>
                    {item.status}
                  </span>
                </a>
              ))}
              {groupItems.length === 0 && <EmptyLine compact>Empty</EmptyLine>}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function AgentsList({ agents }: { agents: NonNullable<NodeSummary["agents_owned"]> }) {
  if (agents.length === 0) {
    return <EmptyLine>No owned agents are visible through this lens.</EmptyLine>;
  }
  return (
    <div className="space-y-2" data-testid="dashboard-agents">
      {agents.map((agent) => (
        <div key={agent.id} className="ap-card rounded p-2" data-testid="dashboard-agent">
          <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            {agent.name}
          </p>
          <p className="ap-register-evidence ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
            {agent.id}
          </p>
        </div>
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
    return <EmptyLine>No access requests are active for this actor.</EmptyLine>;
  }
  return (
    <div className="space-y-2" data-testid="dashboard-requests" id="dashboard-requests">
      {grantError && (
        <p className="ap-soft rounded border px-2 py-1" style={{ fontSize: TYPE.scale.xs }} role="alert">
          {grantError}
        </p>
      )}
      {grants.length > 0 && (
        <div className="grid grid-cols-1 gap-2">
          {grants.map((grant) => {
            const project = projectById.get(grant.target.capability_id);
            const canRevoke = grant.approver_id === actor && grant.status === "active";
            const isRevoking = revokingGrantId === grant.grant_id;
            return (
              <div
                key={grant.grant_id}
                className="ap-card rounded p-2"
                data-testid="dashboard-grant"
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0">
                    <a
                      href={`/project?cap=${encodeURIComponent(grant.target.capability_id)}&as=${encodeURIComponent(actor)}`}
                      className="ap-register-chrome ap-washable block truncate rounded px-1 py-0.5"
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
                    className="ap-washable ap-register-chrome ap-soft mt-2 rounded border px-2 py-1"
                    style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
                    disabled={isRevoking}
                    onClick={() => onRevokeGrant(grant.grant_id)}
                    data-testid="dashboard-grant-revoke"
                  >
                    {isRevoking ? "Revoking" : "Revoke"}
                  </button>
                )}
              </div>
            );
          })}
        </div>
      )}
      {rows.map(({ label, request }) => {
        const project = projectById.get(request.target.capability_id);
        return (
          <div key={`${label}:${request.request_id}`} className="ap-card rounded p-2" data-testid="dashboard-request">
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
          </div>
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
        {holdings.length === 0 && <EmptyLine compact>No knowledge rows in this lens.</EmptyLine>}
      </div>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: number }) {
  return (
    <div className="ap-card rounded p-2">
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
