"use client";

import { useEffect, useMemo, useState } from "react";
import * as api from "@/lib/api";
import type { GraphProject, GraphResponse, ProjectWorkflowResponse } from "@/lib/api";
import { TYPE } from "@/lib/tokens";
import { Skeleton } from "./Skeleton";
import { WorkflowView } from "./WorkflowView";

type ProjectTab = "graph" | "workflow";

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
  const [loading, setLoading] = useState(false);
  const [available, setAvailable] = useState(true);

  useEffect(() => {
    if (actor === null || capabilityId === null) {
      setWorkflow(null);
      setGraph(null);
      setAvailable(true);
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setAvailable(true);
    Promise.all([api.getProjectWorkflow(actor, capabilityId), api.getGraph(actor)])
      .then(([workflowResponse, graphResponse]) => {
        if (!cancelled) {
          setWorkflow(workflowResponse);
          setGraph(graphResponse);
          setAvailable(workflowResponse !== null);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setWorkflow(null);
          setGraph(null);
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
      <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="project-empty">
        Select a lens to begin.
      </p>
    );
  }

  if (capabilityId === null) {
    return (
      <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="project-missing-capability">
        Open a project from the graph to view its workflow.
      </p>
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
          <div className="ap-card flex shrink-0 gap-1 rounded p-1" data-testid="project-tabs">
            <TabButton active={tab === "graph"} onClick={() => setTab("graph")}>
              Graph View
            </TabButton>
            <TabButton active={tab === "workflow"} onClick={() => setTab("workflow")}>
              Workflow View
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
        <div className="ap-card rounded p-4" data-testid="project-loading">
          <Skeleton lines={5} />
        </div>
      )}

      {!loading && !available && (
        <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="project-unavailable">
          This project workflow is not available through the current lens.
        </p>
      )}

      {!loading && available && tab === "workflow" && (
        <WorkflowView workflow={workflow} loading={false} available={available} />
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
      className={`${active ? "ap-affordance-button" : "ap-washable ap-soft"} rounded px-2 py-1`}
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
      <div className="ap-card rounded p-3">
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
      </div>

      <div className="ap-card rounded p-3">
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
      </div>
    </section>
  );
}
