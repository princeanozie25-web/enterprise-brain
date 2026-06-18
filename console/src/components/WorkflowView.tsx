"use client";

import type { ProjectWorkflowResponse, RoleScopeSummary, WorkflowItem } from "@/lib/api";
import { TYPE } from "@/lib/tokens";
import { MotionArticle, MotionPanel, MotionSection } from "./MotionPrimitives";
import { Skeleton } from "./Skeleton";

type WorkflowGroup = {
  label: string;
  statuses: string[];
};

const GROUPS: WorkflowGroup[] = [
  { label: "In Progress", statuses: ["active"] },
  { label: "Next", statuses: ["candidate", "planned"] },
  { label: "Waiting", statuses: ["pending"] },
  { label: "Blocked", statuses: ["blocked", "denied", "cancelled", "expired", "dismissed"] },
  { label: "Done", statuses: ["done", "approved"] },
];

const KIND_LABEL: Record<WorkflowItem["kind"], string> = {
  access_request: "Access request",
  accepted_agent_box: "Accepted agent box",
  lane_box: "Lane box",
};

const GROUP_LABELS: Record<string, string> = {
  active: "In Progress",
  approved: "Done",
  blocked: "Blocked",
  candidate: "Next",
  cancelled: "Blocked",
  denied: "Blocked",
  dismissed: "Blocked",
  done: "Done",
  expired: "Blocked",
  pending: "Waiting",
  planned: "Next",
};

function groupFor(status: string): string {
  return GROUP_LABELS[status] ?? "Next";
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

export function WorkflowView({
  workflow,
  loading = false,
  available = true,
  roleScope = null,
}: {
  workflow: ProjectWorkflowResponse | null;
  loading?: boolean;
  available?: boolean;
  roleScope?: RoleScopeSummary | null;
}) {
  if (loading) {
    return (
      <div className="ap-card rounded p-4" data-testid="workflow-loading">
        <Skeleton lines={5} />
      </div>
    );
  }

  if (!available || workflow === null) {
    return (
      <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="workflow-unavailable">
        Workflow projection is not available for this project.
      </p>
    );
  }

  const grouped = new Map<string, WorkflowItem[]>();
  for (const group of GROUPS) {
    grouped.set(group.label, []);
  }
  for (const item of workflow.items) {
    grouped.get(groupFor(item.status))?.push(item);
  }

  return (
    <MotionSection data-testid="workflow-view">
      <div className="mb-4 flex flex-wrap items-end justify-between gap-3">
        <div className="min-w-0">
          <h2
            className="ap-register-chrome"
            style={{ fontSize: TYPE.scale.lg, fontWeight: 600, lineHeight: TYPE.line.display }}
          >
            Workflow Command
          </h2>
          <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
            {workflow.provenance.workflow.name}
          </p>
        </div>
        <span
          className="ap-card ap-register-evidence ap-soft rounded-full px-2 py-1"
          style={{ fontSize: TYPE.scale.xs }}
        >
          {workflow.items.length} real items
        </span>
      </div>

      <WorkflowRolePosture workflow={workflow} roleScope={roleScope} />

      <div className="grid grid-cols-1 gap-3 lg:grid-cols-2 xl:grid-cols-5">
        {GROUPS.map((group, groupIndex) => {
          const items = grouped.get(group.label) ?? [];
          return (
            <MotionPanel
              key={group.label}
              className="ap-card min-w-0 rounded p-3"
              style={{ minHeight: 280 }}
              data-testid="workflow-group"
              delayIndex={groupIndex}
            >
              <div className="mb-3 flex items-center justify-between gap-2">
                <div className="flex min-w-0 items-center gap-2">
                  <span
                    className="shrink-0 rounded-full"
                    style={{
                      border: "1px solid var(--affordance)",
                      height: 10,
                      width: 10,
                    }}
                    aria-hidden="true"
                  />
                  <h2
                    className="ap-register-chrome truncate"
                    style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
                  >
                    {group.label}
                  </h2>
                </div>
                <span
                  className="ap-card ap-register-evidence rounded-full px-2 py-0.5"
                  style={{ fontSize: TYPE.scale.xs }}
                >
                  {items.length}
                </span>
              </div>
              <div className="space-y-2">
                {items.length === 0 ? (
                  <div
                    className="ap-hairline ap-soft rounded border px-2 py-3 text-center"
                    style={{ fontSize: TYPE.scale.xs }}
                    data-testid="workflow-group-empty"
                  >
                    No items in this state.
                  </div>
                ) : (
                  items.map((item) => <WorkflowCard key={item.item_id} item={item} />)
                )}
              </div>
            </MotionPanel>
          );
        })}
      </div>
    </MotionSection>
  );
}

function WorkflowRolePosture({
  roleScope,
  workflow,
}: {
  roleScope: RoleScopeSummary | null;
  workflow: ProjectWorkflowResponse;
}) {
  const approvalRows = workflow.items.filter(
    (item) =>
      item.kind === "access_request" &&
      item.approver_id === workflow.actor_id &&
      ["pending", "active"].includes(item.status.toLowerCase()),
  );
  const teamScope = roleScope?.team_scope.has_team_scope ? roleScope.team_scope.direct_report_count : 0;
  const departmentId = roleScope?.department_scope.department_id ?? null;
  const executiveSignal =
    roleScope?.derived_level === "executive_candidate" || roleScope?.derived_level === "super_admin_candidate";

  return (
    <div
      className="ap-card mb-4 grid grid-cols-1 gap-2 rounded border p-3 md:grid-cols-4"
      data-testid="workflow-role-posture"
    >
      <WorkflowPostureFact
        detail="Personal workflow remains primary."
        label="Employee focus"
        value={`${workflow.items.length} rows`}
      />
      {teamScope > 0 && (
        <WorkflowPostureFact
          detail="Team workflow rows are not added unless exposed by the workflow projection."
          label="Team context"
          value={`${teamScope} direct ${teamScope === 1 ? "report" : "reports"}`}
        />
      )}
      {departmentId && roleScope?.derived_level === "department_head" && (
        <WorkflowPostureFact
          detail="Department context is label-only on this project surface."
          label="Department context"
          value={departmentId}
        />
      )}
      {approvalRows.length > 0 && (
        <WorkflowPostureFact
          detail="Approval waiting states are real access-request workflow rows."
          label="Approval waiting"
          value={`${approvalRows.length} rows`}
        />
      )}
      {executiveSignal && (
        <WorkflowPostureFact
          detail="Candidate signal does not unlock restricted admin-domain workflow."
          label="Executive candidate"
          value="label only"
        />
      )}
    </div>
  );
}

function WorkflowPostureFact({
  detail,
  label,
  value,
}: {
  detail: string;
  label: string;
  value: string;
}) {
  return (
    <div className="ap-washable rounded border px-2 py-2" data-testid="workflow-role-posture-fact">
      <div className="flex items-start justify-between gap-2">
        <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
          {label}
        </p>
        <Chip>{value}</Chip>
      </div>
      <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
        {detail}
      </p>
    </div>
  );
}

function WorkflowCard({ item }: { item: WorkflowItem }) {
  const actors = [
    item.owner_id ? ["owner", item.owner_id] : null,
    item.requester_id ? ["requester", item.requester_id] : null,
    item.approver_id ? ["approver", item.approver_id] : null,
    item.agent_id ? ["agent", item.agent_id] : null,
  ].filter((entry): entry is [string, string] => entry !== null);

  return (
    <MotionArticle
      className="ap-card ap-washable rounded p-3"
      data-testid="workflow-item"
      data-status={item.status}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p
            className="ap-register-chrome"
            style={{ fontSize: TYPE.scale.sm, fontWeight: 600, lineHeight: TYPE.line.body }}
            data-testid="workflow-item-title"
          >
            {item.title}
          </p>
          <p
            className="ap-register-evidence ap-soft mt-1 truncate"
            style={{ fontSize: TYPE.scale.xs }}
            data-testid="workflow-item-id"
          >
            {item.item_id}
          </p>
        </div>
        <Chip mono>{item.status}</Chip>
      </div>

      <p
        className="ap-soft mt-2"
        style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
        data-testid="workflow-provenance"
      >
        {item.provenance.strategy.name} / {item.provenance.initiative.name} / {item.provenance.workflow.name}
      </p>

      <div className="mt-3 flex flex-wrap gap-1.5 border-t pt-2" style={{ borderColor: "var(--hairline)" }}>
        <Chip>{KIND_LABEL[item.kind]}</Chip>
        {actors.map(([label, value]) => (
          <Chip key={`${label}:${value}`} mono>
            {label} {value}
          </Chip>
        ))}
      </div>

      {item.dependencies.length > 0 && (
        <div className="mt-2">
          <p className="ap-soft uppercase tracking-wide" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
            Dependencies
          </p>
          <div className="mt-1 flex flex-wrap gap-1.5">
            {item.dependencies.map((dependency) => (
              <Chip key={dependency} mono>
                {dependency}
              </Chip>
            ))}
          </div>
        </div>
      )}
    </MotionArticle>
  );
}
