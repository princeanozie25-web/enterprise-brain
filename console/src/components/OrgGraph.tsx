"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import {
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  forceX,
  forceY,
  type SimulationLinkDatum,
} from "d3-force";
import { select } from "d3-selection";
import { zoom as d3zoom, zoomIdentity, type ZoomBehavior, type ZoomTransform } from "d3-zoom";
import type { GraphEdge, GraphPerson, GraphResponse } from "@/lib/api";
import { COLOR, DEPARTMENT_TINT, DERIVED, FONT, GEOMETRY, TYPE } from "@/lib/tokens";
import { initialsOf, PersonAvatar } from "./PersonAvatar";

/**
 * THE ORG GRAPH (AR-2, rebuilt) — a calm force-directed map of the company.
 * The org sits at the still center; each department is a soft tinted DISTRICT
 * whose radius grows with its headcount; the humanized people are placed by a
 * REAL simulation — repulsion spreads them, reporting links bind manager to
 * report, a gentle pull holds each cluster over its district, and collision
 * (sized to the LABEL, not just the disc) guarantees nothing overlaps. The
 * simulation runs once to equilibrium and FREEZES — deterministic (a seeded
 * golden-angle spiral, never Math.random), no perpetual jitter.
 *
 * Names are level-of-detail: leadership anchors are always named; a member's
 * name appears on hover or once you zoom in past the reveal scale. Edges are
 * curved and ranked by kind — reporting is the solid spine, membership is a
 * recessive web, agent ownership is a dashed affordance. HONEST DARK: it draws
 * exactly the nodes in the data — never a ghost, never a "+N hidden". A small
 * permitted world is a small graph, and that is the truth.
 */
type Pos = { x: number; y: number };
type District = { id: string; x: number; y: number; r: number; hubR: number };
type Layout = {
  pos: Map<string, Pos>;
  districts: District[];
  /** Per-anchor extra vertical label offset from the declutter pass. */
  anchorLabelDy: Map<string, number>;
};

/** A node's collision footprint: the avatar half-width, padding, and — for an
 * always-labelled anchor — extra room so name bands don't collide. */
function footprint(p: GraphPerson): number {
  const avatar = p.ring === "anchor" ? GEOMETRY.graphAnchorAvatar : GEOMETRY.graphMemberAvatar;
  const labelPad = p.ring === "anchor" ? 15 : 2;
  return avatar / 2 + GEOMETRY.graphCollidePad + labelPad;
}

/**
 * LOD rule for a person's name: anchors are always named; a member's name is
 * revealed on direct focus (hover) or once the view is zoomed past the reveal
 * scale. Pure — exported for the LOD test.
 */
export function nameVisible(
  ring: "anchor" | "member",
  focused: boolean,
  scaleK: number,
  lodReveal: number = GEOMETRY.graphLodReveal,
): boolean {
  return ring === "anchor" || focused || scaleK >= lodReveal;
}

/**
 * Anchor labels hang just below their disc; when two land close enough that
 * their name bands would collide, push the lower one further down. Pure and
 * deterministic — exported for the overlap test. Returns each id's extra
 * downward offset (0 when no nudge is needed).
 */
export function declutterAnchorLabels(
  labels: { id: string; x: number; y: number; name: string }[],
  opts: { charWidth?: number; lineHeight?: number } = {},
): Map<string, number> {
  const charWidth = opts.charWidth ?? 6.2;
  const lineHeight = opts.lineHeight ?? 26; // name + title band
  const dy = new Map<string, number>();
  const placed: { x1: number; x2: number; y: number }[] = [];
  // Top→bottom, then left→right: a stable, deterministic sweep.
  const sorted = [...labels].sort((a, b) => (a.y === b.y ? a.x - b.x : a.y - b.y));
  for (const l of sorted) {
    const halfW = (l.name.length * charWidth) / 2 + 6;
    let y = l.y;
    let moved = true;
    let guard = 0;
    while (moved && guard < 256) {
      moved = false;
      guard += 1;
      for (const p of placed) {
        const xOverlap = l.x - halfW < p.x2 && l.x + halfW > p.x1;
        const yOverlap = Math.abs(y - p.y) < lineHeight;
        if (xOverlap && yOverlap) {
          y = p.y + lineHeight;
          moved = true;
        }
      }
    }
    placed.push({ x1: l.x - halfW, x2: l.x + halfW, y });
    dy.set(l.id, y - l.y);
  }
  return dy;
}

/** A simulation node — the forces only need id/ring/footprint/cluster-home.
 * Fixed OBSTACLES (the org core, the department hubs) join the same sim with
 * fx/fy pinned and a large collide radius so people orbit them instead of
 * piling onto the center mark or a hub. */
type SimNode = {
  id: string;
  x: number;
  y: number;
  r: number;
  hx: number;
  hy: number;
  ring?: "anchor" | "member";
  obstacle?: boolean;
  fx?: number;
  fy?: number;
};

function computeLayout(graph: GraphResponse): Layout {
  const v = GEOMETRY.graphViewport;
  const cx = v / 2;
  const cy = v / 2;
  const pos = new Map<string, Pos>();
  pos.set(graph.center.id, { x: cx, y: cy });

  // Headcount per department drives the √-scaled footprint: the district
  // radius and the hub weight both grow with how many people work there, and
  // bigger districts are pushed outward so they don't crowd small neighbours.
  const headcount = new Map<string, number>();
  for (const p of graph.people) {
    headcount.set(p.department_id, (headcount.get(p.department_id) ?? 0) + 1);
  }
  const districtR = (deptId: string) =>
    GEOMETRY.graphDistrictBase +
    GEOMETRY.graphDistrictPerHead * Math.sqrt(headcount.get(deptId) ?? 0);
  const hubR = (deptId: string) =>
    GEOMETRY.graphHubRadius + Math.min(Math.sqrt(headcount.get(deptId) ?? 0) * 1.6, 12);

  const hub = new Map<string, Pos>();
  const n = Math.max(graph.departments.length, 1);
  graph.departments.forEach((d, i) => {
    const a = -Math.PI / 2 + (i / n) * 2 * Math.PI;
    const extra = districtR(d.id) - GEOMETRY.graphDistrictBase;
    const radius = GEOMETRY.graphRingDept + extra * 0.5;
    const hx = cx + radius * Math.cos(a);
    const hy = cy + radius * Math.sin(a);
    hub.set(d.id, { x: hx, y: hy });
    pos.set(d.id, { x: hx, y: hy });
  });
  const fallbackHub: Pos = { x: cx, y: cy };
  const hubOf = (deptId: string) => hub.get(deptId) ?? fallbackHub;

  // Seed each person on a small deterministic golden-angle spiral around its
  // hub (reproducible, no Math.random) — the simulation does the rest.
  const GOLDEN = Math.PI * (3 - Math.sqrt(5));
  const seenInDept = new Map<string, number>();
  const sim: SimNode[] = graph.people.map((p) => {
    const k = seenInDept.get(p.department_id) ?? 0;
    seenInDept.set(p.department_id, k + 1);
    const h = hubOf(p.department_id);
    const seedR = 6 + 7 * Math.sqrt(k + 1);
    const seedA = (k + 1) * GOLDEN;
    return {
      id: p.id,
      ring: p.ring,
      x: h.x + seedR * Math.cos(seedA),
      y: h.y + seedR * Math.sin(seedA),
      r: footprint(p),
      hx: h.x,
      hy: h.y,
    };
  });

  // Fixed obstacles: the org core and each department hub. People can't sit on
  // top of them (collision), so the center mark stays clear and clusters orbit
  // their hubs. Pinned via fx/fy; excluded from charge and from the readout.
  const obstacles: SimNode[] = [
    { id: "__org", x: cx, y: cy, fx: cx, fy: cy, hx: cx, hy: cy, r: GEOMETRY.graphCenterRadius + 22, obstacle: true },
    ...graph.departments.map((d): SimNode => {
      const h = hubOf(d.id);
      return { id: `__hub_${d.id}`, x: h.x, y: h.y, fx: h.x, fy: h.y, hx: h.x, hy: h.y, r: hubR(d.id) + 6, obstacle: true };
    }),
  ];
  const nodes = [...sim, ...obstacles];

  const links: SimulationLinkDatum<SimNode>[] = graph.edges
    .filter((e) => e.kind === "reports_to")
    .map((e) => ({ source: e.from, target: e.to }));

  forceSimulation<SimNode>(nodes)
    .force(
      "charge",
      forceManyBody<SimNode>().strength((d) =>
        d.obstacle ? 0 : d.ring === "anchor" ? GEOMETRY.graphCharge * 1.7 : GEOMETRY.graphCharge,
      ),
    )
    .force(
      "link",
      forceLink<SimNode, SimulationLinkDatum<SimNode>>(links)
        .id((d) => d.id)
        .distance(GEOMETRY.graphLinkDistance)
        .strength(GEOMETRY.graphLinkStrength),
    )
    .force("x", forceX<SimNode>((d) => d.hx).strength(GEOMETRY.graphClusterStrength))
    .force("y", forceY<SimNode>((d) => d.hy).strength(GEOMETRY.graphClusterStrength))
    .force("collide", forceCollide<SimNode>((d) => d.r).iterations(GEOMETRY.graphCollideIters))
    .stop()
    .tick(GEOMETRY.graphForceTicks);

  for (const node of sim) {
    pos.set(node.id, { x: node.x, y: node.y });
  }

  const districts: District[] = graph.departments.map((d) => {
    const h = hubOf(d.id);
    return { id: d.id, x: h.x, y: h.y, r: districtR(d.id), hubR: hubR(d.id) };
  });

  // Tools: just outside their owning cluster, fanned along the hub direction.
  graph.tools.forEach((t, i) => {
    const home = t.department_id ? hubOf(t.department_id) : fallbackHub;
    const dx = home.x - cx;
    const dy = home.y - cy;
    const len = Math.hypot(dx, dy) || 1;
    const out = districtR(t.department_id ?? "") + 30;
    const jitter = (i % 2 === 0 ? 1 : -1) * 0.18 * (Math.floor(i / 2) + 1);
    const ca = Math.cos(jitter);
    const sa = Math.sin(jitter);
    const ux = (dx / len) * ca - (dy / len) * sa;
    const uy = (dx / len) * sa + (dy / len) * ca;
    pos.set(t.id, { x: home.x + ux * out, y: home.y + uy * out });
  });

  const anchorLabelDy = declutterAnchorLabels(
    graph.people
      .filter((p) => p.ring === "anchor")
      .map((p) => {
        const pt = pos.get(p.id) ?? { x: cx, y: cy };
        return { id: p.id, x: pt.x, y: pt.y, name: p.display_name };
      }),
  );

  return { pos, districts, anchorLabelDy };
}

function tintOf(label: string): { background: string; border: string } {
  return DEPARTMENT_TINT[label] ?? { background: DERIVED.wash, border: DERIVED.hairline };
}

/** Per-kind edge ranking: the reporting spine, the recessive membership web,
 * the dashed agent-ownership affordance. */
const EDGE_STYLE: Record<
  string,
  { width: number; opacity: number; dash?: string; affordance?: boolean }
> = {
  reports_to: { width: 1.6, opacity: 0.5 },
  member_of: { width: 0.8, opacity: 0.12 },
  owns_agent: { width: 1.1, opacity: 0.45, dash: "3 4", affordance: true },
  uses: { width: 1, opacity: 0.3, dash: "1 4" },
};

function edgePath(from: Pos, to: Pos, curve: number): string {
  const mx = (from.x + to.x) / 2;
  const my = (from.y + to.y) / 2;
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const len = Math.hypot(dx, dy) || 1;
  // Control point pushed perpendicular to the chord — a gentle bundled arc.
  const px = mx + (-dy / len) * curve;
  const py = my + (dx / len) * curve;
  return `M${from.x},${from.y}Q${px},${py} ${to.x},${to.y}`;
}

export function OrgGraph({
  graph,
  onSelectPerson,
  reducedMotion = false,
}: {
  graph: GraphResponse;
  onSelectPerson: (id: string) => void;
  reducedMotion?: boolean;
}) {
  const layout = useMemo(() => computeLayout(graph), [graph]);
  const { pos, districts, anchorLabelDy } = layout;
  const [focus, setFocus] = useState<string | null>(null);
  const [focusDept, setFocusDept] = useState<string | null>(null);
  const [transform, setTransform] = useState<ZoomTransform>(zoomIdentity);
  const svgRef = useRef<SVGSVGElement | null>(null);
  const zoomRef = useRef<ZoomBehavior<SVGSVGElement, unknown> | null>(null);

  // Adjacency for the focus pattern: a node and everything one edge away.
  const neighbors = useMemo(() => {
    const map = new Map<string, Set<string>>();
    const link = (a: string, b: string) => {
      (map.get(a) ?? map.set(a, new Set()).get(a)!).add(b);
      (map.get(b) ?? map.set(b, new Set()).get(b)!).add(a);
    };
    for (const e of graph.edges) link(e.from, e.to);
    return map;
  }, [graph.edges]);

  // d3-zoom: pan/zoom the whole scene. React holds the transform; the behavior
  // instance is kept so Fit/reset drives it (not a throwaway instance).
  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    try {
      const behavior = d3zoom<SVGSVGElement, unknown>()
        .scaleExtent([0.5, 4])
        .on("zoom", (event) => setTransform(event.transform));
      zoomRef.current = behavior;
      const sel = select(svg);
      sel.call(behavior);
      return () => {
        sel.on(".zoom", null);
        zoomRef.current = null;
      };
    } catch {
      // Pan/zoom is an enhancement; the static graph stands without it.
      return;
    }
  }, []);

  const peopleById = useMemo(() => new Map(graph.people.map((p) => [p.id, p])), [graph.people]);
  const toolsById = useMemo(() => new Map(graph.tools.map((t) => [t.id, t])), [graph.tools]);

  const v = GEOMETRY.graphViewport;
  const m = GEOMETRY.graphMargin;
  const k = transform.k || 1;
  const at = (id: string): Pos => pos.get(id) ?? { x: v / 2, y: v / 2 };
  // Counter-scale chrome labels by 1/k so text holds a constant on-screen size
  // as the scene zooms (the avatars and the org mark scale naturally).
  const lab = (px: number) => px / k;

  const inDept = (id: string): boolean =>
    id === focusDept ||
    peopleById.get(id)?.department_id === focusDept ||
    toolsById.get(id)?.department_id === focusDept;
  const edgeTouchesDept = (e: GraphEdge): boolean => inDept(e.from) || inDept(e.to);

  const emphasized = (id: string): boolean => {
    if (focus !== null) return id === focus || (neighbors.get(focus)?.has(id) ?? false);
    if (focusDept !== null) return inDept(id);
    return true;
  };
  const dim = focus !== null || focusDept !== null;
  const op = (id: string) => (dim && !emphasized(id) ? GEOMETRY.graphDimOpacity : 1);

  const reset = () => {
    setFocus(null);
    setFocusDept(null);
    setTransform(zoomIdentity);
    const svg = svgRef.current;
    if (svg && zoomRef.current) {
      try {
        select(svg).call(zoomRef.current.transform, zoomIdentity);
      } catch {
        /* state is already reset above */
      }
    }
  };

  return (
    <div className="relative" data-testid="org-graph">
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
        className={`w-full${reducedMotion ? "" : " ap-fade-view"}`}
        style={{ maxHeight: "78vh", touchAction: "none", cursor: "grab" }}
        role="img"
        aria-label="Organization graph"
      >
        <g transform={transform.toString()} data-testid="graph-scene">
          {/* District fields — the soft, headcount-sized department grounds. */}
          <g data-testid="graph-districts">
            {districts.map((d) => {
              const tint = tintOf(d.id);
              return (
                <circle
                  key={d.id}
                  cx={d.x}
                  cy={d.y}
                  r={d.r}
                  fill={tint.background}
                  opacity={op(d.id) * GEOMETRY.graphDistrictOpacity}
                  data-testid="graph-district"
                  data-dept={d.id}
                />
              );
            })}
          </g>

          {/* Edges (behind every node), curved and ranked by kind. */}
          <g data-testid="graph-edges">
            {graph.edges.map((e, i) => {
              const from = at(e.from);
              const to = at(e.to);
              const style = EDGE_STYLE[e.kind] ?? EDGE_STYLE.member_of;
              const litByFocus = focus !== null && (e.from === focus || e.to === focus);
              const litByDept = focusDept !== null && edgeTouchesDept(e);
              const lit = litByFocus || litByDept;
              const faded = dim && !lit;
              const dx = to.x - from.x;
              const dy = to.y - from.y;
              const length = Math.hypot(dx, dy);
              const sign = e.from < e.to ? 1 : -1;
              const curve = sign * Math.min(length * 0.12, 42);
              const stroke = lit || style.affordance ? COLOR.affordance : COLOR.inkSoft;
              return (
                <path
                  key={`${e.from}-${e.kind}-${e.to}-${i}`}
                  d={edgePath(from, to, curve)}
                  fill="none"
                  stroke={stroke}
                  strokeWidth={lit ? Math.max(style.width, 1.6) : style.width}
                  strokeOpacity={faded ? 0.05 : lit ? 0.6 : style.opacity}
                  strokeDasharray={style.dash}
                  strokeLinecap="round"
                  data-testid="graph-edge"
                  data-kind={e.kind}
                />
              );
            })}
          </g>

          {/* Department hubs — small weighted marks naming each district. */}
          {districts.map((d) => {
            const dept = graph.departments.find((x) => x.id === d.id);
            const tint = tintOf(d.id);
            return (
              <g
                key={d.id}
                transform={`translate(${d.x},${d.y})`}
                opacity={op(d.id)}
                style={{ cursor: "pointer" }}
                onClick={() => setFocusDept((cur) => (cur === d.id ? null : d.id))}
                data-testid="graph-dept"
                data-dept={d.id}
              >
                <circle r={d.hubR} fill={tint.background} stroke={tint.border} strokeWidth={1.5} />
                <text
                  y={d.hubR + lab(13)}
                  textAnchor="middle"
                  fill={COLOR.ink}
                  paintOrder="stroke"
                  stroke={COLOR.paper}
                  strokeWidth={GEOMETRY.graphLabelHalo}
                  style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs), fontWeight: 600 }}
                  data-testid="graph-dept-label"
                >
                  {dept?.label ?? d.id}
                </text>
              </g>
            );
          })}

          {/* Tools / agents — distinct rounded marks, always named small. */}
          {graph.tools.map((t) => {
            const p = at(t.id);
            const tint = t.department_id ? tintOf(t.department_id) : tintOf("");
            const s = GEOMETRY.graphToolSize;
            return (
              <g
                key={t.id}
                transform={`translate(${p.x},${p.y})`}
                opacity={op(t.id)}
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
                />
                <text
                  y={s + lab(11)}
                  textAnchor="middle"
                  fill={COLOR.inkSoft}
                  paintOrder="stroke"
                  stroke={COLOR.paper}
                  strokeWidth={GEOMETRY.graphLabelHalo}
                  style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs) }}
                >
                  {t.label}
                </text>
              </g>
            );
          })}

          {/* Center: the org mark, given presence — halo, weight, monogram. */}
          {(() => {
            const c = at(graph.center.id);
            const r = GEOMETRY.graphCenterRadius;
            return (
              <g data-testid="graph-center">
                <circle cx={c.x} cy={c.y} r={r + 9} fill="none" stroke={COLOR.affordance} strokeWidth={1} opacity={0.35} />
                <circle cx={c.x} cy={c.y} r={r} fill={COLOR.ink} />
                <circle cx={c.x} cy={c.y} r={r} fill="none" stroke={COLOR.paper} strokeWidth={1.5} />
                <text
                  x={c.x}
                  y={c.y + r * 0.32}
                  textAnchor="middle"
                  fill={COLOR.paper}
                  style={{ fontFamily: FONT.chrome, fontSize: r * 0.8, fontWeight: 700 }}
                  data-testid="graph-center-mark"
                >
                  {initialsOf(graph.center.label)}
                </text>
              </g>
            );
          })()}

          {/* People — members first so anchors paint on top. */}
          {[...graph.people]
            .sort((a, b) => (a.ring === b.ring ? 0 : a.ring === "anchor" ? 1 : -1))
            .map((person) => {
              const p = at(person.id);
              const size =
                person.ring === "anchor" ? GEOMETRY.graphAnchorAvatar : GEOMETRY.graphMemberAvatar;
              const tint = tintOf(person.department_id);
              const focused = focus === person.id;
              const showName = nameVisible(person.ring, focused, k);
              const showTitle = person.ring === "anchor" || focused;
              const ringPad = person.ring === "anchor" ? 3 : 2;
              const labelDy = anchorLabelDy.get(person.id) ?? 0;
              const nameY = size / 2 + ringPad + lab(13) + labelDy;
              return (
                <g
                  key={person.id}
                  transform={`translate(${p.x},${p.y})`}
                  opacity={op(person.id)}
                  style={{ cursor: "pointer" }}
                  onMouseEnter={() => setFocus(person.id)}
                  onMouseLeave={() => setFocus(null)}
                  onClick={() => onSelectPerson(person.id)}
                  data-testid="graph-person"
                  data-id={person.id}
                  data-ring={person.ring}
                  data-self={person.is_self ? "true" : "false"}
                >
                  {/* Paper backing + dept-tinted rim — crisp under zoom. */}
                  <circle r={size / 2 + ringPad} fill={COLOR.paper} />
                  <circle
                    r={size / 2 + ringPad}
                    fill="none"
                    stroke={tint.border}
                    strokeWidth={person.ring === "anchor" ? 2 : 1}
                  />
                  {/* "You are here" ring for the acting principal. */}
                  {person.is_self ? (
                    <circle
                      r={size / 2 + ringPad + 4}
                      fill="none"
                      stroke={COLOR.affordance}
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
                  {showName ? (
                    <g>
                      <text
                        y={nameY}
                        textAnchor="middle"
                        fill={COLOR.ink}
                        paintOrder="stroke"
                        stroke={COLOR.paper}
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
                      {showTitle ? (
                        <text
                          y={nameY + lab(12)}
                          textAnchor="middle"
                          fill={COLOR.inkSoft}
                          paintOrder="stroke"
                          stroke={COLOR.paper}
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
        </g>
      </svg>
    </div>
  );
}
