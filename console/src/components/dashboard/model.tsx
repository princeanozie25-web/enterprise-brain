import type { LensResponse, NodeSummary } from "@/lib/api";
import { roleLabel, scopeModeLabel } from "./shared";
import type {
  AccessGrantRecord,
  AccessRequestRecord,
  GraphResponse,
  RoleScopeSummary,
  WorkflowItem,
} from "@/lib/api";
import {
  CONNECTOR_NAMES,
  activeKnowledgeGrants,
  isDepartmentHead,
  isExecutiveCandidate,
  type ConnectedSystem,
  type NotificationItem,
  type RoleExperienceCard,
  type ScopeBadge,
} from "./shared";

export function buildNotificationItems({
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


export function deriveConnectedSystems(graph: GraphResponse | null): ConnectedSystem[] {
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


export function deriveScopeBadges({
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


export function buildRoleExperienceCards({
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

