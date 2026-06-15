"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { select } from "d3-selection";
import { zoom as d3zoom, zoomIdentity, type ZoomBehavior, type ZoomTransform } from "d3-zoom";
import type { GraphEdge, GraphPerson, GraphResponse } from "@/lib/api";
import { DEPARTMENT_TINT, DERIVED, FONT, GEOMETRY, TYPE } from "@/lib/tokens";
import { PersonAvatar } from "./PersonAvatar";

/**
 * THE ORG BRAIN — a dense, scope-honest CONCENTRIC-RING map of the company.
 * Deterministic polar layout (no force-scatter): the org core at the still
 * point; the department hubs on ring 1; the agents on ring 2; the systems of
 * record (sources) on ring 3; and ALL 120 people on one continuous outer
 * circle, ordered so each department owns an unbroken arc whose width is
 * proportional to its headcount. Every node is a real Bryremead entity — no
 * decorative padding. HONEST DARK: it draws exactly the data; a small world is
 * a small graph. Colours come from theme CSS variables (dark by default), the
 * department hues from the reserved sensitivity palette.
 */
type Pos = { x: number; y: number };
export type GraphNodeKind = "org" | "department" | "human" | "agent" | "source";
export type SelectedNode = { id: string; kind: GraphNodeKind; label: string };

type Layout = {
  pos: Map<string, Pos>;
  /** Per-person angle on the outer ring (for radial label placement). */
  ang: Map<string, number>;
  /** Department hub centre angle, for framing focus mode. */
  hubAngle: Map<string, number>;
};

/** LOD rule for a person's name: anchors always; a member's name on hover,
 * selection, a search hit, or once zoomed past the reveal scale. Pure —
 * exported for the LOD test. */
export function nameVisible(
  ring: "anchor" | "member",
  active: boolean,
  scaleK: number,
  lodReveal: number = GEOMETRY.graphLodReveal,
): boolean {
  return ring === "anchor" || active || scaleK >= lodReveal;
}

function polar(cx: number, cy: number, r: number, a: number): Pos {
  return { x: cx + r * Math.cos(a), y: cy + r * Math.sin(a) };
}

function computeLayout(graph: GraphResponse): Layout {
  const v = GEOMETRY.graphViewport;
  const cx = v / 2;
  const cy = v / 2;
  const coreY = cy + GEOMETRY.graphCenterOffsetY;
  const pos = new Map<string, Pos>();
  const ang = new Map<string, number>();
  const hubAngle = new Map<string, number>();
  pos.set(graph.center.id, { x: cx, y: coreY });

  const peopleByDept = new Map<string, GraphPerson[]>();
  for (const p of graph.people) {
    const list = peopleByDept.get(p.department_id) ?? [];
    list.push(p);
    peopleByDept.set(p.department_id, list);
  }
  const total = Math.max(graph.people.length, 1);
  const gap = GEOMETRY.graphArcGap;
  const usable = 2 * Math.PI - graph.departments.length * gap;

  // Walk the departments in declared order, each owning an arc ∝ its headcount.
  let cursor = -Math.PI / 2;
  for (const dept of graph.departments) {
    const list = [...(peopleByDept.get(dept.id) ?? [])].sort((a, b) =>
      a.ring === b.ring ? a.id.localeCompare(b.id) : a.ring === "anchor" ? -1 : 1,
    );
    const span = usable * (list.length / total);
    const arcStart = cursor + gap / 2;
    const arcCenter = arcStart + span / 2;
    hubAngle.set(dept.id, arcCenter);
    pos.set(dept.id, polar(cx, cy, GEOMETRY.graphRingDept, arcCenter));
    list.forEach((person, i) => {
      const frac = list.length <= 1 ? 0.5 : i / (list.length - 1);
      const a = arcStart + frac * span;
      pos.set(person.id, polar(cx, cy, GEOMETRY.graphRingPeople, a));
      ang.set(person.id, a);
    });
    cursor = arcStart + span + gap / 2;
  }

  // Agents on ring 2, radially inward from their owner's department arc.
  const agentsSeen = new Map<string, number>();
  for (const tool of graph.tools) {
    const base = tool.department_id ? hubAngle.get(tool.department_id) : undefined;
    const k = tool.department_id ? agentsSeen.get(tool.department_id) ?? 0 : 0;
    if (tool.department_id) agentsSeen.set(tool.department_id, k + 1);
    const a =
      base !== undefined
        ? base + (k % 2 === 0 ? 1 : -1) * 0.06 * Math.ceil(k / 2)
        : -Math.PI / 2 + (graph.tools.indexOf(tool) / Math.max(graph.tools.length, 1)) * 2 * Math.PI;
    pos.set(tool.id, polar(cx, cy, GEOMETRY.graphRingAgents, a));
  }

  // Sources on ring 3, evenly spaced (org-wide systems, no department tie).
  graph.sources.forEach((source, i) => {
    const a = -Math.PI / 2 + (i / Math.max(graph.sources.length, 1)) * 2 * Math.PI;
    pos.set(source.id, polar(cx, cy, GEOMETRY.graphRingSources, a));
  });

  return { pos, ang, hubAngle };
}

function tintOf(label: string): { background: string; border: string } {
  return DEPARTMENT_TINT[label] ?? { background: DERIVED.wash, border: DERIVED.hairline };
}

/** Per-kind edge ranking. Reporting is the spine; membership the recessive web;
 * agent ownership a dashed warm affordance; a source's tie to the core dotted. */
const EDGE_STYLE: Record<string, { width: number; opacity: number; dash?: string; warm?: boolean }> = {
  reports_to: { width: 1.4, opacity: 0.42 },
  member_of: { width: 0.7, opacity: 0.12 },
  owns_agent: { width: 1.0, opacity: 0.4, dash: "3 4", warm: true },
  system_of: { width: 0.9, opacity: 0.22, dash: "1 4" },
  uses: { width: 1.0, opacity: 0.3, dash: "1 4" },
};

function edgePath(from: Pos, to: Pos, curve: number): string {
  const mx = (from.x + to.x) / 2;
  const my = (from.y + to.y) / 2;
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const len = Math.hypot(dx, dy) || 1;
  return `M${from.x},${from.y}Q${mx + (-dy / len) * curve},${my + (dx / len) * curve} ${to.x},${to.y}`;
}

const C = {
  ink: "var(--ink)",
  paper: "var(--paper)",
  inkSoft: "var(--ink-soft)",
  affordance: "var(--affordance)",
  warm: "var(--accent-warm)",
  hairline: "var(--hairline)",
};

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
  const { pos, ang, hubAngle } = useMemo(() => computeLayout(graph), [graph]);
  const [hover, setHover] = useState<string | null>(null);
  const [transform, setTransform] = useState<ZoomTransform>(zoomIdentity);
  const svgRef = useRef<SVGSVGElement | null>(null);
  const zoomRef = useRef<ZoomBehavior<SVGSVGElement, unknown> | null>(null);

  const neighbors = useMemo(() => {
    const map = new Map<string, Set<string>>();
    const link = (a: string, b: string) => {
      (map.get(a) ?? map.set(a, new Set()).get(a)!).add(b);
      (map.get(b) ?? map.set(b, new Set()).get(b)!).add(a);
    };
    for (const e of graph.edges) link(e.from, e.to);
    return map;
  }, [graph.edges]);

  const peopleById = useMemo(() => new Map(graph.people.map((p) => [p.id, p])), [graph.people]);
  const toolsById = useMemo(() => new Map(graph.tools.map((t) => [t.id, t])), [graph.tools]);

  const v = GEOMETRY.graphViewport;
  const m = GEOMETRY.graphMargin;
  const k = transform.k || 1;
  const at = (id: string): Pos => pos.get(id) ?? { x: v / 2, y: v / 2 };
  const lab = (px: number) => px / k;

  // d3-zoom: pan/zoom the whole scene; the behavior instance is kept so reset
  // and focus framing drive it (never a throwaway instance).
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

  // FOCUS MODE framing: when a department is focused, fly the view to its arc;
  // clearing it returns to fit. A visual transform, not a navigation.
  useEffect(() => {
    const svg = svgRef.current;
    if (!svg || !zoomRef.current) return;
    try {
      if (focusDept !== null) {
        const a = hubAngle.get(focusDept);
        if (a === undefined) return;
        const focusK = 1.85;
        // Frame a point partway out the department's arc (between hub and ring).
        const fx = v / 2 + GEOMETRY.graphRingPeople * 0.78 * Math.cos(a);
        const fy = v / 2 + GEOMETRY.graphRingPeople * 0.78 * Math.sin(a);
        const t = zoomIdentity.translate(v / 2 - focusK * fx, v / 2 - focusK * fy).scale(focusK);
        select(svg).call(zoomRef.current.transform, t);
      } else {
        select(svg).call(zoomRef.current.transform, zoomIdentity);
      }
    } catch {
      /* framing is an enhancement */
    }
  }, [focusDept, hubAngle, v]);

  const hidden = useMemo(() => new Set(hiddenKinds), [hiddenKinds]);
  const q = query.trim().toLowerCase();

  const matches = (id: string): boolean => {
    if (q.length === 0) return false;
    const person = peopleById.get(id);
    if (person)
      return (
        person.display_name.toLowerCase().includes(q) ||
        person.title.toLowerCase().includes(q) ||
        person.department_id.toLowerCase().includes(q) ||
        person.id.toLowerCase().includes(q)
      );
    const tool = toolsById.get(id);
    if (tool) return tool.label.toLowerCase().includes(q) || tool.id.toLowerCase().includes(q);
    return id.toLowerCase().includes(q);
  };

  const inDept = (id: string): boolean =>
    id === focusDept ||
    peopleById.get(id)?.department_id === focusDept ||
    toolsById.get(id)?.department_id === focusDept;

  // Emphasis: focus mode > hover > search > none.
  const dimming = focusDept !== null || hover !== null || q.length > 0;
  const emphasized = (id: string): boolean => {
    if (focusDept !== null) return inDept(id) || id === graph.center.id;
    if (hover !== null) return id === hover || (neighbors.get(hover)?.has(id) ?? false);
    if (q.length > 0) return matches(id);
    return true;
  };
  const op = (id: string): number => {
    if (!dimming || emphasized(id)) return 1;
    return focusDept !== null ? GEOMETRY.graphGhostOpacity : GEOMETRY.graphDimOpacity;
  };

  const edgeTouchesFocus = (e: GraphEdge): boolean => {
    if (focusDept !== null) return inDept(e.from) || inDept(e.to);
    if (hover !== null) return e.from === hover || e.to === hover;
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

  return (
    <div className="relative h-full" data-testid="org-graph">
      <div className="absolute right-2 top-2 z-10">
        <button
          type="button"
          onClick={reset}
          className="ap-card ap-washable ap-register-chrome rounded px-2 py-1"
          style={{ fontSize: TYPE.scale.xs, fontWeight: 500 }}
          data-testid="graph-reset"
        >
          Fit / reset
        </button>
      </div>
      <svg
        ref={svgRef}
        viewBox={`${-m} ${-m} ${v + 2 * m} ${v + 2 * m}`}
        className={`block w-full${reducedMotion ? "" : " ap-fade-view"}`}
        style={{ maxHeight: "82vh", touchAction: "none", cursor: "grab" }}
        role="img"
        aria-label="Organization graph"
      >
        <g transform={transform.toString()} data-testid="graph-scene">
          {/* Faint ring guides — the concentric scaffold. */}
          <g data-testid="graph-rings" opacity={focusDept !== null ? GEOMETRY.graphGhostOpacity : 1}>
            {[
              GEOMETRY.graphRingDept,
              GEOMETRY.graphRingAgents,
              GEOMETRY.graphRingSources,
              GEOMETRY.graphRingPeople,
            ].map((r) => (
              <circle
                key={r}
                cx={v / 2}
                cy={v / 2}
                r={r}
                fill="none"
                stroke={C.hairline}
                strokeWidth={1}
                strokeOpacity={0.5}
              />
            ))}
          </g>

          {/* Edges, curved and ranked by kind. */}
          <g data-testid="graph-edges">
            {graph.edges.map((e, i) => {
              if (hidden.has("people") && (peopleById.has(e.from) || peopleById.has(e.to))) return null;
              if (hidden.has("agents") && (toolsById.has(e.from) || toolsById.has(e.to))) return null;
              if (hidden.has("sources") && e.kind === "system_of") return null;
              const from = at(e.from);
              const to = at(e.to);
              const style = EDGE_STYLE[e.kind] ?? EDGE_STYLE.member_of;
              const lit = edgeTouchesFocus(e);
              const faded = dimming && !lit;
              const dx = to.x - from.x;
              const dy = to.y - from.y;
              const curve = (e.from < e.to ? 1 : -1) * Math.min(Math.hypot(dx, dy) * 0.1, 46);
              const stroke = lit || style.warm ? C.warm : C.inkSoft;
              return (
                <path
                  key={`${e.from}-${e.kind}-${e.to}-${i}`}
                  d={edgePath(from, to, curve)}
                  fill="none"
                  stroke={stroke}
                  strokeWidth={lit ? Math.max(style.width, 1.5) : style.width}
                  strokeOpacity={faded ? 0.04 : lit ? 0.7 : style.opacity}
                  strokeDasharray={style.dash}
                  strokeLinecap="round"
                  data-testid="graph-edge"
                  data-kind={e.kind}
                />
              );
            })}
          </g>

          {/* Department hubs. */}
          {graph.departments.map((d) => {
            const p = at(d.id);
            const tint = tintOf(d.tint_key);
            return (
              <g
                key={d.id}
                transform={`translate(${p.x},${p.y})`}
                opacity={op(d.id)}
                style={{ cursor: "pointer" }}
                onClick={() => {
                  onFocusDept(focusDept === d.id ? null : d.id);
                  onSelectNode({ id: d.id, kind: "department", label: d.label });
                }}
                data-testid="graph-dept"
                data-dept={d.id}
              >
                <circle r={GEOMETRY.graphHubRadius} fill={tint.background} stroke={tint.border} strokeWidth={1.5} />
                <text
                  y={-GEOMETRY.graphHubRadius - lab(6)}
                  textAnchor="middle"
                  fill={C.ink}
                  paintOrder="stroke"
                  stroke={C.paper}
                  strokeWidth={GEOMETRY.graphLabelHalo}
                  style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs), fontWeight: 600 }}
                  data-testid="graph-dept-label"
                >
                  {d.label}
                </text>
              </g>
            );
          })}

          {/* Agents — distinct dashed rounded squares, always labelled small. */}
          {!hidden.has("agents") &&
            graph.tools.map((t) => {
              const p = at(t.id);
              const tint = t.department_id ? tintOf(t.department_id) : tintOf("");
              const s = GEOMETRY.graphAgentSize;
              return (
                <g
                  key={t.id}
                  transform={`translate(${p.x},${p.y})`}
                  opacity={op(t.id)}
                  style={{ cursor: "pointer" }}
                  onMouseEnter={() => setHover(t.id)}
                  onMouseLeave={() => setHover(null)}
                  onClick={() => onSelectNode({ id: t.id, kind: "agent", label: t.label })}
                  data-testid="graph-tool"
                  data-id={t.id}
                >
                  <rect
                    x={-s}
                    y={-s}
                    width={s * 2}
                    height={s * 2}
                    rx={3}
                    fill={tint.background}
                    stroke={tint.border}
                    strokeWidth={1}
                    strokeDasharray="3 2"
                  />
                  <text
                    y={s + lab(11)}
                    textAnchor="middle"
                    fill={C.inkSoft}
                    paintOrder="stroke"
                    stroke={C.paper}
                    strokeWidth={GEOMETRY.graphLabelHalo}
                    style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs) }}
                  >
                    {t.label}
                  </text>
                </g>
              );
            })}

          {/* Sources — the org's real systems of record (ring 3). */}
          {!hidden.has("sources") &&
            graph.sources.map((s) => {
              const p = at(s.id);
              const sz = GEOMETRY.graphSourceSize;
              return (
                <g
                  key={s.id}
                  transform={`translate(${p.x},${p.y})`}
                  opacity={op(s.id)}
                  style={{ cursor: "pointer" }}
                  onMouseEnter={() => setHover(s.id)}
                  onMouseLeave={() => setHover(null)}
                  onClick={() => onSelectNode({ id: s.id, kind: "source", label: s.label })}
                  data-testid="graph-source"
                  data-id={s.id}
                >
                  <rect
                    x={-sz}
                    y={-sz}
                    width={sz * 2}
                    height={sz * 2}
                    transform="rotate(45)"
                    fill={C.paper}
                    stroke={C.affordance}
                    strokeWidth={1.5}
                  />
                  <text
                    y={sz + lab(12)}
                    textAnchor="middle"
                    fill={C.inkSoft}
                    paintOrder="stroke"
                    stroke={C.paper}
                    strokeWidth={GEOMETRY.graphLabelHalo}
                    style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs) }}
                  >
                    {s.label}
                  </text>
                </g>
              );
            })}

          {/* People — the outer ring; members first so anchors paint on top. */}
          {!hidden.has("people") &&
            [...graph.people]
              .sort((a, b) => (a.ring === b.ring ? 0 : a.ring === "anchor" ? 1 : -1))
              .map((person) => {
                const p = at(person.id);
                const a = ang.get(person.id) ?? 0;
                const size =
                  person.ring === "anchor" ? GEOMETRY.graphAnchorAvatar : GEOMETRY.graphMemberAvatar;
                const tint = tintOf(person.department_id);
                const active =
                  hover === person.id ||
                  selectedId === person.id ||
                  matches(person.id) ||
                  (focusDept !== null && person.department_id === focusDept);
                const show = nameVisible(person.ring, active, k);
                const ringPad = person.ring === "anchor" ? 3 : 2;
                // Labels sit radially OUTWARD (their own collision avoidance).
                const lr = size / 2 + ringPad + lab(9);
                const lx = lr * Math.cos(a);
                const ly = lr * Math.sin(a);
                const anchorFor = Math.cos(a) > 0.34 ? "start" : Math.cos(a) < -0.34 ? "end" : "middle";
                return (
                  <g
                    key={person.id}
                    transform={`translate(${p.x},${p.y})`}
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
                    <circle r={size / 2 + ringPad} fill={C.paper} />
                    <circle
                      r={size / 2 + ringPad}
                      fill="none"
                      stroke={selectedId === person.id ? C.warm : tint.border}
                      strokeWidth={person.ring === "anchor" || selectedId === person.id ? 2 : 1}
                    />
                    {person.is_self ? (
                      <circle
                        r={size / 2 + ringPad + 4}
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
                    {show ? (
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
                            fontWeight: person.ring === "anchor" ? 600 : 500,
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
                            style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs) }}
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

          {/* Center: the org core, given presence — warm glow, weight, monogram. */}
          {(() => {
            const c = at(graph.center.id);
            const r = GEOMETRY.graphCoreRadius;
            return (
              <g
                data-testid="graph-center"
                style={{ cursor: "pointer" }}
                onClick={() => onSelectNode({ id: graph.center.id, kind: "org", label: graph.center.label })}
              >
                <circle cx={c.x} cy={c.y} r={r + 10} fill="none" stroke={C.warm} strokeWidth={1.5} opacity={0.45} />
                <circle cx={c.x} cy={c.y} r={r} fill={C.ink} />
                <circle cx={c.x} cy={c.y} r={r} fill="none" stroke={C.warm} strokeWidth={1.5} />
                <text
                  x={c.x}
                  y={c.y + r * 0.34}
                  textAnchor="middle"
                  fill={C.paper}
                  style={{ fontFamily: FONT.chrome, fontSize: r * 0.78, fontWeight: 700 }}
                  data-testid="graph-center-mark"
                >
                  {monogram(graph.center.label)}
                </text>
              </g>
            );
          })()}
        </g>
      </svg>
    </div>
  );
}

function monogram(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}
