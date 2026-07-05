"use client";

import { useEffect, useMemo, useState } from "react";
import * as api from "@/lib/api";
import type { GraphProject, GraphResponse, ProjectWorkflowResponse, RoleScopeSummary } from "@/lib/api";
import { TYPE } from "@/lib/tokens";
import { MotionAnchor, MotionArticle, MotionSection } from "./MotionPrimitives";
import { Skeleton } from "./Skeleton";
import { WorkflowView } from "./WorkflowView";

type ProjectTab = "graph" | "workflow";

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

function HeaderValue({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="min-w-0">
      <p className="ap-soft uppercase tracking-wide" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
        {label}
      </p>
      <p className="ap-register-chrome mt-1 truncate" style={{ fontSize: TYPE.scale.sm }}>
        {children}
      </p>
    </div>
  );
}

function ProjectEntryState({
  actor,
  detail,
  testId,
  title,
}: {
  actor: string | null;
  detail: string;
  testId: string;
  title: string;
}) {
  // A2: no hardwired identity — with no actor the links carry no `?as`; the
  // identity picker (the front door) catches identity-less arrivals.
  const carry = actor === null ? "" : `?as=${encodeURIComponent(actor)}`;
  return (
    <main className="min-w-0 flex-1" data-testid={testId}>
      <MotionSection className="ap-hero rounded-2xl p-4">
        <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
          Workflow Command
        </p>
        <h1 className="ap-register-chrome mt-2" style={{ fontSize: TYPE.scale.lg, fontWeight: 600 }}>
          {title}
        </h1>
        <p className="ap-soft mt-2 max-w-2xl" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
          {detail}
        </p>
        <div className="mt-4 flex flex-wrap gap-2">
          <MotionAnchor
            className="ap-affordance-button ap-register-chrome rounded-lg px-3 py-2"
            data-testid="project-empty-work-identity-link"
            href={`/me${carry}`}
            style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
          >
            Open Work Identity
          </MotionAnchor>
          <MotionAnchor
            className="ap-washable ap-register-chrome rounded-lg border px-3 py-2"
            data-testid="project-empty-operating-map-link"
            href={`/admin/graph${carry}`}
            style={{ borderColor: "var(--hairline)", fontSize: TYPE.scale.xs, fontWeight: 600 }}
          >
            Open Operating Map
          </MotionAnchor>
        </div>
      </MotionSection>
    </main>
  );
}

export function ProjectSurface({
  actor,
  capabilityId,
}: {
  actor: string | null;
  capabilityId: string | null;
}) {
  const [tab, setTab] = useState<ProjectTab>("workflow");
  const [workflow, setWorkflow] = useState<ProjectWorkflowResponse | null>(null);
  const [graph, setGraph] = useState<GraphResponse | null>(null);
  const [roleScope, setRoleScope] = useState<RoleScopeSummary | null>(null);
  const [loading, setLoading] = useState(false);
  const [available, setAvailable] = useState(true);

  useEffect(() => {
    if (actor === null || capabilityId === null) {
      setWorkflow(null);
      setGraph(null);
      setRoleScope(null);
      setAvailable(true);
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setAvailable(true);
    Promise.all([api.getProjectWorkflow(actor, capabilityId), api.getGraph(actor), api.getRoleScope(actor)])
      .then(([workflowResponse, graphResponse, roleScopeResponse]) => {
        if (!cancelled) {
          setWorkflow(workflowResponse);
          setGraph(graphResponse);
          setRoleScope(roleScopeResponse);
          setAvailable(workflowResponse !== null);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setWorkflow(null);
          setGraph(null);
          setRoleScope(null);
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
  }, [actor, capabilityId]);

  const project = useMemo(
    () => graph?.projects.find((candidate) => candidate.id === capabilityId) ?? null,
    [graph, capabilityId],
  );

  if (actor === null) {
    return (
      <ProjectEntryState
        actor={actor}
        detail="Choose a Work Identity first. Then open a real capability-backed workflow from that identity's work list or the Operating Map."
        testId="project-empty"
        title="Choose a Work Identity to review work."
      />
    );
  }

  if (capabilityId === null) {
    return (
      <ProjectEntryState
        actor={actor}
        detail="Workflow Command opens when a real capability is selected from Work Identity or the Operating Map. This page does not fabricate project state."
        testId="project-missing-capability"
        title="Choose real work before opening Workflow Command."
      />
    );
  }

  const title =
    workflow?.provenance.capability.name ??
    project?.label.replace(/^Capability:\s*/i, "") ??
    capabilityId;
  const provenance = workflow?.provenance;

  return (
    <main className="min-w-0 flex-1" data-testid="project-surface">
      <header className="mb-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
              {capabilityId}
            </p>
            <h1
              className="ap-register-chrome mt-1"
              style={{ fontSize: TYPE.scale.lg, lineHeight: TYPE.line.display, fontWeight: 600 }}
              data-testid="project-title"
            >
              {title}
            </h1>
          </div>
          <div className="ap-card flex shrink-0 gap-1 rounded-full p-1" data-testid="project-tabs">
            <TabButton active={tab === "graph"} onClick={() => setTab("graph")}>
              Operating Map Trace
            </TabButton>
            <TabButton active={tab === "workflow"} onClick={() => setTab("workflow")}>
              Workflow Command
            </TabButton>
          </div>
        </div>

        {provenance && (
          <div className="mt-3 grid grid-cols-1 gap-2 md:grid-cols-3">
            <HeaderValue label="Strategy">{provenance.strategy.name}</HeaderValue>
            <HeaderValue label="Initiative">{provenance.initiative.name}</HeaderValue>
            <HeaderValue label="Workflow">{provenance.workflow.name}</HeaderValue>
          </div>
        )}
      </header>

      {loading && (
        <div className="ap-card rounded-lg p-4" data-testid="project-loading">
          <Skeleton lines={5} />
        </div>
      )}

      {!loading && !available && (
        <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="project-unavailable">
          This project workflow is not available for the selected Work Identity.
        </p>
      )}

      {!loading && available && tab === "workflow" && (
        <WorkflowView workflow={workflow} loading={false} available={available} roleScope={roleScope} />
      )}

      {!loading && available && tab === "graph" && (
        <ProjectTraceView graph={graph} project={project} workflow={workflow} />
      )}
    </main>
  );
}

function TabButton({
  active,
  children,
  onClick,
}: {
  active: boolean;
  children: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`${active ? "ap-affordance-button" : "ap-washable ap-soft"} rounded-lg px-2 py-1`}
      style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
      aria-pressed={active}
      data-testid="project-tab"
    >
      {children}
    </button>
  );
}

function ProjectTraceView({
  graph,
  project,
  workflow,
}: {
  graph: GraphResponse | null;
  project: GraphProject | null;
  workflow: ProjectWorkflowResponse | null;
}) {
  if (graph === null || project === null) {
    return (
      <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="project-graph-unavailable">
        Graph trace is not available for this project.
      </p>
    );
  }

  const relatedEdges = graph.edges.filter((edge) => edge.from === project.id || edge.to === project.id);
  const itemCount = workflow?.items.length ?? 0;

  return (
    <section className="grid grid-cols-1 gap-3 lg:grid-cols-[1fr_1fr]" data-testid="project-graph-view">
      <MotionArticle className="ap-card rounded-2xl p-4">
        <h2 className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
          Project Trace
        </h2>
        <div className="mt-3 flex flex-wrap gap-1.5">
          <Chip mono>{project.id}</Chip>
          <Chip>{project.people} people</Chip>
          <Chip>{project.departments.length} departments</Chip>
          <Chip>{relatedEdges.length} edges</Chip>
          <Chip>{itemCount} workflow items</Chip>
        </div>
        <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
          {project.workflow_name}
        </p>
        <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
          {project.initiative_name} / {project.strategy_name}
        </p>
      </MotionArticle>

      <MotionArticle className="ap-card rounded-2xl p-4" delayIndex={1}>
        <h2 className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
          Departments
        </h2>
        <div className="mt-3 flex flex-wrap gap-1.5">
          {project.departments.map((department) => (
            <Chip key={department}>{department}</Chip>
          ))}
        </div>
        <h3
          className="ap-soft mt-4 uppercase tracking-wide"
          style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
        >
          Status Mix
        </h3>
        <div className="mt-2 flex flex-wrap gap-1.5">
          {Object.entries(project.status_counts).map(([status, count]) => (
            <Chip key={status}>
              {status}: {count}
            </Chip>
          ))}
        </div>
      </MotionArticle>
    </section>
  );
}
