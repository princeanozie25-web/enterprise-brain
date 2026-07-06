"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { select } from "d3-selection";
import { zoom as d3zoom, zoomIdentity, type ZoomBehavior, type ZoomTransform } from "d3-zoom";
import type { GraphEdge, GraphPerson, GraphProject, GraphResponse } from "@/lib/api";
import { FONT, GEOMETRY, RADIUS, TYPE, graphRampStep } from "@/lib/tokens";
import { peoplePlural } from "./graphDisplay";
import { PersonAvatar } from "./PersonAvatar";

type Pos = { x: number; y: number };
export type GraphNodeKind = "org" | "department" | "human" | "agent" | "source" | "project";
export type SelectedNode = { id: string; kind: GraphNodeKind; label: string };

type DeptArc = { start: number; end: number; center: number; count: number };
type MemberSlot = { angle: number; orbit: number; hubId: string };
type Layout = {
  pos: Map<string, Pos>;
  ang: Map<string, number>;
  hubAngle: Map<string, number>;
  deptArcs: Map<string, DeptArc>;
  memberSlot: Map<string, MemberSlot>;
  /** Departments whose people render as an honest cluster CHIP instead of a
   * fan: every department past the 40-person threshold, plus any single
   * department whose fan would need a third orbit row — member rows may
   * never reach through ring 2 (the tier law), so arcs shrink, rows
   * overflow ONCE, and past that the chip is the honest representation. */
  chipDepts: Set<string>;
  bound: number;
};

/**
 * THE LAYOUT LAW (graph-presence pass, A1) — radial department clustering:
 * - CENTER: the organization node, 56px.
 * - RING 1: department hubs at 240px, evenly distributed, 48px circles,
 *   name + member count beneath (13px/500).
 * - MEMBER CLUSTERS: each person (36px) orbits its OWN hub at 96px, fanned
 *   in an arc facing outward from center (span ∝ member count, max 300°);
 *   ≥24px between sibling node edges; 8px cluster-to-cluster padding —
 *   arcs shrink (and overflow to a deterministic second orbit row at +60px)
 *   before hubs ever move. The overflow row is a resolved conflict, flagged
 *   in the closeout: >9 members cannot satisfy both the 24px gap and the
 *   300° cap on a single 96px orbit.
 * - PROJECTS: 24px nodes at 140px, ONLY those the payload links via edges —
 *   unlinked projects stay a masthead count, never invented placement.
 * - RING 2: systems/sources at 420px, 28px nodes, resting at 80% opacity.
 *   AGENTS are a payload kind this law does not tier (flagged): they ride
 *   the same periphery ring with their existing icon treatment.
 * - Depth = scale + opacity only. No blur, no glass, no new colors. The
 *   whole layout scales to fit (viewBox = computed bounding radius).
 * Every rendered node maps to a REAL payload entity; every rendered edge
 * exists in the payload (F6 law, re-pinned). Departments are coded ONLY by
 * the sensitivity-safe NEUTRAL RAMP; amber marks the lit connection path
 * and nothing else; selection/focus rings are the interactive ink-blue.
 */
const STAGE = {
  /** Center org node: 56px square. */
  coreSize: 56,
  /** Ring 1: department hubs. */
  hubRing: 240,
  hubRadius: 24,
  /** Member fans. */
  memberOrbit: 96,
  memberRowGap: 60,
  personNode: 36,
  memberGap: 24,
  clusterPad: 8,
  fanMaxSpan: (300 * Math.PI) / 180,
  /** Projects (payload-linked only). */
  projectRing: 140,
  projectSize: 12,
  /** Ring 2: systems of record (+ the unplaced agent kind, periphery). */
  sourceRing: 420,
  sourceRadius: 14,
  agentSize: 7,
  /** Ring 2 rests at 80% opacity (depth via opacity, never blur). */
  ring2RestOpacity: 0.8,
  /** Cluster chips (>40 people): a quiet rounded-lg chip per department. */
  clusterWidth: 148,
  clusterHeight: 44,
  /** Label breathing room inside the computed bounding radius. */
  boundMargin: 84,
} as const;

/** The people tier collapses to department clusters past this count. */
export const PEOPLE_CLUSTER_THRESHOLD = 40;

/** A2: structural edges rest at 1px/35%; the focused node's edge set lights
 * at 1.5px/100%; everything non-connected dims to 15% (GEOMETRY token). */
const EDGE_REST = { width: 1, opacity: 0.35 } as const;
const EDGE_LIT = { width: 1.5, opacity: 1 } as const;

const C = {
  ink: "var(--ink)",
  paper: "var(--paper)",
  inkSoft: "var(--ink-soft)",
  affordance: "var(--affordance)",
  warm: "var(--accent-warm)",
  hairline: "var(--hairline)",
  wash: "var(--wash)",
};

function polar(angle: number, radius: number, cx = 0, cy = 0): Pos {
  // Rounded to 0.01px: stable SVG attributes (never scientific notation)
  // with sub-pixel accuracy the layout laws' ±1px tolerances don't feel.
  return {
    x: Math.round((cx + radius * Math.cos(angle)) * 100) / 100,
    y: Math.round((cy + radius * Math.sin(angle)) * 100) / 100,
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

/** Minimum angular step (radians) between sibling centers on an orbit of
 * radius `r`, so node EDGES keep the 24px law: chord ≥ node + gap = 60px. */
function minStep(r: number): number {
  const chord = STAGE.personNode + STAGE.memberGap;
  return 2 * Math.asin(Math.min(1, chord / 2 / r));
}

/** The lateral half-extent one cluster may occupy before it crosses the
 * midline to its neighbor (8px padding, split): shrink arcs, never hubs. */
function lateralLimit(hubCount: number): number {
  if (hubCount <= 1) return Number.POSITIVE_INFINITY;
  const neighborDistance = 2 * STAGE.hubRing * Math.sin(Math.PI / hubCount);
  return neighborDistance / 2 - STAGE.personNode / 2 - STAGE.clusterPad / 2;
}

/** A wrapped fan may not intrude on the projects band: members keep this
 * distance from the CENTER (projects ring + node + label room). */
const INNER_CLEARANCE = STAGE.projectRing + STAGE.projectSize + 32;

/** Span cap for an orbit row: the 300° law, shrunk when the row would
 * collide with a neighbor cluster OR wrap into the projects band — arcs
 * shrink (then overflow to the next row); hubs never move. */
function rowSpanCap(orbit: number, hubCount: number, hasInnerTier: boolean): number {
  let cap = STAGE.fanMaxSpan;
  const limit = lateralLimit(hubCount);
  if (orbit > limit) {
    cap = Math.min(cap, 2 * Math.asin(Math.max(0, Math.min(1, limit / orbit))));
  }
  if (hasInnerTier) {
    // dist(center, member)² = hubRing² + orbit² + 2·hubRing·orbit·cos(θ),
    // θ measured from the outward direction; keep dist ≥ INNER_CLEARANCE.
    const cosLimit =
      (INNER_CLEARANCE * INNER_CLEARANCE - STAGE.hubRing * STAGE.hubRing - orbit * orbit) /
      (2 * STAGE.hubRing * orbit);
    if (cosLimit > -1 && cosLimit < 1) {
      cap = Math.min(cap, 2 * Math.acos(cosLimit));
    }
  }
  return cap;
}

/** Member fans may use at most two orbit rows (96, 156): the outermost
 * member tip (240+156+18 = 414) stays inside ring 2 (420). A third row
 * would cross it, so a fan that cannot seat everyone in two rows becomes
 * a chip instead. */
const MEMBER_ROWS_MAX = 2;

function computeLayout(graph: GraphResponse): Layout {
  const pos = new Map<string, Pos>();
  const ang = new Map<string, number>();
  const hubAngle = new Map<string, number>();
  const deptArcs = new Map<string, DeptArc>();
  const memberSlot = new Map<string, MemberSlot>();
  const chipDepts = new Set<string>();
  const clustered = graph.people.length > PEOPLE_CLUSTER_THRESHOLD;
  pos.set(graph.center.id, { x: 0, y: 0 });

  const peopleByDept = new Map<string, GraphPerson[]>();
  for (const person of graph.people) {
    const list = peopleByDept.get(person.department_id) ?? [];
    list.push(person);
    peopleByDept.set(person.department_id, list);
  }

  // RING 1 — hubs evenly distributed, starting at 12 o'clock.
  const hubCount = Math.max(graph.departments.length, 1);
  let maxMemberReach = 0;
  graph.departments.forEach((dept, index) => {
    const angle = -Math.PI / 2 + (index / hubCount) * 2 * Math.PI;
    hubAngle.set(dept.id, angle);
    pos.set(dept.id, polar(angle, STAGE.hubRing));
  });

  // Projects are placed only when the payload links them (edges); the
  // member fans need to know whether that inner tier exists at all.
  const linked = new Set<string>();
  for (const edge of graph.edges) {
    linked.add(edge.from);
    linked.add(edge.to);
  }
  const linkedProjects = graph.projects.filter((project) => linked.has(project.id));

  // MEMBER FANS — outward-facing, min-gap spacing, deterministic row
  // overflow. A department that cannot seat its members within the two
  // lawful rows (or any department in a >40-person world) renders the
  // honest cluster chip instead — geometry is computed ONLY for what
  // actually draws, so the scale-to-fit bound is never inflated by
  // invisible nodes.
  const hasInnerTier = linkedProjects.length > 0;
  const rowCapacity = (row: number): number => {
    const orbit = STAGE.memberOrbit + row * STAGE.memberRowGap;
    const step = minStep(orbit);
    const cap = rowSpanCap(orbit, hubCount, hasInnerTier);
    return Math.max(1, Math.floor(cap / step) + 1);
  };
  const fanCapacity = Array.from({ length: MEMBER_ROWS_MAX }, (_, row) => rowCapacity(row)).reduce(
    (a, b) => a + b,
    0,
  );

  for (const dept of graph.departments) {
    const list = [...(peopleByDept.get(dept.id) ?? [])].sort((a, b) =>
      a.ring === b.ring ? a.id.localeCompare(b.id) : a.ring === "anchor" ? -1 : 1,
    );
    const hubA = hubAngle.get(dept.id) ?? -Math.PI / 2;
    const hubPos = pos.get(dept.id)!;
    deptArcs.set(dept.id, { start: hubA, end: hubA, center: hubA, count: list.length });

    if (clustered || list.length > fanCapacity) {
      if (list.length > 0) chipDepts.add(dept.id);
      if (list.length > 0) {
        maxMemberReach = Math.max(
          maxMemberReach,
          STAGE.hubRing + STAGE.memberOrbit + STAGE.clusterHeight / 2,
        );
      }
      continue;
    }

    let placed = 0;
    let row = 0;
    let fanStart = hubA;
    let fanEnd = hubA;
    while (placed < list.length) {
      const orbit = STAGE.memberOrbit + row * STAGE.memberRowGap;
      const step = minStep(orbit);
      const capacity = rowCapacity(row);
      const rowMembers = list.slice(placed, placed + capacity);
      const span = (rowMembers.length - 1) * step;
      rowMembers.forEach((person, i) => {
        const angle = rowMembers.length === 1 ? hubA : hubA - span / 2 + i * step;
        const p = polar(angle, orbit, hubPos.x, hubPos.y);
        pos.set(person.id, p);
        ang.set(person.id, angle);
        memberSlot.set(person.id, { angle, orbit, hubId: dept.id });
        fanStart = Math.min(fanStart, angle);
        fanEnd = Math.max(fanEnd, angle);
      });
      maxMemberReach = Math.max(maxMemberReach, STAGE.hubRing + orbit + STAGE.personNode / 2);
      placed += rowMembers.length;
      row += 1;
    }
    deptArcs.set(dept.id, { start: fanStart, end: fanEnd, center: hubA, count: list.length });
  }

  // PROJECTS — only those the payload links via edges (no invented placement).
  linkedProjects.forEach((project, index) => {
    const angle = -Math.PI / 2 + (index / Math.max(linkedProjects.length, 1)) * 2 * Math.PI;
    pos.set(project.id, polar(angle, STAGE.projectRing));
    ang.set(project.id, angle);
  });

  // RING 2 — systems of record + the unplaced agent kind (periphery),
  // evenly interleaved from 12 o'clock.
  const ring2: Array<{ id: string }> = [...graph.sources, ...graph.tools];
  ring2.forEach((item, index) => {
    const angle = -Math.PI / 2 + (index / Math.max(ring2.length, 1)) * 2 * Math.PI;
    pos.set(item.id, polar(angle, STAGE.sourceRing));
    ang.set(item.id, angle);
  });

  // Scale-to-fit: the bounding radius covers the widest tier that actually
  // DRAWS, plus label room; the viewBox derives from it, so nothing ever
  // clips and nothing invisible inflates the frame.
  const bound =
    Math.max(
      ring2.length > 0 ? STAGE.sourceRing + STAGE.sourceRadius : 0,
      maxMemberReach,
      STAGE.hubRing + STAGE.hubRadius,
      linkedProjects.length > 0 ? STAGE.projectRing + STAGE.projectSize : 0,
    ) + STAGE.boundMargin;

  return { pos, ang, hubAngle, deptArcs, memberSlot, chipDepts, bound };
}

function edgePath(from: Pos, to: Pos, curve: number): string {
  if (curve === 0) return `M${from.x},${from.y}L${to.x},${to.y}`;
  const mx = (from.x + to.x) / 2;
  const my = (from.y + to.y) / 2;
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const len = Math.hypot(dx, dy) || 1;
  return `M${from.x},${from.y}Q${mx + (-dy / len) * curve},${my + (dx / len) * curve} ${to.x},${to.y}`;
}

/** Edge KIND rides dash pattern, never extra hue. */
const EDGE_DASH: Record<string, string | undefined> = {
  reports_to: undefined,
  member_of: undefined,
  owns_agent: "2 5",
  system_of: "1 8",
  works_on: "1 7",
  involves_department: "2 7",
  uses: "1 8",
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
  const { pos, ang, hubAngle, deptArcs, memberSlot, chipDepts, bound } = useMemo(
    () => computeLayout(graph),
    [graph],
  );
  const [hover, setHover] = useState<string | null>(null);
  const [transform, setTransform] = useState<ZoomTransform>(zoomIdentity);
  const svgRef = useRef<SVGSVGElement | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const zoomRef = useRef<ZoomBehavior<SVGSVGElement, unknown> | null>(null);
  const nodeRefs = useRef<Map<string, SVGGElement>>(new Map());

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

  /** Members per hub, sorted exactly as the layout placed them. */
  const membersByDept = useMemo(() => {
    const map = new Map<string, GraphPerson[]>();
    for (const dept of graph.departments) map.set(dept.id, []);
    const sorted = [...graph.people].sort((a, b) =>
      a.ring === b.ring ? a.id.localeCompare(b.id) : a.ring === "anchor" ? -1 : 1,
    );
    for (const person of sorted) {
      const list = map.get(person.department_id) ?? [];
      list.push(person);
      map.set(person.department_id, list);
    }
    return map;
  }, [graph.departments, graph.people]);

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
        const focusK = 1.7;
        const frame = polar(angle, STAGE.hubRing + STAGE.memberOrbit / 2);
        const t = zoomIdentity.translate(-focusK * frame.x, -focusK * frame.y).scale(focusK);
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
  const at = (id: string): Pos => pos.get(id) ?? { x: 0, y: 0 };

  /** A4: the ONE new choreographed motion — the focus/dim transition,
   * 180ms ease-out (≤200ms law), fired only by user action, DEAD under
   * prefers-reduced-motion (instant state swap, no tween). */
  const nodeTransition = reducedMotion ? undefined : "opacity 180ms ease-out";
  const edgeTransition = reducedMotion
    ? undefined
    : "opacity 180ms ease-out, stroke-opacity 180ms ease-out, stroke-width 180ms ease-out";

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

  /** A2: the EGO — hover mirrors keyboard focus (onFocus sets hover); a
   * selection persists the ego until Escape or click-away, for EVERY node
   * kind (the center included — its edge set is the org's spine). A chip's
   * synthetic `cluster:X` id resolves to its department, so the neighbors
   * map (payload-keyed) always has an answer and the chip itself stays
   * emphasized. */
  const rawEgo = hover ?? selectedId;
  const ego =
    rawEgo !== null && rawEgo.startsWith("cluster:") ? rawEgo.slice("cluster:".length) : rawEgo;
  const egoNeighbors = ego !== null ? neighbors.get(ego) : undefined;
  const traceRelated = (id: string): boolean =>
    ego !== null && (id === ego || (egoNeighbors?.has(id) ?? false));
  const dimming = focusDept !== null || ego !== null || q.length > 0;
  const emphasized = (id: string): boolean => {
    if (focusDept !== null) return inDept(id) || id === graph.center.id;
    if (ego !== null) return traceRelated(id);
    if (q.length > 0) return matches(id);
    return true;
  };
  const op = (id: string): number => {
    if (!dimming || emphasized(id)) return 1;
    return focusDept !== null ? GEOMETRY.graphGhostOpacity : GEOMETRY.graphDimOpacity;
  };
  /** Ring 2 rests at 80%; dimming laws still apply beneath it. */
  const ring2Op = (id: string): number => Math.min(op(id), dimming && emphasized(id) ? 1 : STAGE.ring2RestOpacity);

  const projectVisible = (project: GraphProject): boolean =>
    !hidden.has("projects") && pos.has(project.id);

  /** A2 staged edges: structural person→hub edges are the REST state; every
   * other payload edge (a relationship chord) draws only when the ego's
   * edge set lights it (or in department focus). Totals stay honest in the
   * masthead — the map never claims fewer relationships than it disclosed. */
  const isStructural = (edge: GraphEdge): boolean =>
    edge.kind === "member_of" && peopleById.has(edge.from) && deptById.has(edge.to);
  const touchesEgo = (edge: GraphEdge): boolean =>
    ego !== null && (edge.from === ego || edge.to === ego);
  const litInFocusDept = (edge: GraphEdge): boolean =>
    focusDept !== null && inDept(edge.from) && inDept(edge.to);

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

  /** A person renders only when their department fans out; a chip-dept
   * member has no position, so their edges must not draw either. */
  const personUnfanned = (id: string): boolean => peopleById.has(id) && !memberSlot.has(id);
  const edgeHidden = (edge: GraphEdge): boolean =>
    personUnfanned(edge.from) ||
    personUnfanned(edge.to) ||
    (hidden.has("people") && (peopleById.has(edge.from) || peopleById.has(edge.to))) ||
    (hidden.has("agents") && (toolsById.has(edge.from) || toolsById.has(edge.to))) ||
    (hidden.has("sources") && (sourcesById.has(edge.from) || sourcesById.has(edge.to))) ||
    (projectsById.has(edge.from) && !projectVisible(projectsById.get(edge.from)!)) ||
    (projectsById.has(edge.to) && !projectVisible(projectsById.get(edge.to)!));

  // -------------------------------------------------------------------------
  // KEYBOARD OPERABILITY (A5 / WCAG 2.1.1): tab order IS the cluster order —
  // center → each hub → its members → next hub → ring 2 → projects (the DOM
  // renders in that order). Arrow keys traverse WITHIN the current tier;
  // Escape climbs a tier (member → hub → root) and releases a persisted
  // selection. Focus mirrors hover, so the A2 ego visuals double as the
  // focus cue on top of the standard focus ring.
  // -------------------------------------------------------------------------
  const visibleProjects = graph.projects.filter(projectVisible);
  const tiers: string[][] = useMemo(() => {
    const hubTier = graph.departments.map((d) => d.id);
    // The people tier honors the People filter in BOTH representations:
    // hiding people removes fans AND chips alike (no silent no-op).
    const memberTiers: string[][] = hidden.has("people")
      ? []
      : graph.departments.map((d) =>
          chipDepts.has(d.id)
            ? [`cluster:${d.id}`]
            : (membersByDept.get(d.id) ?? []).map((p) => p.id),
        );
    const ring2Tier = [
      ...(hidden.has("sources") ? [] : graph.sources.map((s) => s.id)),
      ...(hidden.has("agents") ? [] : graph.tools.map((t) => t.id)),
    ];
    const projectTier = visibleProjects.map((p) => p.id);
    return [[graph.center.id], hubTier, ...memberTiers, ring2Tier, projectTier].filter(
      (tier) => tier.length > 0,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [graph, hidden, chipDepts, membersByDept, visibleProjects.length]);

  const tierOf = (id: string): string[] | undefined => tiers.find((tier) => tier.includes(id));

  const registerNode = (id: string) => (el: SVGGElement | null) => {
    if (el) nodeRefs.current.set(id, el);
    else nodeRefs.current.delete(id);
  };

  /** Arrows: circular traversal WITHIN the node's tier (A5). */
  const moveFocus = (fromId: string, delta: 1 | -1) => {
    const tier = tierOf(fromId);
    if (!tier) return;
    const index = tier.indexOf(fromId);
    const next = tier[(index + delta + tier.length) % tier.length];
    nodeRefs.current.get(next)?.focus();
  };

  /** Escape: release a persisted selection, then climb a tier —
   * member → its hub; everything else → the graph root. */
  const escapeFrom = (id: string) => {
    if (selectedId !== null) onSelectNode(null);
    const slot = memberSlot.get(id);
    const clusterDept = id.startsWith("cluster:") ? id.slice("cluster:".length) : null;
    const hubId = slot?.hubId ?? clusterDept;
    if (hubId !== null && hubId !== undefined && nodeRefs.current.has(hubId)) {
      nodeRefs.current.get(hubId)?.focus();
    } else {
      rootRef.current?.focus();
    }
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
        escapeFrom(id);
      }
    },
  });

  /** A2: click-away on the stage background releases a persisted selection. */
  const onStageClick = (event: React.MouseEvent<SVGSVGElement>) => {
    if (event.target === event.currentTarget && selectedId !== null) {
      onSelectNode(null);
    }
  };

  // Radial member label placement: names stay 13px/500 (the label law), fanned
  // outward from the hub along each member's own direction so sibling names
  // do not stack.
  const memberLabel = (personId: string) => {
    const slot = memberSlot.get(personId);
    const direction = slot?.angle ?? ang.get(personId) ?? 0;
    // Overflow-row labels sit further out along their own radial so the two
    // rows' names never share a screen position on the fan's flanks.
    const rowBump = slot && slot.orbit > STAGE.memberOrbit ? lab(14) : 0;
    const offset = STAGE.personNode / 2 + lab(10) + rowBump;
    const lx = Math.cos(direction) * offset;
    const ly = Math.sin(direction) * offset;
    const anchor: "start" | "end" | "middle" =
      Math.cos(direction) > 0.34 ? "start" : Math.cos(direction) < -0.34 ? "end" : "middle";
    const dy = anchor === "middle" ? (Math.sin(direction) >= 0 ? lab(14) : -lab(8)) : lab(4);
    return { lx, ly: ly + dy, anchor };
  };

  const renderPerson = (person: GraphPerson) => {
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
    const activate = () => onSelectNode({ id: person.id, kind: "human", label: person.display_name });
    const label = memberLabel(person.id);
    return (
      <g
        key={person.id}
        transform={`translate(${point.x},${point.y})`}
        opacity={op(person.id)}
        style={{ cursor: "pointer", transition: nodeTransition }}
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
        {/* The full name, ALWAYS: 13px/500 (the label law), fanned outward. */}
        <g transform={`translate(${label.lx},${label.ly})`}>
          <text
            textAnchor={label.anchor}
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
              textAnchor={label.anchor}
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
  };

  const renderClusterChip = (deptId: string) => {
    const dept = deptById.get(deptId);
    const arc = deptArcs.get(deptId);
    if (!dept || !arc || arc.count === 0) return null;
    const hubPos = at(deptId);
    const direction = hubAngle.get(deptId) ?? 0;
    const point = polar(direction, STAGE.memberOrbit, hubPos.x, hubPos.y);
    const ramp = rampOf(deptId);
    const clusterId = `cluster:${deptId}`;
    const active = focusDept === deptId || hover === clusterId || matches(deptId);
    const activate = () => {
      onFocusDept(focusDept === deptId ? null : deptId);
      onSelectNode({ id: deptId, kind: "department", label: dept.label });
    };
    return (
      <g
        key={clusterId}
        transform={`translate(${point.x},${point.y})`}
        opacity={op(deptId)}
        style={{ cursor: "pointer", transition: nodeTransition }}
        onMouseEnter={() => setHover(clusterId)}
        onMouseLeave={() => setHover(null)}
        onClick={activate}
        data-testid="graph-people-cluster"
        data-id={deptId}
        data-count={arc.count}
        {...nodeKeyProps(clusterId, `${dept.label}: ${peoplePlural(arc.count)} in scope`, activate)}
      >
        <title>{`${dept.label}: ${peoplePlural(arc.count)} in scope`}</title>
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
          {`${peoplePlural(arc.count)} in scope`}
        </text>
      </g>
    );
  };

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

      {/* The visually-hidden mirror (A5 / WCAG 4.1.2), REGROUPED by
          department: one list per hub, then ring 2, then projects — the same
          rendered nodes as plain text, real entities only. */}
      <div className="sr-only" data-testid="graph-sr-mirror">
        <p>{graph.center.label} — organization</p>
        {graph.departments.map((dept) => {
          const count = deptArcs.get(dept.id)?.count ?? 0;
          return (
            <ul key={dept.id} aria-label={`${dept.label} department`} data-testid="graph-sr-dept">
              <li>
                {dept.label} — department — {peoplePlural(count)} in scope
              </li>
              {/* The mirror is textual, so it names every member even when
                  the visual collapses that department to a chip. */}
              {!hidden.has("people") &&
                (membersByDept.get(dept.id) ?? []).map((person) => (
                  <li key={person.id}>
                    {person.display_name} — {person.title}
                  </li>
                ))}
            </ul>
          );
        })}
        {(!hidden.has("sources") || !hidden.has("agents")) && (
          <ul aria-label="Systems and agents">
            {!hidden.has("sources") &&
              graph.sources.map((source) => <li key={source.id}>{source.label} — system of record</li>)}
            {!hidden.has("agents") &&
              graph.tools.map((tool) => <li key={tool.id}>{tool.label} — agent</li>)}
          </ul>
        )}
        {visibleProjects.length > 0 && (
          <ul aria-label="Projects">
            {visibleProjects.map((project) => (
              <li key={project.id}>
                {compactProjectLabel(project.label)} — project — {peoplePlural(project.people)}
              </li>
            ))}
          </ul>
        )}
      </div>

      <svg
        ref={svgRef}
        viewBox={`${-bound} ${-bound} ${bound * 2} ${bound * 2}`}
        className={`block h-full w-full${reducedMotion ? "" : " ap-fade-view"}`}
        style={{ touchAction: "none", cursor: "grab" }}
        role="group"
        aria-label="Organization graph"
        onClick={onStageClick}
      >
        <g transform={transform.toString()} data-testid="graph-scene">
          <g data-testid="graph-edges">
            {graph.edges.map((edge, index) => {
              if (edgeHidden(edge)) return null;
              const structural = isStructural(edge);
              const lit = touchesEgo(edge) || litInFocusDept(edge);
              // A2: chords exist ONLY on focus; structural edges are the
              // rest state, dimming (never vanishing) when an ego lights.
              if (!structural && !lit) return null;
              const from = at(edge.from);
              const to = at(edge.to);
              const dx = to.x - from.x;
              const dy = to.y - from.y;
              const curve = structural
                ? 0
                : (edge.from < edge.to ? 1 : -1) * Math.min(Math.hypot(dx, dy) * 0.08, 44);
              const dimmed = dimming && !lit;
              return (
                <path
                  key={`${edge.from}-${edge.kind}-${edge.to}-${index}`}
                  d={edgePath(from, to, curve)}
                  fill="none"
                  stroke={lit ? C.warm : C.inkSoft}
                  strokeWidth={lit ? EDGE_LIT.width : EDGE_REST.width}
                  strokeOpacity={
                    lit ? EDGE_LIT.opacity : dimmed ? GEOMETRY.graphDimOpacity : EDGE_REST.opacity
                  }
                  strokeDasharray={EDGE_DASH[edge.kind]}
                  strokeLinecap="round"
                  style={{ transition: edgeTransition }}
                  data-testid="graph-edge"
                  data-kind={edge.kind}
                  data-from={edge.from}
                  data-to={edge.to}
                  data-structural={structural ? "true" : "false"}
                  data-lit={lit ? "true" : "false"}
                />
              );
            })}
          </g>

          <g data-testid="graph-rings" opacity={focusDept !== null ? 0.42 : 1}>
            {[STAGE.projectRing, STAGE.hubRing, STAGE.sourceRing].map((radius) => (
              <circle
                key={radius}
                cx={0}
                cy={0}
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

          {/* CENTER — first tab stop (A5). */}
          {(() => {
            const point = at(graph.center.id);
            const half = STAGE.coreSize / 2;
            const active =
              selectedId === graph.center.id || traceRelated(graph.center.id) || hover === graph.center.id;
            const activate = () =>
              onSelectNode({ id: graph.center.id, kind: "org", label: graph.center.label });
            return (
              <g
                data-testid="graph-center"
                data-id={graph.center.id}
                style={{ cursor: "pointer", transition: nodeTransition }}
                opacity={op(graph.center.id)}
                onMouseEnter={() => setHover(graph.center.id)}
                onMouseLeave={() => setHover(null)}
                onClick={activate}
                {...nodeKeyProps(graph.center.id, `${graph.center.label}, organization`, activate)}
              >
                <title>{graph.center.label}</title>
                <rect
                  x={point.x - half}
                  y={point.y - half}
                  width={STAGE.coreSize}
                  height={STAGE.coreSize}
                  rx={8}
                  fill={C.paper}
                  stroke={active ? C.affordance : C.hairline}
                  strokeWidth={active ? 2 : 1.4}
                />
                <text
                  x={point.x}
                  y={point.y + half * 0.34}
                  textAnchor="middle"
                  fill={C.ink}
                  style={{ fontFamily: FONT.chrome, fontSize: half * 0.78, fontWeight: 800 }}
                  data-testid="graph-center-mark"
                >
                  {monogram(graph.center.label)}
                </text>
                <text
                  x={point.x}
                  y={point.y + half + lab(20)}
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

          {/* RING 1 + MEMBER FANS — cluster order: each hub, then its
              members (A5 tab order = DOM order). */}
          {graph.departments.map((dept) => {
            const point = at(dept.id);
            const ramp = rampOf(dept.id);
            const count = deptArcs.get(dept.id)?.count ?? 0;
            const active =
              focusDept === dept.id ||
              selectedId === dept.id ||
              traceRelated(dept.id) ||
              hover === dept.id ||
              matches(dept.id);
            const activate = () => {
              onFocusDept(focusDept === dept.id ? null : dept.id);
              onSelectNode({ id: dept.id, kind: "department", label: dept.label });
            };
            return (
              <g key={dept.id}>
                <g
                  transform={`translate(${point.x},${point.y})`}
                  opacity={op(dept.id)}
                  style={{ cursor: "pointer", transition: nodeTransition }}
                  onMouseEnter={() => setHover(dept.id)}
                  onMouseLeave={() => setHover(null)}
                  onClick={activate}
                  data-testid="graph-dept"
                  data-id={dept.id}
                  data-dept={dept.id}
                  {...nodeKeyProps(dept.id, `${dept.label} department, ${peoplePlural(count)} in scope`, activate)}
                >
                  <title>{`${dept.label} department`}</title>
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
                    style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs), fontWeight: 500 }}
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
                    data-testid="graph-dept-count"
                  >
                    {peoplePlural(count)}
                  </text>
                </g>
                {!hidden.has("people") &&
                  (chipDepts.has(dept.id)
                    ? renderClusterChip(dept.id)
                    : (membersByDept.get(dept.id) ?? []).map(renderPerson))}
              </g>
            );
          })}

          {/* RING 2 — systems of record + the unplaced agent kind, resting
              at 80% opacity (depth = scale + opacity only). */}
          {!hidden.has("sources") &&
            graph.sources.map((source) => {
              const point = at(source.id);
              const active =
                selectedId === source.id || traceRelated(source.id) || hover === source.id || matches(source.id);
              const activate = () => onSelectNode({ id: source.id, kind: "source", label: source.label });
              return (
                <g
                  key={source.id}
                  transform={`translate(${point.x},${point.y})`}
                  opacity={active ? 1 : ring2Op(source.id)}
                  style={{ cursor: "pointer", transition: nodeTransition }}
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
                    style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 4), fontWeight: 800 }}
                  >
                    {shortMark(source.label)}
                  </text>
                  <text
                    y={STAGE.sourceRadius + lab(14)}
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
                  opacity={active ? 1 : ring2Op(tool.id)}
                  style={{ cursor: "pointer", transition: nodeTransition }}
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
                    rx={RADIUS.glyph}
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

          {/* PROJECTS — payload-linked only, 24px, between center and ring 1. */}
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
                style={{ cursor: "pointer", transition: nodeTransition }}
                onMouseEnter={() => setHover(project.id)}
                onMouseLeave={() => setHover(null)}
                onClick={activate}
                data-testid="graph-project"
                data-id={project.id}
                {...nodeKeyProps(
                  project.id,
                  `${compactProjectLabel(project.label)}, project, ${peoplePlural(project.people)}`,
                  activate,
                )}
              >
                <title>{`${project.label} - ${peoplePlural(project.people)} - ${project.workflow_name}`}</title>
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
        </g>
      </svg>
    </div>
  );
}
