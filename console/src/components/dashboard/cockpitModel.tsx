import type { NodeSummary } from "@/lib/api";
import { activeKnowledgeGrants } from "./shared";
import type {
  AccessGrantRecord,
  AccessRequestRecord,
  GraphProject,
  ProjectRecord,
  RoleScopeSummary,
  WorkflowItem,
} from "@/lib/api";
import {
  TODAY_WORKFLOW_STATUSES,
  askGrantHref,
  capabilityTitle,
  isDepartmentHead,
  isExecutiveCandidate,
  plural,
  projectHref,
  workflowStatusLabel,
  type CockpitItem,
  type CommandPodModel,
  type TodayCockpitModel,
} from "./shared";

export function buildTodayCockpit({
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


export function buildCommandPods({
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

