"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { select } from "d3-selection";
import { zoom as d3zoom, zoomIdentity, type ZoomBehavior, type ZoomTransform } from "d3-zoom";
import type { GraphEdge, GraphPerson, GraphResponse } from "@/lib/api";
import { DEPARTMENT_TINT, DERIVED, FONT, GEOMETRY, TYPE } from "@/lib/tokens";
import { PersonAvatar } from "./PersonAvatar";

type Pos = { x: number; y: number };
export type GraphNodeKind = "org" | "department" | "human" | "agent" | "source";
export type SelectedNode = { id: string; kind: GraphNodeKind; label: string };

type DeptArc = { start: number; end: number; center: number; count: number };
type Layout = {
  pos: Map<string, Pos>;
  ang: Map<string, number>;
  hubAngle: Map<string, number>;
  deptArcs: Map<string, DeptArc>;
};

const STAGE = {
  width: 1400,
  height: 980,
  cx: 700,
  cy: 520,
  ringDept: 205,
  ringAgents: 258,
  ringSignal: 304,
  ringPermission: 373,
  ringSources: 444,
  ringPeople: 575,
  ringAccess: 521,
  hubRadius: 23,
  coreRadius: 32,
  sourceRadius: 8,
  agentSize: 7,
  personMember: 8,
  personMemberActive: 18,
  personAnchor: 30,
} as const;

const C = {
  ink: "var(--ink)",
  paper: "var(--paper)",
  inkSoft: "var(--ink-soft)",
  affordance: "var(--affordance)",
  warm: "var(--accent-warm)",
  hairline: "var(--hairline)",
  wash: "var(--wash)",
};

/** LOD rule for a person's name: anchors always; a member's name on hover,
 * selection, a search hit, or once zoomed past the reveal scale. Pure -
 * exported for the LOD test. */
export function nameVisible(
  ring: "anchor" | "member",
  active: boolean,
  scaleK: number,
  lodReveal: number = GEOMETRY.graphLodReveal,
): boolean {
  return ring === "anchor" || active || scaleK >= lodReveal;
}

function polar(angle: number, radius: number, cx = STAGE.cx, cy = STAGE.cy): Pos {
  return {
    x: cx + radius * Math.cos(angle),
    y: cy + radius * Math.sin(angle),
  };
}

function jitter(index: number, amplitude: number, seed = 1): number {
  const value = Math.sin((index + 1) * 12.9898 + seed * 78.233) * 43758.5453;
  return (value - Math.floor(value) - 0.5) * amplitude;
}

function tintOf(label: string): { background: string; border: string } {
  return DEPARTMENT_TINT[label] ?? { background: DERIVED.wash, border: DERIVED.hairline };
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
    pos.set(dept.id, polar(center, STAGE.ringDept + jitter(graph.departments.indexOf(dept), 9, 2)));

    list.forEach((person, index) => {
      const frac = list.length <= 1 ? 0.5 : index / (list.length - 1);
      const angle = start + frac * span;
      const radius = STAGE.ringPeople + jitter(index, person.ring === "anchor" ? 12 : 20, dept.id.length);
      pos.set(person.id, polar(angle, radius));
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
    pos.set(tool.id, polar(angle, STAGE.ringAgents + jitter(index, 8, 5)));
    ang.set(tool.id, angle);
  });

  graph.sources.forEach((source, index) => {
    const angle = -Math.PI / 2 + (index / Math.max(graph.sources.length, 1)) * 2 * Math.PI;
    pos.set(source.id, polar(angle + jitter(index, 0.05, 7), STAGE.ringSources + jitter(index, 8, 8)));
    ang.set(source.id, angle);
  });

  return { pos, ang, hubAngle, deptArcs };
}

const EDGE_STYLE: Record<string, { width: number; opacity: number; dash?: string; warm?: boolean }> = {
  reports_to: { width: 0.9, opacity: 0.16 },
  member_of: { width: 0.75, opacity: 0.12 },
  owns_agent: { width: 1.0, opacity: 0.38, dash: "2 5", warm: true },
  system_of: { width: 0.9, opacity: 0.2, dash: "1 8" },
  uses: { width: 0.9, opacity: 0.2, dash: "1 8" },
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
  const zoomRef = useRef<ZoomBehavior<SVGSVGElement, unknown> | null>(null);

  const peopleById = useMemo(() => new Map(graph.people.map((p) => [p.id, p])), [graph.people]);
  const toolsById = useMemo(() => new Map(graph.tools.map((t) => [t.id, t])), [graph.tools]);
  const sourcesById = useMemo(() => new Map(graph.sources.map((s) => [s.id, s])), [graph.sources]);
  const deptById = useMemo(() => new Map(graph.departments.map((d) => [d.id, d])), [graph.departments]);
  const primaryAnchorIds = useMemo(() => {
    const ids = new Set<string>();
    for (const dept of graph.departments) {
      const anchor = graph.people
        .filter((person) => person.department_id === dept.id && person.ring === "anchor")
        .sort((a, b) => a.id.localeCompare(b.id))[0];
      if (anchor) ids.add(anchor.id);
    }
    return ids;
  }, [graph.departments, graph.people]);

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
    const dept = deptById.get(id);
    if (dept) return dept.label.toLowerCase().includes(q) || dept.id.toLowerCase().includes(q);
    return id.toLowerCase().includes(q);
  };

  const inDept = (id: string): boolean =>
    id === focusDept ||
    peopleById.get(id)?.department_id === focusDept ||
    toolsById.get(id)?.department_id === focusDept;

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
    (hidden.has("people") && (peopleById.has(edge.from) || peopleById.has(edge.to))) ||
    (hidden.has("agents") && (toolsById.has(edge.from) || toolsById.has(edge.to))) ||
    (hidden.has("sources") && (sourcesById.has(edge.from) || sourcesById.has(edge.to)));

  return (
    <div className="relative h-full min-h-0" data-testid="org-graph">
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

      <svg
        ref={svgRef}
        viewBox={`0 0 ${STAGE.width} ${STAGE.height}`}
        className={`block h-full w-full${reducedMotion ? "" : " ap-fade-view"}`}
        style={{ touchAction: "none", cursor: "grab" }}
        role="img"
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
              const style = EDGE_STYLE[edge.kind] ?? EDGE_STYLE.member_of;
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
                  stroke={lit || style.warm ? C.warm : C.inkSoft}
                  strokeWidth={lit ? Math.max(style.width, 1.5) : style.width}
                  strokeOpacity={faded ? 0.035 : lit ? 0.72 : style.opacity}
                  strokeDasharray={style.dash}
                  strokeLinecap="round"
                  data-testid="graph-edge"
                  data-kind={edge.kind}
                />
              );
            })}
          </g>

          <g data-testid="graph-rings" opacity={focusDept !== null ? 0.42 : 1}>
            {[STAGE.ringPeople, STAGE.ringSources, STAGE.ringPermission, STAGE.ringSignal, STAGE.ringAgents].map(
              (radius) => (
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
              ),
            )}
            <circle
              cx={STAGE.cx}
              cy={STAGE.cy}
              r={STAGE.ringSignal}
              fill="none"
              stroke={C.warm}
              strokeWidth={2}
              strokeDasharray="1 14"
              strokeLinecap="round"
              strokeOpacity={0.22}
              data-testid="graph-signal-ring"
              aria-label="Signals unavailable in graph payload"
            />
            <circle
              cx={STAGE.cx}
              cy={STAGE.cy}
              r={STAGE.ringPermission}
              fill="none"
              stroke={C.inkSoft}
              strokeWidth={1.5}
              strokeDasharray="1 12"
              strokeLinecap="round"
              strokeOpacity={0.18}
              data-testid="graph-permission-ring"
              aria-label="Permissions unavailable in graph payload"
            />
            <text
              x={STAGE.cx - STAGE.ringSignal - 16}
              y={STAGE.cy + STAGE.ringSignal + 16}
              fill={C.inkSoft}
              style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs), opacity: 0.72 }}
            >
              signals unavailable
            </text>
            <text
              x={STAGE.cx + STAGE.ringPermission - 136}
              y={STAGE.cy - STAGE.ringPermission - 12}
              fill={C.inkSoft}
              style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs), opacity: 0.62 }}
            >
              permissions unavailable
            </text>
          </g>

          <g data-testid="graph-dept-arcs">
            {graph.departments.map((dept) => {
              const arc = deptArcs.get(dept.id);
              if (!arc) return null;
              const tint = tintOf(dept.tint_key);
              return (
                <path
                  key={dept.id}
                  d={arcPath(STAGE.ringAccess, arc.start, arc.end)}
                  fill="none"
                  stroke={tint.border}
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
            const tint = tintOf(dept.tint_key);
            const active =
              focusDept === dept.id || selectedId === dept.id || traceRelated(dept.id) || hover === dept.id || matches(dept.id);
            return (
              <g
                key={dept.id}
                transform={`translate(${point.x},${point.y})`}
                opacity={op(dept.id)}
                style={{ cursor: "pointer" }}
                onMouseEnter={() => setHover(dept.id)}
                onMouseLeave={() => setHover(null)}
                onClick={() => {
                  onFocusDept(focusDept === dept.id ? null : dept.id);
                  onSelectNode({ id: dept.id, kind: "department", label: dept.label });
                }}
                data-testid="graph-dept"
                data-dept={dept.id}
              >
                <title>{`${dept.label} department`}</title>
                <circle r={STAGE.hubRadius + 10} fill={tint.border} opacity={0.2} filter="url(#graph-soft-glow)" />
                <circle r={STAGE.hubRadius} fill={tint.background} stroke={tint.border} strokeWidth={1.6} />
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
                  stroke={active ? C.warm : C.hairline}
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
                  {`${deptArcs.get(dept.id)?.count ?? 0} people`}
                </text>
              </g>
            );
          })}

          {!hidden.has("agents") &&
            graph.tools.map((tool) => {
              const point = at(tool.id);
              const tint = tool.department_id ? tintOf(tool.department_id) : tintOf("");
              const active = selectedId === tool.id || traceRelated(tool.id) || hover === tool.id || matches(tool.id);
              const size = active || focusDept === tool.department_id ? STAGE.agentSize + 2 : STAGE.agentSize;
              return (
                <g
                  key={tool.id}
                  transform={`translate(${point.x},${point.y})`}
                  opacity={op(tool.id)}
                  style={{ cursor: "pointer" }}
                  onMouseEnter={() => setHover(tool.id)}
                  onMouseLeave={() => setHover(null)}
                  onClick={() => onSelectNode({ id: tool.id, kind: "agent", label: tool.label })}
                  data-testid="graph-tool"
                  data-id={tool.id}
                  data-kind={tool.kind}
                >
                  <title>{tool.label}</title>
                  <rect
                    x={-size}
                    y={-size}
                    width={size * 2}
                    height={size * 2}
                    rx={3}
                    fill={tint.border}
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
                  <circle
                    r={size + 5}
                    fill="none"
                    stroke={C.warm}
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
              return (
                <g
                  key={source.id}
                  transform={`translate(${point.x},${point.y})`}
                  opacity={op(source.id)}
                  style={{ cursor: "pointer" }}
                  onMouseEnter={() => setHover(source.id)}
                  onMouseLeave={() => setHover(null)}
                  onClick={() => onSelectNode({ id: source.id, kind: "source", label: source.label })}
                  data-testid="graph-source"
                  data-id={source.id}
                  aria-label={source.label}
                >
                  <circle
                    r={STAGE.sourceRadius + 5}
                    fill={C.paper}
                    stroke={active ? C.warm : C.hairline}
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

          {!hidden.has("people") &&
            [...graph.people]
              .sort((a, b) => (a.ring === b.ring ? 0 : a.ring === "anchor" ? 1 : -1))
              .map((person) => {
                const point = at(person.id);
                const angle = ang.get(person.id) ?? 0;
                const active =
                  hover === person.id ||
                  selectedId === person.id ||
                  traceRelated(person.id) ||
                  matches(person.id) ||
                  (focusDept !== null && person.department_id === focusDept);
                const expanded = person.ring === "anchor" || active || k >= GEOMETRY.graphLodReveal;
                const size =
                  person.ring === "anchor"
                    ? STAGE.personAnchor
                    : expanded
                      ? STAGE.personMemberActive
                      : STAGE.personMember;
                const ringPad = person.ring === "anchor" ? 3 : expanded ? 2 : 1.5;
                const labelFrameSafe =
                  point.x > 90 &&
                  point.x < STAGE.width - 90 &&
                  point.y > 110 &&
                  point.y < STAGE.height - 110;
                const anchorLabelAllowed =
                  person.ring !== "anchor" ||
                  (primaryAnchorIds.has(person.id) && labelFrameSafe) ||
                  active ||
                  k >= GEOMETRY.graphLodReveal;
                const showName = nameVisible(person.ring, active, k) && anchorLabelAllowed;
                const labelRadius = size / 2 + ringPad + lab(person.ring === "anchor" ? 11 : 8);
                const lx = labelRadius * Math.cos(angle);
                const ly = labelRadius * Math.sin(angle);
                const anchorFor = Math.cos(angle) > 0.34 ? "start" : Math.cos(angle) < -0.34 ? "end" : "middle";
                const tint = tintOf(person.department_id);
                return (
                  <g
                    key={person.id}
                    transform={`translate(${point.x},${point.y})`}
                    opacity={op(person.id)}
                    style={{ cursor: "pointer" }}
                    onMouseEnter={() => setHover(person.id)}
                    onMouseLeave={() => setHover(null)}
                    onClick={() => onSelectNode({ id: person.id, kind: "human", label: person.display_name })}
                    data-testid="graph-person"
                    data-id={person.id}
                    data-ring={person.ring}
                    data-self={person.is_self ? "true" : "false"}
                  >
                    <title>{`${person.display_name}, ${person.title}`}</title>
                    <circle r={size / 2 + ringPad + 1} fill={C.paper} stroke={tint.border} strokeWidth={person.ring === "anchor" ? 1.3 : 0.8} />
                    {person.is_self ? (
                      <circle
                        r={size / 2 + ringPad + 5}
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
                        department={person.department_id}
                        size={size}
                      />
                    </foreignObject>
                    <circle
                      r={size / 2 + ringPad + 5}
                      fill="none"
                      stroke={selectedId === person.id || active ? C.warm : C.hairline}
                      strokeWidth={selectedId === person.id || active ? 2 : 0}
                      strokeOpacity={selectedId === person.id || active ? 0.9 : 0}
                    />
                    {showName ? (
                      <g transform={`translate(${lx},${ly})`}>
                        <text
                          textAnchor={anchorFor}
                          dominantBaseline="middle"
                          fill={C.ink}
                          paintOrder="stroke"
                          stroke={C.paper}
                          strokeWidth={GEOMETRY.graphLabelHalo}
                          style={{
                            fontFamily: FONT.chrome,
                            fontSize: lab(TYPE.scale.xs),
                            fontWeight: person.ring === "anchor" ? 700 : 500,
                          }}
                          data-testid="graph-person-name"
                        >
                          {person.display_name}
                        </text>
                        {person.ring === "anchor" || active ? (
                          <text
                            y={lab(12)}
                            textAnchor={anchorFor}
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
                    ) : null}
                  </g>
                );
              })}

          {(() => {
            const point = at(graph.center.id);
            const active = selectedId === graph.center.id || traceRelated(graph.center.id) || hover === graph.center.id;
            return (
              <g
                data-testid="graph-center"
                style={{ cursor: "pointer" }}
                onMouseEnter={() => setHover(graph.center.id)}
                onMouseLeave={() => setHover(null)}
                onClick={() => onSelectNode({ id: graph.center.id, kind: "org", label: graph.center.label })}
              >
                <title>{graph.center.label}</title>
                <rect
                  x={point.x - STAGE.coreRadius}
                  y={point.y - STAGE.coreRadius}
                  width={STAGE.coreRadius * 2}
                  height={STAGE.coreRadius * 2}
                  rx={8}
                  fill={C.paper}
                  stroke={active ? C.warm : C.hairline}
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
