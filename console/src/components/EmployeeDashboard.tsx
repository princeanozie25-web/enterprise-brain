"use client";

import { useEffect, useMemo, useState } from "react";
import * as api from "@/lib/api";
import type {
  AccessRequestRecord,
  GraphProject,
  GraphResponse,
  LensResponse,
  NodeSummary,
  ProjectRecord,
  ProjectWorkflowResponse,
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

export function EmployeeDashboard({ actor }: { actor: string | null }) {
  const [lens, setLens] = useState<LensResponse | null>(null);
  const [graph, setGraph] = useState<GraphResponse | null>(null);
  const [summary, setSummary] = useState<NodeSummary | null>(null);
  const [requests, setRequests] = useState<AccessRequestRecord[]>([]);
  const [inbox, setInbox] = useState<AccessRequestRecord[]>([]);
  const [workflows, setWorkflows] = useState<ProjectWorkflowResponse[]>([]);
  const [loading, setLoading] = useState(false);
  const [available, setAvailable] = useState(true);

  useEffect(() => {
    if (actor === null) {
      setLens(null);
      setGraph(null);
      setSummary(null);
      setRequests([]);
      setInbox([]);
      setWorkflows([]);
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
      api.getAccessRequests(actor),
      api.getAccessRequestInbox(actor),
    ])
      .then(async ([lensResponse, graphResponse, summaryResponse, requestResponse, inboxResponse]) => {
        if (cancelled) return;
        setLens(lensResponse);
        setGraph(graphResponse);
        setSummary(summaryResponse);
        setRequests(requestResponse?.requests ?? []);
        setInbox(inboxResponse?.requests ?? []);
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
          setRequests([]);
          setInbox([]);
          setWorkflows([]);
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
        Select a lens to open your dashboard.
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
        Your dashboard is not available through this lens.
      </p>
    );
  }

  const human = lens.subject_human;
  const projects = human?.projects ?? [];
  const workflowItems = workflows.flatMap((workflow) => workflow.items);
  const knowledgeSections = lens.holdings.length;
  const visibleKnowledgeRows = lens.holdings.reduce((sum, section) => sum + section.docs.length, 0);

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
          <a
            href={`/ask?as=${encodeURIComponent(actor)}`}
            className="ap-affordance-button ap-register-chrome rounded px-3 py-2"
            style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
            data-testid="dashboard-ask-link"
          >
            Ask Enterprise Brain
          </a>
        </div>
      </header>

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-[1.15fr_0.85fr]">
        <div className="space-y-4">
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
            action={<span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>{requests.length + inbox.length}</span>}
          >
            <RequestsList requests={requests} inbox={inbox} projectById={projectById} />
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
    <div className="grid grid-cols-1 gap-2 lg:grid-cols-5" data-testid="dashboard-workflow">
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
  inbox,
  projectById,
  requests,
}: {
  inbox: AccessRequestRecord[];
  projectById: Map<string, GraphProject>;
  requests: AccessRequestRecord[];
}) {
  const rows = [
    ...requests.map((request) => ({ label: "Mine", request })),
    ...inbox.map((request) => ({ label: "Approval", request })),
  ];
  if (rows.length === 0) {
    return <EmptyLine>No access requests are active for this actor.</EmptyLine>;
  }
  return (
    <div className="space-y-2" data-testid="dashboard-requests">
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
