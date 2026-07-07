"use client";
import { MotionArticle } from "../MotionPrimitives";
import { activeKnowledgeGrants, dashboardPanelStyle, workflowGroup } from "./shared";

import type {
  AccessGrantRecord,
  AccessRequestRecord,
  GraphProject,
  NodeSummary,
  ProjectRecord,
  WorkflowItem,
} from "@/lib/api";
import { TYPE } from "@/lib/tokens";
import { MotionAnchor } from "../MotionPrimitives";
import {
  Chip,
  EmptyLine,
  Metric,
  capabilityTitle,
  plural,
  projectHref,
  scopeModeLabel,
  workflowStatusLabel,
  type ScopeBadge,
} from "./shared";

export function GrantedKnowledgeList({
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


export function ScopePosture({ badges }: { badges: ScopeBadge[] }) {
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


export function ProjectsList({
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


export function WorkflowSummary({ actor, items }: { actor: string; items: WorkflowItem[] }) {
  if (items.length === 0) {
    return <EmptyLine>No workflow items are projected for your assigned projects.</EmptyLine>;
  }
  // Cockpit digest: only the items actually in flight (everything except Done),
  // in lane-priority order, capped at five. The full five-lane board lives on the
  // Projects surface (/project), reachable via "Open workflow".
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
        <EmptyLine>No active workflow items in flight. Completed work lives in Projects.</EmptyLine>
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


export function AgentsList({ agents }: { agents: NonNullable<NodeSummary["agents_owned"]> }) {
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


export function RequestsList({
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


export function KnowledgeSummary({
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

