"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { select } from "d3-selection";
import { zoom as d3zoom, zoomIdentity, type ZoomBehavior, type ZoomTransform } from "d3-zoom";
import type { GraphEdge, GraphPerson, GraphProject, GraphResponse } from "@/lib/api";
import { FONT, GEOMETRY, TYPE, graphRampStep } from "@/lib/tokens";
import { PersonAvatar } from "./PersonAvatar";

type Pos = { x: number; y: number };
export type GraphNodeKind = "org" | "department" | "human" | "agent" | "source" | "project";
export type SelectedNode = { id: string; kind: GraphNodeKind; label: string };

type DeptArc = { start: number; end: number; center: number; count: number };
type Layout = {
  pos: Map<string, Pos>;
  ang: Map<string, number>;
  hubAngle: Map<string, number>;
  deptArcs: Map<string, DeptArc>;
};

/**
 * THE RING LAW (comprehension pass, B1):
 * - Every rendered node maps to a REAL entity in the scoped /graph payload —
 *   zero decorative nodes, zero synthetic satellites (T-B8 enforces it).
 * - A person is a 40px circle (initial-pair or face) with the FULL NAME
 *   always beneath at 13px/500 — no monogram caterpillar, no LOD hiding.
 * - Departments are coded ONLY by the sensitivity-safe NEUTRAL RAMP
 *   (tokens.GRAPH_NEUTRAL_RAMP); saturated color stays reserved. Amber marks
 *   the LIT CONNECTION PATH and nothing else; selection rings are the
 *   interactive ink-blue.
 * - Edges render ONLY from real payload relationships: 1.5px, ≥60% opacity
 *   at rest (interaction dimming is transient emphasis, not the base style).
 * - Past 40 in-scope people the ring collapses to per-department cluster
 *   chips (department name + in-scope count — counts come straight from the
 *   payload; dark counts stay banned).
 * - Fully keyboard-operable (tab / Enter / Space / arrows / Escape) with a
 *   visually-hidden list mirror for screen readers (WCAG 2.1.1, 4.1.2).
 */
const STAGE = {
  width: 1400,
  height: 980,
  cx: 700,
  cy: 520,
  ringDept: 205,
  ringAgents: 258,
  ringProjects: 334,
  ringSources: 444,
  ringPeople: 575,
  ringAccess: 521,
  hubRadius: 23,
  coreRadius: 32,
  sourceRadius: 8,
  agentSize: 7,
  projectSize: 9,
  /** B1: one person size — a 40px circle, every ring. */
  personNode: 40,
  /** Cluster chips (>40 people): a quiet rounded-lg chip per department. */
  clusterWidth: 148,
  clusterHeight: 44,
} as const;

/** The people ring collapses to department clusters past this count. */
export const PEOPLE_CLUSTER_THRESHOLD = 40;

const C = {
  ink: "var(--ink)",
  paper: "var(--paper)",
  inkSoft: "var(--ink-soft)",
  affordance: "var(--affordance)",
  warm: "var(--accent-warm)",
  hairline: "var(--hairline)",
  wash: "var(--wash)",
};

function polar(angle: number, radius: number, cx = STAGE.cx, cy = STAGE.cy): Pos {
  return {
    x: cx + radius * Math.cos(angle),
    y: cy + radius * Math.sin(angle),
  };
}

function shortMark(label: string): string {
  const words = label
    .replace(/&/g, " ")
    .split(/\s+/)
    .filter(Boolean);
  if (words.length === 0) return "?";
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[words.length - 1][0]).toUpperCase();
}

function monogram(name: string): string {
  return shortMark(name).slice(0, 2);
}

function compactProjectLabel(label: string): string {
  const clean = label.replace(/^Capability:\s*/i, "");
  return clean.length > 34 ? `${clean.slice(0, 31)}...` : clean;
}

function computeLayout(graph: GraphResponse): Layout {
  const pos = new Map<string, Pos>();
  const ang = new Map<string, number>();
  const hubAngle = new Map<string, number>();
  const deptArcs = new Map<string, DeptArc>();
  pos.set(graph.center.id, { x: STAGE.cx, y: STAGE.cy });

  const peopleByDept = new Map<string, GraphPerson[]>();
  for (const person of graph.people) {
    const list = peopleByDept.get(person.department_id) ?? [];
    list.push(person);
    peopleByDept.set(person.department_id, list);
  }

  const total = Math.max(graph.people.length, 1);
  const gap = GEOMETRY.graphArcGap;
  const usable = 2 * Math.PI - graph.departments.length * gap;
  let cursor = -Math.PI * 0.94;

  for (const dept of graph.departments) {
    const list = [...(peopleByDept.get(dept.id) ?? [])].sort((a, b) =>
      a.ring === b.ring ? a.id.localeCompare(b.id) : a.ring === "anchor" ? -1 : 1,
    );
    const span = usable * (Math.max(list.length, 1) / total);
    const start = cursor + gap / 2;
    const center = start + span / 2;
    const end = start + span;
    hubAngle.set(dept.id, center);
    deptArcs.set(dept.id, { start, center, end, count: list.length });
    pos.set(dept.id, polar(center, STAGE.ringDept));

    list.forEach((person, index) => {
      // Deterministic, jitter-free ring placement: 40px nodes with always-on
      // names need honest, even spacing, not organic scatter.
      const frac = list.length <= 1 ? 0.5 : index / (list.length - 1);
      const angle = start + frac * span;
      pos.set(person.id, polar(angle, STAGE.ringPeople));
      ang.set(person.id, angle);
    });
    cursor = end + gap / 2;
  }

  const agentsSeen = new Map<string, number>();
  graph.tools.forEach((tool, index) => {
    const base = tool.department_id ? hubAngle.get(tool.department_id) : undefined;
    const seen = tool.department_id ? agentsSeen.get(tool.department_id) ?? 0 : index;
    if (tool.department_id) agentsSeen.set(tool.department_id, seen + 1);
    const angle =
      base !== undefined
        ? base + (seen % 2 === 0 ? 1 : -1) * 0.11 * Math.ceil((seen + 1) / 2)
        : -Math.PI / 2 + (index / Math.max(graph.tools.length, 1)) * 2 * Math.PI;
    pos.set(tool.id, polar(angle, STAGE.ringAgents));
    ang.set(tool.id, angle);
  });

  const projectsSeen = new Map<string, number>();
  graph.projects.forEach((project, index) => {
    const base = hubAngle.get(project.primary_department_id);
    const seen = projectsSeen.get(project.primary_department_id) ?? 0;
    projectsSeen.set(project.primary_department_id, seen + 1);
    const angle =
      base !== undefined
        ? base + (seen % 2 === 0 ? 1 : -1) * 0.066 * Math.ceil((seen + 1) / 2)
        : -Math.PI / 2 + (index / Math.max(graph.projects.length, 1)) * 2 * Math.PI;
    pos.set(project.id, polar(angle, STAGE.ringProjects));
    ang.set(project.id, angle);
  });

  graph.sources.forEach((source, index) => {
    const angle = -Math.PI / 2 + (index / Math.max(graph.sources.length, 1)) * 2 * Math.PI;
    pos.set(source.id, polar(angle, STAGE.ringSources));
    ang.set(source.id, angle);
  });

  return { pos, ang, hubAngle, deptArcs };
}

/** B1: one edge weight — 1.5px at ≥60% opacity from the real payload. Kind
 * is carried by dash pattern, never by extra hue. */
const EDGE_BASE = { width: 1.5, opacity: 0.6 } as const;
const EDGE_DASH: Record<string, string | undefined> = {
  reports_to: undefined,
  member_of: undefined,
  owns_agent: "2 5",
  system_of: "1 8",
  works_on: "1 7",
  involves_department: "2 7",
  uses: "1 8",
};

function edgePath(from: Pos, to: Pos, curve: number): string {
  const mx = (from.x + to.x) / 2;
  const my = (from.y + to.y) / 2;
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const len = Math.hypot(dx, dy) || 1;
  return `M${from.x},${from.y}Q${mx + (-dy / len) * curve},${my + (dx / len) * curve} ${to.x},${to.y}`;
}

function arcPath(radius: number, start: number, end: number): string {
  const a0 = polar(start, radius);
  const a1 = polar(end, radius);
  const large = end - start > Math.PI ? 1 : 0;
  return `M${a0.x},${a0.y}A${radius},${radius} 0 ${large} 1 ${a1.x},${a1.y}`;
}

export function OrgGraph({
  graph,
  onSelectNode,
  onFocusDept,
  selectedId = null,
  focusDept = null,
  query = "",
  hiddenKinds = [],
  reducedMotion = false,
}: {
  graph: GraphResponse;
  onSelectNode: (node: SelectedNode | null) => void;
  onFocusDept: (deptId: string | null) => void;
  selectedId?: string | null;
  focusDept?: string | null;
  query?: string;
  hiddenKinds?: string[];
  reducedMotion?: boolean;
}) {
  const { pos, ang, hubAngle, deptArcs } = useMemo(() => computeLayout(graph), [graph]);
  const [hover, setHover] = useState<string | null>(null);
  const [transform, setTransform] = useState<ZoomTransform>(zoomIdentity);
  const svgRef = useRef<SVGSVGElement | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const zoomRef = useRef<ZoomBehavior<SVGSVGElement, unknown> | null>(null);
  const nodeRefs = useRef<Map<string, SVGGElement>>(new Map());

  const clustered = graph.people.length > PEOPLE_CLUSTER_THRESHOLD;

  const peopleById = useMemo(() => new Map(graph.people.map((p) => [p.id, p])), [graph.people]);
  const toolsById = useMemo(() => new Map(graph.tools.map((t) => [t.id, t])), [graph.tools]);
  const sourcesById = useMemo(() => new Map(graph.sources.map((s) => [s.id, s])), [graph.sources]);
  const projectsById = useMemo(() => new Map(graph.projects.map((p) => [p.id, p])), [graph.projects]);
  const deptById = useMemo(() => new Map(graph.departments.map((d) => [d.id, d])), [graph.departments]);
  const deptIndex = useMemo(
    () => new Map(graph.departments.map((d, index) => [d.id, index])),
    [graph.departments],
  );
  const rampOf = (deptId: string | null | undefined) =>
    graphRampStep(deptId != null ? deptIndex.get(deptId) ?? 0 : 0);

  const neighbors = useMemo(() => {
    const map = new Map<string, Set<string>>();
    const link = (a: string, b: string) => {
      (map.get(a) ?? map.set(a, new Set()).get(a)!).add(b);
      (map.get(b) ?? map.set(b, new Set()).get(b)!).add(a);
    };
    for (const edge of graph.edges) link(edge.from, edge.to);
    return map;
  }, [graph.edges]);

  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    try {
      const behavior = d3zoom<SVGSVGElement, unknown>()
        .scaleExtent([GEOMETRY.graphScaleMin, GEOMETRY.graphScaleMax])
        .on("zoom", (event) => setTransform(event.transform));
      zoomRef.current = behavior;
      const sel = select(svg);
      sel.call(behavior);
      return () => {
        sel.on(".zoom", null);
        zoomRef.current = null;
      };
    } catch {
      return;
    }
  }, []);

  useEffect(() => {
    const svg = svgRef.current;
    if (!svg || !zoomRef.current) return;
    try {
      if (focusDept !== null) {
        const angle = hubAngle.get(focusDept);
        if (angle === undefined) return;
        const focusK = 1.9;
        const frame = polar(angle, STAGE.ringPeople * 0.76);
        const t = zoomIdentity
          .translate(STAGE.width / 2 - focusK * frame.x, STAGE.height / 2 - focusK * frame.y)
          .scale(focusK);
        select(svg).call(zoomRef.current.transform, t);
      } else {
        select(svg).call(zoomRef.current.transform, zoomIdentity);
      }
    } catch {
      /* framing is an enhancement */
    }
  }, [focusDept, hubAngle]);

  const hidden = useMemo(() => new Set(hiddenKinds), [hiddenKinds]);
  const q = query.trim().toLowerCase();
  const k = transform.k || 1;
  const lab = (px: number) => px / k;
  const at = (id: string): Pos => pos.get(id) ?? { x: STAGE.cx, y: STAGE.cy };

  const matches = (id: string): boolean => {
    if (q.length === 0) return false;
    const person = peopleById.get(id);
    if (person) {
      return (
        person.display_name.toLowerCase().includes(q) ||
        person.title.toLowerCase().includes(q) ||
        person.department_id.toLowerCase().includes(q) ||
        person.id.toLowerCase().includes(q)
      );
    }
    const tool = toolsById.get(id);
    if (tool) return tool.label.toLowerCase().includes(q) || tool.id.toLowerCase().includes(q);
    const source = sourcesById.get(id);
    if (source) return source.label.toLowerCase().includes(q) || source.id.toLowerCase().includes(q);
    const project = projectsById.get(id);
    if (project) {
      return [
        project.id,
        project.label,
        project.workflow_name,
        project.initiative_name,
        project.strategy_name,
        ...project.departments,
        ...Object.keys(project.status_counts),
      ].some((value) => value.toLowerCase().includes(q));
    }
    const dept = deptById.get(id);
    if (dept) return dept.label.toLowerCase().includes(q) || dept.id.toLowerCase().includes(q);
    return id.toLowerCase().includes(q);
  };

  const inDept = (id: string): boolean =>
    id === focusDept ||
    peopleById.get(id)?.department_id === focusDept ||
    toolsById.get(id)?.department_id === focusDept ||
    (focusDept !== null && (projectsById.get(id)?.departments.includes(focusDept) ?? false));

  const selectedIsCenter = selectedId === graph.center.id;
  const selectedNeighbors = selectedId !== null ? neighbors.get(selectedId) : undefined;
  const traceRelated = (id: string): boolean =>
    selectedId !== null && !selectedIsCenter && (selectedNeighbors?.has(id) ?? false);
  const dimming = focusDept !== null || hover !== null || (selectedId !== null && !selectedIsCenter) || q.length > 0;
  const emphasized = (id: string): boolean => {
    if (focusDept !== null) return inDept(id) || id === graph.center.id;
    if (hover !== null) return id === hover || (neighbors.get(hover)?.has(id) ?? false);
    if (selectedId !== null) return selectedIsCenter || id === selectedId || traceRelated(id);
    if (q.length > 0) return matches(id);
    return true;
  };
  const op = (id: string): number => {
    if (!dimming || emphasized(id)) return 1;
    return focusDept !== null ? GEOMETRY.graphGhostOpacity : GEOMETRY.graphDimOpacity;
  };

  const projectVisible = (project: GraphProject): boolean => {
    if (hidden.has("projects")) return false;
    if (selectedId === project.id || hover === project.id || matches(project.id) || traceRelated(project.id)) return true;
    if (hover !== null && (neighbors.get(hover)?.has(project.id) ?? false)) return true;
    return focusDept !== null && project.departments.includes(focusDept);
  };

  const edgeTouchesFocus = (edge: GraphEdge): boolean => {
    if (focusDept !== null) return inDept(edge.from) || inDept(edge.to);
    if (hover !== null) return edge.from === hover || edge.to === hover;
    if (selectedId !== null && !selectedIsCenter) return edge.from === selectedId || edge.to === selectedId;
    return false;
  };

  const reset = () => {
    setHover(null);
    onFocusDept(null);
    onSelectNode(null);
    setTransform(zoomIdentity);
    const svg = svgRef.current;
    if (svg && zoomRef.current) {
      try {
        select(svg).call(zoomRef.current.transform, zoomIdentity);
      } catch {
        /* state already reset */
      }
    }
  };

  const edgeHidden = (edge: GraphEdge): boolean =>
    (clustered && (peopleById.has(edge.from) || peopleById.has(edge.to))) ||
    (hidden.has("people") && (peopleById.has(edge.from) || peopleById.has(edge.to))) ||
    (hidden.has("agents") && (toolsById.has(edge.from) || toolsById.has(edge.to))) ||
    (hidden.has("sources") && (sourcesById.has(edge.from) || sourcesById.has(edge.to))) ||
    (projectsById.has(edge.from) && !projectVisible(projectsById.get(edge.from)!)) ||
    (projectsById.has(edge.to) && !projectVisible(projectsById.get(edge.to)!));

  // -------------------------------------------------------------------------
  // KEYBOARD OPERABILITY (B1 / WCAG 2.1.1): every rendered node is a real tab
  // stop. Enter/Space activates (opens the drawer / focuses the department),
  // arrow keys traverse the ring order, Escape returns focus to the graph
  // root. Focus mirrors hover so the emphasis visuals double as the focus cue
  // on top of the standard focus ring.
  // -------------------------------------------------------------------------
  const visibleProjects = graph.projects.filter(projectVisible);
  const keyOrder: string[] = useMemo(() => {
    const order: string[] = [graph.center.id];
    for (const dept of graph.departments) order.push(dept.id);
    if (!hidden.has("agents")) for (const tool of graph.tools) order.push(tool.id);
    if (!hidden.has("sources")) for (const source of graph.sources) order.push(source.id);
    for (const project of visibleProjects) order.push(project.id);
    if (clustered) {
      for (const dept of graph.departments) order.push(`cluster:${dept.id}`);
    } else if (!hidden.has("people")) {
      for (const person of graph.people) order.push(person.id);
    }
    return order;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [graph, hidden, clustered, visibleProjects.length]);

  const registerNode = (id: string) => (el: SVGGElement | null) => {
    if (el) nodeRefs.current.set(id, el);
    else nodeRefs.current.delete(id);
  };

  const moveFocus = (fromId: string, delta: 1 | -1) => {
    const index = keyOrder.indexOf(fromId);
    if (index === -1) return;
    const next = keyOrder[(index + delta + keyOrder.length) % keyOrder.length];
    nodeRefs.current.get(next)?.focus();
  };

  const nodeKeyProps = (id: string, ariaLabel: string, activate: () => void) => ({
    ref: registerNode(id),
    tabIndex: 0,
    role: "button" as const,
    "aria-label": ariaLabel,
    onFocus: () => setHover(id),
    onBlur: () => setHover(null),
    onKeyDown: (event: React.KeyboardEvent<SVGGElement>) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        activate();
      } else if (event.key === "ArrowRight" || event.key === "ArrowDown") {
        event.preventDefault();
        moveFocus(id, 1);
      } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
        event.preventDefault();
        moveFocus(id, -1);
      } else if (event.key === "Escape") {
        event.preventDefault();
        rootRef.current?.focus();
      }
    },
  });

  return (
    <div className="relative h-full min-h-0" data-testid="org-graph" ref={rootRef} tabIndex={-1}>
      <button
        type="button"
        onClick={reset}
        className="ap-card ap-washable absolute right-3 top-3 z-10 grid rounded-full"
        style={{ width: 34, height: 34, placeItems: "center" }}
        data-testid="graph-reset"
        aria-label="Fit graph"
        title="Fit graph"
      >
        <svg width={15} height={15} viewBox="0 0 24 24" aria-hidden="true">
          <path
            d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5"
            fill="none"
            stroke={C.ink}
            strokeWidth={2}
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>

      {/* The visually-hidden list mirror (B1 / WCAG 4.1.2): the same rendered
          nodes as plain text for screen readers — real entities only. */}
      <ul className="sr-only" data-testid="graph-sr-mirror">
        <li>{graph.center.label} — organization</li>
        {graph.departments.map((dept) => (
          <li key={dept.id}>
            {dept.label} — department — {deptArcs.get(dept.id)?.count ?? 0} people in scope
          </li>
        ))}
        {!hidden.has("agents") &&
          graph.tools.map((tool) => <li key={tool.id}>{tool.label} — agent</li>)}
        {!hidden.has("sources") &&
          graph.sources.map((source) => <li key={source.id}>{source.label} — system of record</li>)}
        {visibleProjects.map((project) => (
          <li key={project.id}>
            {compactProjectLabel(project.label)} — project — {project.people} people
          </li>
        ))}
        {!clustered &&
          !hidden.has("people") &&
          graph.people.map((person) => (
            <li key={person.id}>
              {person.display_name} — {person.title} — {deptById.get(person.department_id)?.label ?? person.department_id}
            </li>
          ))}
      </ul>

      <svg
        ref={svgRef}
        viewBox={`0 0 ${STAGE.width} ${STAGE.height}`}
        className={`block h-full w-full${reducedMotion ? "" : " ap-fade-view"}`}
        style={{ touchAction: "none", cursor: "grab" }}
        role="group"
        aria-label="Organization graph"
      >
        <defs>
          <filter id="graph-soft-glow" x="-35%" y="-35%" width="170%" height="170%">
            <feGaussianBlur stdDeviation="8" />
          </filter>
        </defs>
        <g transform={transform.toString()} data-testid="graph-scene">
          <g data-testid="graph-edges">
            {graph.edges.map((edge, index) => {
              if (edgeHidden(edge)) return null;
              const from = at(edge.from);
              const to = at(edge.to);
              const lit = edgeTouchesFocus(edge);
              const faded = dimming && !lit;
              const dx = to.x - from.x;
              const dy = to.y - from.y;
              const curve = (edge.from < edge.to ? 1 : -1) * Math.min(Math.hypot(dx, dy) * 0.08, 44);
              return (
                <path
                  key={`${edge.from}-${edge.kind}-${edge.to}-${index}`}
                  d={edgePath(from, to, curve)}
                  fill="none"
                  stroke={lit ? C.warm : C.inkSoft}
                  strokeWidth={lit ? 2 : EDGE_BASE.width}
                  strokeOpacity={faded ? 0.15 : lit ? 0.85 : EDGE_BASE.opacity}
                  strokeDasharray={EDGE_DASH[edge.kind]}
                  strokeLinecap="round"
                  data-testid="graph-edge"
                  data-kind={edge.kind}
                />
              );
            })}
          </g>

          <g data-testid="graph-rings" opacity={focusDept !== null ? 0.42 : 1}>
            {[STAGE.ringPeople, STAGE.ringSources, STAGE.ringProjects, STAGE.ringAgents].map((radius) => (
              <circle
                key={radius}
                cx={STAGE.cx}
                cy={STAGE.cy}
                r={radius}
                fill="none"
                stroke={C.hairline}
                strokeWidth={1}
                strokeDasharray="1 10"
                strokeLinecap="round"
                strokeOpacity={0.72}
              />
            ))}
          </g>

          <g data-testid="graph-dept-arcs">
            {graph.departments.map((dept) => {
              const arc = deptArcs.get(dept.id);
              if (!arc) return null;
              const ramp = rampOf(dept.id);
              return (
                <path
                  key={dept.id}
                  d={arcPath(STAGE.ringAccess, arc.start, arc.end)}
                  fill="none"
                  stroke={ramp.line}
                  strokeWidth={7}
                  strokeLinecap="round"
                  strokeOpacity={focusDept === null || focusDept === dept.id ? 0.48 : 0.07}
                  data-testid="graph-access-arc"
                  data-dept={dept.id}
                />
              );
            })}
          </g>

          {graph.departments.map((dept) => {
            const point = at(dept.id);
            const ramp = rampOf(dept.id);
            const count = deptArcs.get(dept.id)?.count ?? 0;
            const active =
              focusDept === dept.id || selectedId === dept.id || traceRelated(dept.id) || hover === dept.id || matches(dept.id);
            const activate = () => {
              onFocusDept(focusDept === dept.id ? null : dept.id);
              onSelectNode({ id: dept.id, kind: "department", label: dept.label });
            };
            return (
              <g
                key={dept.id}
                transform={`translate(${point.x},${point.y})`}
                opacity={op(dept.id)}
                style={{ cursor: "pointer" }}
                onMouseEnter={() => setHover(dept.id)}
                onMouseLeave={() => setHover(null)}
                onClick={activate}
                data-testid="graph-dept"
                data-id={dept.id}
                data-dept={dept.id}
                {...nodeKeyProps(dept.id, `${dept.label} department, ${count} people in scope`, activate)}
              >
                <title>{`${dept.label} department`}</title>
                <circle r={STAGE.hubRadius + 10} fill={ramp.line} opacity={0.16} filter="url(#graph-soft-glow)" />
                <circle r={STAGE.hubRadius} fill={ramp.surface} stroke={ramp.line} strokeWidth={1.6} />
                <path
                  d="M-7 4h14M-4 4v-7h8v7M-10 8h20v5h-20z"
                  fill="none"
                  stroke={C.ink}
                  strokeWidth={2}
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  opacity={0.86}
                />
                <circle
                  r={STAGE.hubRadius + 7}
                  fill="none"
                  stroke={active ? C.affordance : C.hairline}
                  strokeWidth={active ? 2 : 1}
                  strokeOpacity={active ? 0.9 : 0}
                />
                <text
                  y={STAGE.hubRadius + lab(18)}
                  textAnchor="middle"
                  fill={C.ink}
                  paintOrder="stroke"
                  stroke={C.paper}
                  strokeWidth={GEOMETRY.graphLabelHalo}
                  style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs), fontWeight: 700 }}
                  data-testid="graph-dept-label"
                >
                  {dept.label}
                </text>
                <text
                  y={STAGE.hubRadius + lab(31)}
                  textAnchor="middle"
                  fill={C.inkSoft}
                  paintOrder="stroke"
                  stroke={C.paper}
                  strokeWidth={GEOMETRY.graphLabelHalo}
                  style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 3), fontWeight: 500 }}
                >
                  {`${count} people`}
                </text>
              </g>
            );
          })}

          {!hidden.has("agents") &&
            graph.tools.map((tool) => {
              const point = at(tool.id);
              const ramp = rampOf(tool.department_id);
              const active = selectedId === tool.id || traceRelated(tool.id) || hover === tool.id || matches(tool.id);
              const size = active || focusDept === tool.department_id ? STAGE.agentSize + 2 : STAGE.agentSize;
              const activate = () => onSelectNode({ id: tool.id, kind: "agent", label: tool.label });
              return (
                <g
                  key={tool.id}
                  transform={`translate(${point.x},${point.y})`}
                  opacity={op(tool.id)}
                  style={{ cursor: "pointer" }}
                  onMouseEnter={() => setHover(tool.id)}
                  onMouseLeave={() => setHover(null)}
                  onClick={activate}
                  data-testid="graph-tool"
                  data-id={tool.id}
                  data-kind={tool.kind}
                  {...nodeKeyProps(tool.id, `${tool.label}, agent`, activate)}
                >
                  <title>{tool.label}</title>
                  <rect
                    x={-size}
                    y={-size}
                    width={size * 2}
                    height={size * 2}
                    rx={3}
                    fill={ramp.line}
                    stroke={C.paper}
                    strokeWidth={1}
                    strokeOpacity={0.42}
                  />
                  <text
                    textAnchor="middle"
                    dominantBaseline="central"
                    fill={C.paper}
                    style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 3), fontWeight: 800 }}
                  >
                    A
                  </text>
                  <text
                    y={size + lab(13)}
                    textAnchor="middle"
                    fill={C.inkSoft}
                    paintOrder="stroke"
                    stroke={C.paper}
                    strokeWidth={GEOMETRY.graphLabelHalo}
                    style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 2) }}
                    data-testid="graph-tool-label"
                  >
                    {tool.label}
                  </text>
                  <circle
                    r={size + 5}
                    fill="none"
                    stroke={C.affordance}
                    strokeWidth={active ? 2 : 0}
                    strokeOpacity={active ? 0.86 : 0}
                  />
                </g>
              );
            })}

          {!hidden.has("sources") &&
            graph.sources.map((source) => {
              const point = at(source.id);
              const active = selectedId === source.id || traceRelated(source.id) || hover === source.id || matches(source.id);
              const activate = () => onSelectNode({ id: source.id, kind: "source", label: source.label });
              return (
                <g
                  key={source.id}
                  transform={`translate(${point.x},${point.y})`}
                  opacity={op(source.id)}
                  style={{ cursor: "pointer" }}
                  onMouseEnter={() => setHover(source.id)}
                  onMouseLeave={() => setHover(null)}
                  onClick={activate}
                  data-testid="graph-source"
                  data-id={source.id}
                  {...nodeKeyProps(source.id, `${source.label}, system of record`, activate)}
                >
                  <circle
                    r={STAGE.sourceRadius + 5}
                    fill={C.paper}
                    stroke={active ? C.affordance : C.hairline}
                    strokeWidth={active ? 2 : 1}
                    strokeOpacity={active ? 0.88 : 0.75}
                  />
                  <circle r={STAGE.sourceRadius} fill={C.affordance} stroke={C.paper} strokeWidth={1} strokeOpacity={0.3} />
                  <text
                    y={0.5}
                    textAnchor="middle"
                    dominantBaseline="central"
                    fill={C.paper}
                    style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 5), fontWeight: 800 }}
                  >
                    {shortMark(source.label)}
                  </text>
                  <text
                    y={STAGE.sourceRadius + lab(13)}
                    textAnchor="middle"
                    fill={C.inkSoft}
                    paintOrder="stroke"
                    stroke={C.paper}
                    strokeWidth={GEOMETRY.graphLabelHalo}
                    style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 2) }}
                  >
                    {source.label}
                  </text>
                </g>
              );
            })}

          {visibleProjects.map((project) => {
            const point = at(project.id);
            const angle = ang.get(project.id) ?? 0;
            const ramp = rampOf(project.primary_department_id);
            const active =
              selectedId === project.id || traceRelated(project.id) || hover === project.id || matches(project.id);
            const size = active ? STAGE.projectSize + 2 : STAGE.projectSize;
            const labelRadius = size + lab(12);
            const lx = labelRadius * Math.cos(angle);
            const ly = labelRadius * Math.sin(angle);
            const anchorFor = Math.cos(angle) > 0.34 ? "start" : Math.cos(angle) < -0.34 ? "end" : "middle";
            const activate = () => onSelectNode({ id: project.id, kind: "project", label: project.label });
            return (
              <g
                key={project.id}
                transform={`translate(${point.x},${point.y})`}
                opacity={op(project.id)}
                style={{ cursor: "pointer" }}
                onMouseEnter={() => setHover(project.id)}
                onMouseLeave={() => setHover(null)}
                onClick={activate}
                data-testid="graph-project"
                data-id={project.id}
                {...nodeKeyProps(
                  project.id,
                  `${compactProjectLabel(project.label)}, project, ${project.people} people`,
                  activate,
                )}
              >
                <title>{`${project.label} - ${project.people} people - ${project.workflow_name}`}</title>
                <path
                  d={`M0 ${-size}L${size} 0L0 ${size}L${-size} 0Z`}
                  fill={ramp.surface}
                  stroke={active ? C.affordance : ramp.line}
                  strokeWidth={active ? 2 : 1.1}
                  strokeOpacity={active ? 0.94 : 0.74}
                />
                <text
                  y={0.5}
                  textAnchor="middle"
                  dominantBaseline="central"
                  fill={C.ink}
                  style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 4), fontWeight: 800 }}
                >
                  {project.people}
                </text>
                <circle
                  r={size + 6}
                  fill="none"
                  stroke={C.affordance}
                  strokeWidth={active ? 2 : 0}
                  strokeOpacity={active ? 0.84 : 0}
                />
                <g transform={`translate(${lx},${ly})`}>
                  <text
                    textAnchor={anchorFor}
                    dominantBaseline="middle"
                    fill={C.ink}
                    paintOrder="stroke"
                    stroke={C.paper}
                    strokeWidth={GEOMETRY.graphLabelHalo}
                    style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 1), fontWeight: 700 }}
                    data-testid="graph-project-label"
                  >
                    {compactProjectLabel(project.label)}
                  </text>
                  <text
                    y={lab(12)}
                    textAnchor={anchorFor}
                    dominantBaseline="middle"
                    fill={C.inkSoft}
                    paintOrder="stroke"
                    stroke={C.paper}
                    strokeWidth={GEOMETRY.graphLabelHalo}
                    style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 3) }}
                    data-testid="graph-project-workflow"
                  >
                    {compactProjectLabel(project.workflow_name.replace(/^Workflow:\s*/i, ""))}
                  </text>
                </g>
              </g>
            );
          })}

          {/* THE PEOPLE RING. ≤40 in scope: every person is a real 40px node
              with the full name always beneath (13px/500). >40: honest
              per-department cluster chips with in-scope counts. */}
          {clustered
            ? graph.departments.map((dept) => {
                const arc = deptArcs.get(dept.id);
                if (!arc || arc.count === 0) return null;
                const point = polar(arc.center, STAGE.ringPeople);
                const ramp = rampOf(dept.id);
                const active =
                  focusDept === dept.id || hover === `cluster:${dept.id}` || matches(dept.id);
                const activate = () => {
                  onFocusDept(focusDept === dept.id ? null : dept.id);
                  onSelectNode({ id: dept.id, kind: "department", label: dept.label });
                };
                const clusterId = `cluster:${dept.id}`;
                return (
                  <g
                    key={clusterId}
                    transform={`translate(${point.x},${point.y})`}
                    opacity={op(dept.id)}
                    style={{ cursor: "pointer" }}
                    onMouseEnter={() => setHover(clusterId)}
                    onMouseLeave={() => setHover(null)}
                    onClick={activate}
                    data-testid="graph-people-cluster"
                    data-id={dept.id}
                    data-count={arc.count}
                    {...nodeKeyProps(clusterId, `${dept.label}: ${arc.count} people in scope`, activate)}
                  >
                    <title>{`${dept.label}: ${arc.count} people in scope`}</title>
                    <rect
                      x={-STAGE.clusterWidth / 2}
                      y={-STAGE.clusterHeight / 2}
                      width={STAGE.clusterWidth}
                      height={STAGE.clusterHeight}
                      rx={16}
                      fill={ramp.surface}
                      stroke={active ? C.affordance : ramp.line}
                      strokeWidth={active ? 2 : 1.2}
                    />
                    <text
                      y={-4}
                      textAnchor="middle"
                      fill={C.ink}
                      style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs), fontWeight: 600 }}
                    >
                      {dept.label}
                    </text>
                    <text
                      y={lab(13)}
                      textAnchor="middle"
                      fill={C.inkSoft}
                      style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 2), fontWeight: 500 }}
                    >
                      {`${arc.count} people in scope`}
                    </text>
                  </g>
                );
              })
            : !hidden.has("people") &&
              graph.people.map((person) => {
                const point = at(person.id);
                const active =
                  hover === person.id ||
                  selectedId === person.id ||
                  traceRelated(person.id) ||
                  matches(person.id) ||
                  (focusDept !== null && person.department_id === focusDept);
                const size = STAGE.personNode;
                const ramp = rampOf(person.department_id);
                const deptLabel = deptById.get(person.department_id)?.label ?? person.department_id;
                const activate = () =>
                  onSelectNode({ id: person.id, kind: "human", label: person.display_name });
                return (
                  <g
                    key={person.id}
                    transform={`translate(${point.x},${point.y})`}
                    opacity={op(person.id)}
                    style={{ cursor: "pointer" }}
                    onMouseEnter={() => setHover(person.id)}
                    onMouseLeave={() => setHover(null)}
                    onClick={activate}
                    data-testid="graph-person"
                    data-id={person.id}
                    data-ring={person.ring}
                    data-self={person.is_self ? "true" : "false"}
                    {...nodeKeyProps(person.id, `${person.display_name}, ${person.title}, ${deptLabel}`, activate)}
                  >
                    <title>{`${person.display_name}, ${person.title}`}</title>
                    <circle r={size / 2 + 3} fill={C.paper} stroke={ramp.line} strokeWidth={1.2} />
                    {person.is_self ? (
                      <circle
                        r={size / 2 + 7}
                        fill="none"
                        stroke={C.affordance}
                        strokeWidth={2}
                        data-testid="graph-self-marker"
                      />
                    ) : null}
                    <foreignObject x={-size / 2} y={-size / 2} width={size} height={size}>
                      <PersonAvatar
                        principalId={person.id}
                        displayName={person.display_name}
                        size={size}
                        tint={{ background: ramp.surface, border: ramp.line }}
                      />
                    </foreignObject>
                    <circle
                      r={size / 2 + 7}
                      fill="none"
                      stroke={C.affordance}
                      strokeWidth={selectedId === person.id || active ? 2 : 0}
                      strokeOpacity={selectedId === person.id || active ? 0.9 : 0}
                    />
                    {/* The full name, ALWAYS, beneath the node: 13px, 500. */}
                    <g transform={`translate(0,${size / 2 + lab(16)})`}>
                      <text
                        textAnchor="middle"
                        dominantBaseline="middle"
                        fill={C.ink}
                        paintOrder="stroke"
                        stroke={C.paper}
                        strokeWidth={GEOMETRY.graphLabelHalo}
                        style={{
                          fontFamily: FONT.chrome,
                          fontSize: lab(TYPE.scale.xs),
                          fontWeight: 500,
                        }}
                        data-testid="graph-person-name"
                      >
                        {person.display_name}
                      </text>
                      {person.ring === "anchor" || active ? (
                        <text
                          y={lab(13)}
                          textAnchor="middle"
                          dominantBaseline="middle"
                          fill={C.inkSoft}
                          paintOrder="stroke"
                          stroke={C.paper}
                          strokeWidth={GEOMETRY.graphLabelHalo}
                          style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 2) }}
                          data-testid="graph-person-title"
                        >
                          {person.title}
                        </text>
                      ) : null}
                    </g>
                  </g>
                );
              })}

          {(() => {
            const point = at(graph.center.id);
            const active = selectedId === graph.center.id || traceRelated(graph.center.id) || hover === graph.center.id;
            const activate = () =>
              onSelectNode({ id: graph.center.id, kind: "org", label: graph.center.label });
            return (
              <g
                data-testid="graph-center"
                data-id={graph.center.id}
                style={{ cursor: "pointer" }}
                onMouseEnter={() => setHover(graph.center.id)}
                onMouseLeave={() => setHover(null)}
                onClick={activate}
                {...nodeKeyProps(graph.center.id, `${graph.center.label}, organization`, activate)}
              >
                <title>{graph.center.label}</title>
                <rect
                  x={point.x - STAGE.coreRadius}
                  y={point.y - STAGE.coreRadius}
                  width={STAGE.coreRadius * 2}
                  height={STAGE.coreRadius * 2}
                  rx={8}
                  fill={C.paper}
                  stroke={active ? C.affordance : C.hairline}
                  strokeWidth={active ? 2 : 1.4}
                />
                <text
                  x={point.x}
                  y={point.y + STAGE.coreRadius * 0.34}
                  textAnchor="middle"
                  fill={C.ink}
                  style={{ fontFamily: FONT.chrome, fontSize: STAGE.coreRadius * 0.78, fontWeight: 800 }}
                  data-testid="graph-center-mark"
                >
                  {monogram(graph.center.label)}
                </text>
                <path
                  d={`M${point.x - 16} ${point.y + 19}H${point.x + 16}`}
                  stroke={C.inkSoft}
                  strokeWidth={2}
                  strokeLinecap="round"
                  opacity={0.58}
                />
                <text
                  x={point.x}
                  y={point.y + STAGE.coreRadius + lab(20)}
                  textAnchor="middle"
                  fill={C.ink}
                  paintOrder="stroke"
                  stroke={C.paper}
                  strokeWidth={GEOMETRY.graphLabelHalo}
                  style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs), fontWeight: 700 }}
                >
                  {graph.center.label}
                </text>
              </g>
            );
          })()}
        </g>
      </svg>
    </div>
  );
}
