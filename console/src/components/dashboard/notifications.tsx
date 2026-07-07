"use client";
import type { AccessRequestRecord, GraphProject, ProjectRecord } from "@/lib/api";
import { MotionAnchor, MotionArticle } from "../MotionPrimitives";
import { Chip, EmptyLine, dashboardPanelStyle, scopeModeLabel } from "./shared";

import type { RoleScopeSummary, WorkflowItem } from "@/lib/api";
import { TYPE } from "@/lib/tokens";
import { MotionSection } from "../MotionPrimitives";
import {
  isDepartmentHead,
  isExecutiveCandidate,
  roleLabel,
  type NotificationItem,
  type RoleExperienceCard,
} from "./shared";

export function RoleExperienceSummary({ cards }: { cards: RoleExperienceCard[] }) {
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


export function NotificationCenter({ items }: { items: NotificationItem[] }) {
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


export function WorkflowCommandSubbar({ items }: { items: NotificationItem[] }) {
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
              Projects
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


export function WorkspaceNotifications({ items }: { items: NotificationItem[] }) {
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


export function RoleAwareWorkflowLayer({
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


export function WorkflowLayerCard({
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

