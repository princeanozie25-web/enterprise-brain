"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { forceCollide, forceSimulation, forceX, forceY } from "d3-force";
import { select } from "d3-selection";
import { zoom as d3zoom, zoomIdentity, type ZoomTransform } from "d3-zoom";
import type { GraphResponse } from "@/lib/api";
import { COLOR, DEPARTMENT_TINT, DERIVED, FONT, GEOMETRY, TYPE } from "@/lib/tokens";
import { PersonAvatar } from "./PersonAvatar";

/**
 * THE ORG GRAPH (AR-2) — a calm radial map of the company: the org at center,
 * department hubs on ring 1, the humanized people on ring 2 (anchors
 * prominent, members secondary), tools/agents on ring 3. d3-force computes the
 * people cluster positions (seeded deterministically, then frozen — no
 * perpetual jitter); React owns the DOM and every colour comes from the
 * reserved token palette. HONEST DARK: it draws exactly the nodes in the data
 * — never a ghost node, never a "+N hidden". A small permitted world is a
 * small graph, and that is the truth.
 */
type Pos = { x: number; y: number };

function computeLayout(graph: GraphResponse): Map<string, Pos> {
  const v = GEOMETRY.graphViewport;
  const cx = v / 2;
  const cy = v / 2;
  const pos = new Map<string, Pos>();
  pos.set(graph.center.id, { x: cx, y: cy });

  const deptAngle = new Map<string, number>();
  graph.departments.forEach((d, i) => {
    const a = -Math.PI / 2 + (i / Math.max(graph.departments.length, 1)) * 2 * Math.PI;
    deptAngle.set(d.id, a);
    pos.set(d.id, {
      x: cx + GEOMETRY.graphRingDept * Math.cos(a),
      y: cy + GEOMETRY.graphRingDept * Math.sin(a),
    });
  });

  // People clustered near their department's spoke, on ring 2. Seeded at a
  // deterministic arc position (so the layout is stable across renders), then
  // de-overlapped with d3-force collision + a gentle pull back to the seed.
  type SimNode = { id: string; x: number; y: number; tx: number; ty: number; r: number };
  const sim: SimNode[] = [];
  const byDept = new Map<string, typeof graph.people>();
  for (const p of graph.people) {
    const list = byDept.get(p.department_id) ?? [];
    list.push(p);
    byDept.set(p.department_id, list);
  }
  const arc = ((2 * Math.PI) / Math.max(graph.departments.length, 1)) * 0.9;
  for (const [deptId, ppl] of byDept) {
    const a0 = deptAngle.has(deptId) ? deptAngle.get(deptId)! : -Math.PI / 2;
    const sorted = [...ppl].sort((x, y) =>
      x.ring === y.ring ? x.id.localeCompare(y.id) : x.ring === "anchor" ? -1 : 1,
    );
    const n = sorted.length;
    sorted.forEach((p, i) => {
      const frac = n <= 1 ? 0 : i / (n - 1) - 0.5;
      const a = a0 + frac * arc;
      const ring = GEOMETRY.graphRingPeople + (p.ring === "member" ? 24 * ((i % 3) - 1) : -18);
      const tx = cx + ring * Math.cos(a);
      const ty = cy + ring * Math.sin(a);
      const r =
        (p.ring === "anchor" ? GEOMETRY.graphAnchorAvatar : GEOMETRY.graphMemberAvatar) / 2 + 5;
      sim.push({ id: p.id, x: tx, y: ty, tx, ty, r });
    });
  }
  forceSimulation(sim as never[])
    .force("collide", forceCollide<SimNode>((d) => d.r))
    .force("x", forceX<SimNode>((d) => d.tx).strength(0.14))
    .force("y", forceY<SimNode>((d) => d.ty).strength(0.14))
    .stop()
    .tick(GEOMETRY.graphForceTicks);
  for (const node of sim) {
    pos.set(node.id, { x: node.x, y: node.y });
  }

  graph.tools.forEach((t, i) => {
    const base =
      t.department_id !== undefined && deptAngle.has(t.department_id)
        ? deptAngle.get(t.department_id)!
        : -Math.PI / 2 + (i / Math.max(graph.tools.length, 1)) * 2 * Math.PI;
    const jitter = (i % 2 === 0 ? 1 : -1) * 0.07 * (Math.floor(i / 2) + 1);
    const a = base + jitter;
    pos.set(t.id, {
      x: cx + GEOMETRY.graphRingTools * Math.cos(a),
      y: cy + GEOMETRY.graphRingTools * Math.sin(a),
    });
  });
  return pos;
}

function tintOf(label: string): { background: string; border: string } {
  return DEPARTMENT_TINT[label] ?? { background: DERIVED.wash, border: DERIVED.hairline };
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
  const pos = useMemo(() => computeLayout(graph), [graph]);
  const [focus, setFocus] = useState<string | null>(null);
  const [focusDept, setFocusDept] = useState<string | null>(null);
  const [transform, setTransform] = useState<ZoomTransform>(zoomIdentity);
  const svgRef = useRef<SVGSVGElement | null>(null);

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

  // d3-zoom: pan/zoom on the whole scene. React holds the transform.
  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    try {
      const behavior = d3zoom<SVGSVGElement, unknown>()
        .scaleExtent([0.4, 4])
        .on("zoom", (event) => setTransform(event.transform));
      const sel = select(svg);
      sel.call(behavior);
      return () => {
        sel.on(".zoom", null);
      };
    } catch {
      // Pan/zoom is an enhancement; the static graph stands without it.
      return;
    }
  }, []);

  const peopleById = useMemo(
    () => new Map(graph.people.map((p) => [p.id, p])),
    [graph.people],
  );
  const deptById = useMemo(
    () => new Map(graph.departments.map((d) => [d.id, d])),
    [graph.departments],
  );

  const v = GEOMETRY.graphViewport;
  const at = (id: string): Pos => pos.get(id) ?? { x: v / 2, y: v / 2 };

  // Which ids are emphasized given the current focus (hover person or dept).
  const emphasized = (id: string): boolean => {
    if (focus !== null) return id === focus || (neighbors.get(focus)?.has(id) ?? false);
    if (focusDept !== null) {
      if (id === focusDept) return true;
      const p = peopleById.get(id);
      return p ? p.department_id === focusDept : false;
    }
    return true;
  };
  const dim = focus !== null || focusDept !== null;
  const op = (id: string) => (dim && !emphasized(id) ? GEOMETRY.graphDimOpacity : 1);

  const reset = () => {
    setFocus(null);
    setFocusDept(null);
    setTransform(zoomIdentity);
    if (svgRef.current) {
      try {
        select(svgRef.current).call(d3zoom<SVGSVGElement, unknown>().transform, zoomIdentity);
      } catch {
        /* no-op */
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
        viewBox={`0 0 ${v} ${v}`}
        className={`w-full ${reducedMotion ? "ap-fade-view" : "ap-fade-view"}`}
        style={{ maxHeight: "78vh", touchAction: "none", cursor: "grab" }}
        role="img"
        aria-label="Organization graph"
      >
        <g transform={transform.toString()}>
          {/* Edges first (behind every node). */}
          <g data-testid="graph-edges">
            {graph.edges.map((e, i) => {
              const from = at(e.from);
              const to = at(e.to);
              const lit = focus !== null && (e.from === focus || e.to === focus);
              const faded = dim && !lit;
              return (
                <line
                  key={`${e.from}-${e.kind}-${e.to}-${i}`}
                  x1={from.x}
                  y1={from.y}
                  x2={to.x}
                  y2={to.y}
                  stroke={lit ? COLOR.affordance : COLOR.inkSoft}
                  strokeWidth={lit ? 1.5 : 1}
                  strokeOpacity={faded ? 0.05 : lit ? 0.5 : 0.18}
                  data-testid="graph-edge"
                  data-kind={e.kind}
                />
              );
            })}
          </g>

          {/* Center: the org mark. */}
          {(() => {
            const c = at(graph.center.id);
            return (
              <g data-testid="graph-center">
                <circle cx={c.x} cy={c.y} r={GEOMETRY.graphCenterRadius} fill={COLOR.ink} />
                <text
                  x={c.x}
                  y={c.y + 4}
                  textAnchor="middle"
                  fill={COLOR.paper}
                  style={{ fontFamily: FONT.chrome, fontSize: TYPE.scale.md, fontWeight: 600 }}
                >
                  {graph.center.label.slice(0, 1)}
                </text>
              </g>
            );
          })()}

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
                onClick={() => setFocusDept((cur) => (cur === d.id ? null : d.id))}
                data-testid="graph-dept"
                data-dept={d.id}
              >
                <circle
                  r={GEOMETRY.graphDeptRadius}
                  fill={tint.background}
                  stroke={tint.border}
                  strokeWidth={1.5}
                />
                <text
                  y={GEOMETRY.graphDeptRadius + 13}
                  textAnchor="middle"
                  fill={COLOR.ink}
                  style={{ fontFamily: FONT.chrome, fontSize: TYPE.scale.xs, fontWeight: 600 }}
                  data-testid="graph-dept-label"
                >
                  {d.label}
                </text>
              </g>
            );
          })}

          {/* Tools / agents (outer ring). */}
          {graph.tools.map((t) => {
            const p = at(t.id);
            const tint = t.department_id ? tintOf(t.department_id) : tintOf("");
            return (
              <g key={t.id} transform={`translate(${p.x},${p.y})`} opacity={op(t.id)} data-testid="graph-tool">
                <rect
                  x={-GEOMETRY.graphToolRadius}
                  y={-GEOMETRY.graphToolRadius}
                  width={GEOMETRY.graphToolRadius * 2}
                  height={GEOMETRY.graphToolRadius * 2}
                  rx={3}
                  fill={tint.background}
                  stroke={tint.border}
                  strokeWidth={1}
                />
                {focus === t.id || focusDept === t.department_id ? (
                  <text
                    y={GEOMETRY.graphToolRadius + 11}
                    textAnchor="middle"
                    fill={COLOR.inkSoft}
                    style={{ fontFamily: FONT.chrome, fontSize: TYPE.scale.xs }}
                  >
                    {t.label}
                  </text>
                ) : null}
              </g>
            );
          })}

          {/* People — members first so anchors paint on top. */}
          {[...graph.people]
            .sort((a, b) => (a.ring === b.ring ? 0 : a.ring === "anchor" ? 1 : -1))
            .map((person) => {
              const p = at(person.id);
              const size =
                person.ring === "anchor"
                  ? GEOMETRY.graphAnchorAvatar
                  : GEOMETRY.graphMemberAvatar;
              const showName = person.ring === "anchor" || focus === person.id;
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
                  {/* "You are here" ring for the acting principal. */}
                  {person.is_self ? (
                    <circle
                      r={size / 2 + 5}
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
                        y={size / 2 + 13}
                        textAnchor="middle"
                        fill={COLOR.ink}
                        style={{
                          fontFamily: FONT.chrome,
                          fontSize: TYPE.scale.xs,
                          fontWeight: person.ring === "anchor" ? 600 : 500,
                        }}
                        data-testid="graph-person-name"
                      >
                        {person.display_name}
                      </text>
                      {person.ring === "anchor" || focus === person.id ? (
                        <text
                          y={size / 2 + 25}
                          textAnchor="middle"
                          fill={COLOR.inkSoft}
                          style={{ fontFamily: FONT.chrome, fontSize: TYPE.scale.xs }}
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
