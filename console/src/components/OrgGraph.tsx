"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { select } from "d3-selection";
import { zoom as d3zoom, zoomIdentity, type ZoomBehavior, type ZoomTransform } from "d3-zoom";
import type { GraphEdge, GraphPerson, GraphResponse } from "@/lib/api";
import { DEPARTMENT_PASTEL, FONT, GEOMETRY, GRAPH_STAGE, RADIUS, TYPE, departmentPastelMap } from "@/lib/tokens";
import { peoplePlural } from "./graphDisplay";
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
  bound: number;
};

/**
 * SHOWCASE-1 (Track B) — THE REFERENCE OPERATING MAP. The admin Operating
 * Map's rest-state composition, rebuilt to the owner's reference:
 * - CENTER: the organization, a 64px glowing core.
 * - HUB RING (210px): one pastel tile per payload department (the department
 *   pastel family — an owner-ratified amendment to the reserved-color law;
 *   saturated amber/red stay EXCLUSIVELY sensitivity/signal).
 * - THE PEOPLE RIM (430px): every payload person is an 8px dot, grouped into
 *   contiguous department arcs (sorted by department, then name); each group
 *   is embraced by a 3px pastel arc stroke (448px). Department heads (payload
 *   ring=="anchor") are PROMOTED to a 26px avatar at 452px.
 * - MID-FIELD: sources (22px) between hub gaps at ~300px; agents (18px glyph)
 *   adjacent to their owning department's hub.
 * - Center→hub edges: dotted amber — the honest "signals unavailable" state
 *   (live activity signals are not wired in this build; the dotted amber says
 *   so). Rim spokes (person→center): 0.5px white at 12% — barely-there radial
 *   texture. THE ONE DELIBERATE AMENDMENT to GP's rest-edge law for THIS
 *   surface: at 100+ people the fabric reads as texture, not a hairball. Each
 *   spoke corresponds 1:1 to a real member_of payload edge (F6 holds — only
 *   its endpoint is stylized to the center), flagged in the closeout.
 * The GP interaction laws survive underneath: hover/focus/selection lights the
 * ego's true payload edge set at 1.5px/100%, everything else dims to 15%;
 * selection persists until Escape/click-away; ≤200ms; dead under reduced
 * motion. Keyboard: center → hubs → heads → arc members → mid-field; sr-only
 * mirror regrouped per department. p_void: whitespace + masthead. No blur, no
 * glass — the glow is radial-gradient fills + opacity.
 */
const STAGE = {
  coreSize: 64,
  hubRing: 210,
  hubTile: 52,
  hubRadius: 26,
  sourceRing: 300,
  sourceRadius: 11,
  agentRing: 250,
  agentSize: 9,
  peopleRim: 430,
  personDot: 8,
  arcRing: 448,
  arcStroke: 3,
  arcPadDeg: 6,
  headRing: 452,
  headAvatar: 26,
  /** A small angular gap between adjacent department arcs (radians). */
  deptGap: (10 * Math.PI) / 180,
  /** Label breathing room inside the computed bounding radius. */
  boundMargin: 96,
} as const;

/** Rest edges (GP): the person→center rim spokes are the barely-there texture;
 * the ego's payload edge set lights at 1.5px/100%; non-connected dims to 15%. */
const EDGE_LIT = { width: 1.5, opacity: 1 } as const;
const RIM_SPOKE = { width: 0.5, opacity: 0.12 } as const;
/** Center→hub dotted-amber: the "signals unavailable" honesty state. */
const HUB_EDGE = { width: 1.25, opacity: 0.55 } as const;

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
  return {
    x: Math.round((cx + radius * Math.cos(angle)) * 100) / 100,
    y: Math.round((cy + radius * Math.sin(angle)) * 100) / 100,
  };
}

function shortMark(label: string): string {
  const words = label.replace(/&/g, " ").split(/\s+/).filter(Boolean);
  if (words.length === 0) return "?";
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[words.length - 1][0]).toUpperCase();
}

function monogram(name: string): string {
  return shortMark(name).slice(0, 2);
}

/** Members of a department, sorted heads-first then by name — the arc order. */
function sortedMembers(list: GraphPerson[]): GraphPerson[] {
  return [...list].sort((a, b) =>
    a.ring === b.ring
      ? a.display_name.localeCompare(b.display_name)
      : a.ring === "anchor"
        ? -1
        : 1,
  );
}

function computeLayout(graph: GraphResponse): Layout {
  const pos = new Map<string, Pos>();
  const ang = new Map<string, number>();
  const hubAngle = new Map<string, number>();
  const deptArcs = new Map<string, DeptArc>();
  pos.set(graph.center.id, { x: 0, y: 0 });

  const peopleByDept = new Map<string, GraphPerson[]>();
  for (const person of graph.people) {
    const list = peopleByDept.get(person.department_id) ?? [];
    list.push(person);
    peopleByDept.set(person.department_id, list);
  }

  // HUB RING — one hub per department, evenly distributed from 12 o'clock.
  const hubCount = Math.max(graph.departments.length, 1);
  graph.departments.forEach((dept, index) => {
    const angle = -Math.PI / 2 + (index / hubCount) * 2 * Math.PI;
    hubAngle.set(dept.id, angle);
    pos.set(dept.id, polar(angle, STAGE.hubRing));
  });

  // THE PEOPLE RIM — contiguous department arcs; each department's angular
  // SPAN is proportional to its member count so dense departments read wider.
  // People are evenly spaced within their department's span, in arc order.
  const totalPeople = Math.max(graph.people.length, 1);
  const usable = 2 * Math.PI - graph.departments.length * STAGE.deptGap;
  let cursor = -Math.PI / 2 - usable / 2 - (graph.departments.length * STAGE.deptGap) / 2;
  for (const dept of graph.departments) {
    const members = sortedMembers(peopleByDept.get(dept.id) ?? []);
    const span = usable * (Math.max(members.length, 0.6) / totalPeople);
    const start = cursor + STAGE.deptGap / 2;
    const end = start + span;
    const center = (start + end) / 2;
    deptArcs.set(dept.id, { start, end, center, count: members.length });
    members.forEach((person, i) => {
      const frac = members.length <= 1 ? 0.5 : i / (members.length - 1);
      // Keep dots off the exact arc endpoints so the embrace stroke reads.
      const pad = span * 0.08;
      const angle = members.length === 1 ? center : start + pad + frac * (span - 2 * pad);
      pos.set(person.id, polar(angle, STAGE.peopleRim));
      ang.set(person.id, angle);
    });
    cursor = end + STAGE.deptGap / 2;
  }

  // MID-FIELD sources — distributed in the hub GAPS (offset half a step).
  graph.sources.forEach((source, index) => {
    const angle =
      -Math.PI / 2 + ((index + 0.5) / Math.max(graph.sources.length, 1)) * 2 * Math.PI;
    pos.set(source.id, polar(angle, STAGE.sourceRing));
    ang.set(source.id, angle);
  });

  // Agents — adjacent to the owning department's hub (fanned if several).
  const agentsSeen = new Map<string, number>();
  graph.tools.forEach((tool, index) => {
    const base = tool.department_id ? hubAngle.get(tool.department_id) : undefined;
    const seen = tool.department_id ? agentsSeen.get(tool.department_id) ?? 0 : index;
    if (tool.department_id) agentsSeen.set(tool.department_id, seen + 1);
    const angle =
      base !== undefined
        ? base + (seen % 2 === 0 ? 1 : -1) * 0.12 * Math.ceil((seen + 1) / 2)
        : -Math.PI / 2 + (index / Math.max(graph.tools.length, 1)) * 2 * Math.PI;
    pos.set(tool.id, polar(angle, STAGE.agentRing));
    ang.set(tool.id, angle);
  });

  const bound = STAGE.headRing + STAGE.headAvatar + STAGE.boundMargin;
  return { pos, ang, hubAngle, deptArcs, bound };
}

/** An SVG arc path from `start` to `end` at `radius` (clockwise, ≤2π). */
function arcPath(radius: number, start: number, end: number): string {
  const a0 = polar(start, radius);
  const a1 = polar(end, radius);
  const large = end - start > Math.PI ? 1 : 0;
  return `M${a0.x},${a0.y}A${radius},${radius} 0 ${large} 1 ${a1.x},${a1.y}`;
}

function chordPath(from: Pos, to: Pos, curve: number): string {
  if (curve === 0) return `M${from.x},${from.y}L${to.x},${to.y}`;
  const mx = (from.x + to.x) / 2;
  const my = (from.y + to.y) / 2;
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const len = Math.hypot(dx, dy) || 1;
  return `M${from.x},${from.y}Q${mx + (-dy / len) * curve},${my + (dx / len) * curve} ${to.x},${to.y}`;
}

/** Edge KIND rides dash pattern, never extra hue (unchanged from GP). */
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
  const { pos, ang, hubAngle, deptArcs, bound } = useMemo(() => computeLayout(graph), [graph]);
  const [hover, setHover] = useState<string | null>(null);
  const [transform, setTransform] = useState<ZoomTransform>(zoomIdentity);
  const svgRef = useRef<SVGSVGElement | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const zoomRef = useRef<ZoomBehavior<SVGSVGElement, unknown> | null>(null);
  const nodeRefs = useRef<Map<string, SVGGElement>>(new Map());

  const peopleById = useMemo(() => new Map(graph.people.map((p) => [p.id, p])), [graph.people]);
  const toolsById = useMemo(() => new Map(graph.tools.map((t) => [t.id, t])), [graph.tools]);
  const sourcesById = useMemo(() => new Map(graph.sources.map((s) => [s.id, s])), [graph.sources]);
  const deptById = useMemo(() => new Map(graph.departments.map((d) => [d.id, d])), [graph.departments]);

  /** Track B: the department pastel family, deterministically assigned. */
  const pastelOf = useMemo(() => {
    const map = departmentPastelMap(graph.departments.map((d) => d.label));
    return (deptId: string | null | undefined): string => {
      const label = deptId != null ? deptById.get(deptId)?.label : undefined;
      return (label && map.get(label)) || DEPARTMENT_PASTEL[DEPARTMENT_PASTEL.length - 1].hex;
    };
  }, [graph.departments, deptById]);

  const membersByDept = useMemo(() => {
    const map = new Map<string, GraphPerson[]>();
    for (const dept of graph.departments) map.set(dept.id, []);
    for (const person of graph.people) {
      const list = map.get(person.department_id) ?? [];
      list.push(person);
      map.set(person.department_id, list);
    }
    for (const [k, v] of map) map.set(k, sortedMembers(v));
    return map;
  }, [graph.departments, graph.people]);

  /** The head (promoted avatar) per department: the payload anchor. */
  const headByDept = useMemo(() => {
    const map = new Map<string, GraphPerson>();
    for (const dept of graph.departments) {
      const head = (membersByDept.get(dept.id) ?? []).find((p) => p.ring === "anchor");
      if (head) map.set(dept.id, head);
    }
    return map;
  }, [graph.departments, membersByDept]);

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
        const focusK = 1.5;
        const frame = polar(angle, (STAGE.hubRing + STAGE.peopleRim) / 2);
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
    const dept = deptById.get(id);
    if (dept) return dept.label.toLowerCase().includes(q) || dept.id.toLowerCase().includes(q);
    return id.toLowerCase().includes(q);
  };

  const inDept = (id: string): boolean =>
    id === focusDept ||
    peopleById.get(id)?.department_id === focusDept ||
    toolsById.get(id)?.department_id === focusDept;

  /** GP ego: hover mirrors keyboard focus; a selection persists the ego until
   * Escape/click-away, for EVERY node kind (the center included). */
  const rawEgo = hover ?? selectedId;
  const ego = rawEgo;
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

  /** GP staged edges: at REST only the person→center rim spokes (member_of,
   * textural) and the center→hub dotted-amber signals draw; every relationship
   * chord lights ONLY on the ego's focus (or department focus). The masthead
   * keeps the full payload edge count — the map never claims fewer. */
  const isRimSpoke = (edge: GraphEdge): boolean =>
    edge.kind === "member_of" && peopleById.has(edge.from) && deptById.has(edge.to);
  const isHubEdge = (edge: GraphEdge): boolean =>
    edge.kind === "member_of" && deptById.has(edge.from) && edge.to === graph.center.id;
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

  const edgeHidden = (edge: GraphEdge): boolean =>
    (hidden.has("people") && (peopleById.has(edge.from) || peopleById.has(edge.to))) ||
    (hidden.has("agents") && (toolsById.has(edge.from) || toolsById.has(edge.to))) ||
    (hidden.has("sources") && (sourcesById.has(edge.from) || sourcesById.has(edge.to)));

  // -------------------------------------------------------------------------
  // KEYBOARD (GP / WCAG 2.1.1): tab order = center → hubs → heads → arc
  // members → mid-field (sources+agents). Arrows traverse WITHIN a tier;
  // Escape climbs (member → its hub → root) and releases a persisted selection.
  // -------------------------------------------------------------------------
  const tiers: string[][] = useMemo(() => {
    const hubTier = graph.departments.map((d) => d.id);
    const headTier = graph.departments
      .map((d) => headByDept.get(d.id)?.id)
      .filter((id): id is string => id !== undefined);
    // Heads live in the head tier ONLY; the member tier is the department's
    // non-anchor dots (the head is not a member-tier dot, so arrows within an
    // arc traverse the members without ejecting at — and getting trapped on —
    // the head).
    const memberTiers: string[][] = hidden.has("people")
      ? []
      : graph.departments.map((d) =>
          (membersByDept.get(d.id) ?? []).filter((p) => p.ring !== "anchor").map((p) => p.id),
        );
    const midTier = [
      ...(hidden.has("sources") ? [] : graph.sources.map((s) => s.id)),
      ...(hidden.has("agents") ? [] : graph.tools.map((t) => t.id)),
    ];
    return [[graph.center.id], hubTier, headTier, ...memberTiers, midTier].filter(
      (tier) => tier.length > 0,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [graph, hidden, membersByDept, headByDept]);

  const tierOf = (id: string): string[] | undefined => tiers.find((tier) => tier.includes(id));
  const registerNode = (id: string) => (el: SVGGElement | null) => {
    if (el) nodeRefs.current.set(id, el);
    else nodeRefs.current.delete(id);
  };
  const moveFocus = (fromId: string, delta: 1 | -1) => {
    const tier = tierOf(fromId);
    if (!tier) return;
    const index = tier.indexOf(fromId);
    const next = tier[(index + delta + tier.length) % tier.length];
    nodeRefs.current.get(next)?.focus();
  };
  const escapeFrom = (id: string) => {
    if (selectedId !== null) onSelectNode(null);
    const person = peopleById.get(id);
    const hubId = person?.department_id ?? toolsById.get(id)?.department_id ?? null;
    if (hubId !== null && nodeRefs.current.has(hubId)) {
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
  const onStageClick = (event: React.MouseEvent<SVGSVGElement>) => {
    if (event.target === event.currentTarget && selectedId !== null) {
      onSelectNode(null);
    }
  };

  // -------------------------------------------------------------------------
  // RENDER HELPERS
  // -------------------------------------------------------------------------
  const renderPersonDot = (person: GraphPerson) => {
    const point = at(person.id);
    const active =
      hover === person.id ||
      selectedId === person.id ||
      traceRelated(person.id) ||
      matches(person.id) ||
      (focusDept !== null && person.department_id === focusDept);
    const pastel = pastelOf(person.department_id);
    const deptLabel = deptById.get(person.department_id)?.label ?? person.department_id;
    const activate = () => onSelectNode({ id: person.id, kind: "human", label: person.display_name });
    const r = active ? STAGE.personDot / 2 + 2 : STAGE.personDot / 2;
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
        <circle r={r} fill={pastel} fillOpacity={0.9} stroke={C.paper} strokeWidth={0.5} strokeOpacity={0.3} />
        {(active || person.is_self) && (
          <circle
            r={r + 3}
            fill="none"
            stroke={person.is_self ? C.affordance : C.affordance}
            strokeWidth={person.is_self ? 2 : 1.5}
            strokeOpacity={0.9}
            data-testid={person.is_self ? "graph-self-marker" : undefined}
          />
        )}
        {active && (
          <text
            y={-r - lab(6)}
            textAnchor="middle"
            fill={GRAPH_STAGE.label}
            paintOrder="stroke"
            stroke={GRAPH_STAGE.canvasEdge}
            strokeWidth={GEOMETRY.graphLabelHalo}
            style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs), fontWeight: 500 }}
            data-testid="graph-person-name"
          >
            {person.display_name}
          </text>
        )}
      </g>
    );
  };

  const renderHead = (deptId: string) => {
    const head = headByDept.get(deptId);
    if (!head || hidden.has("people")) return null;
    const arc = deptArcs.get(deptId);
    const direction = arc?.center ?? hubAngle.get(deptId) ?? 0;
    const point = polar(direction, STAGE.headRing);
    const size = STAGE.headAvatar;
    const pastel = pastelOf(deptId);
    const active =
      hover === head.id || selectedId === head.id || traceRelated(head.id) || matches(head.id);
    const anchor: "start" | "end" | "middle" =
      Math.cos(direction) > 0.34 ? "start" : Math.cos(direction) < -0.34 ? "end" : "middle";
    const activate = () => onSelectNode({ id: head.id, kind: "human", label: head.display_name });
    const lx = Math.cos(direction) * (size / 2 + lab(6));
    const ly = Math.sin(direction) * (size / 2 + lab(6));
    return (
      <g
        key={`head:${head.id}`}
        transform={`translate(${point.x},${point.y})`}
        opacity={op(head.id)}
        style={{ cursor: "pointer", transition: nodeTransition }}
        onMouseEnter={() => setHover(head.id)}
        onMouseLeave={() => setHover(null)}
        onClick={activate}
        data-testid="graph-head"
        data-id={head.id}
        data-self={head.is_self ? "true" : "false"}
        {...nodeKeyProps(head.id, `${head.display_name}, ${head.title}, ${deptById.get(deptId)?.label ?? deptId} head`, activate)}
      >
        <title>{`${head.display_name}, ${head.title}`}</title>
        <circle r={size / 2 + 3} fill={C.paper} stroke={pastel} strokeWidth={1.5} />
        {head.is_self && (
          <circle r={size / 2 + 7} fill="none" stroke={C.affordance} strokeWidth={2} data-testid="graph-self-marker" />
        )}
        <foreignObject x={-size / 2} y={-size / 2} width={size} height={size}>
          <PersonAvatar
            principalId={head.id}
            displayName={head.display_name}
            size={size}
            tint={{ background: `color-mix(in srgb, ${pastel} 26%, var(--paper))`, border: pastel }}
          />
        </foreignObject>
        <circle
          r={size / 2 + 7}
          fill="none"
          stroke={C.affordance}
          strokeWidth={active || selectedId === head.id ? 2 : 0}
          strokeOpacity={active || selectedId === head.id ? 0.9 : 0}
        />
        <g transform={`translate(${lx},${ly})`}>
          <text
            textAnchor={anchor}
            dominantBaseline="middle"
            fill={GRAPH_STAGE.label}
            paintOrder="stroke"
            stroke={GRAPH_STAGE.canvasEdge}
            strokeWidth={GEOMETRY.graphLabelHalo}
            style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 1), fontWeight: 600 }}
            data-testid="graph-head-name"
          >
            {head.display_name}
          </text>
          <text
            y={lab(12)}
            textAnchor={anchor}
            dominantBaseline="middle"
            fill={GRAPH_STAGE.labelSoft}
            paintOrder="stroke"
            stroke={GRAPH_STAGE.canvasEdge}
            strokeWidth={GEOMETRY.graphLabelHalo}
            style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 3) }}
          >
            {head.title}
          </text>
        </g>
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

      {/* SR mirror (WCAG 4.1.2), regrouped per department: one list per hub. */}
      <div className="sr-only" data-testid="graph-sr-mirror">
        <p>{graph.center.label} — organization</p>
        {graph.departments.map((dept) => {
          const count = deptArcs.get(dept.id)?.count ?? 0;
          return (
            <ul key={dept.id} aria-label={`${dept.label} department`} data-testid="graph-sr-dept">
              <li>
                {dept.label} — department — {peoplePlural(count)} in scope
              </li>
              {!hidden.has("people") &&
                (membersByDept.get(dept.id) ?? []).map((person) => (
                  <li key={person.id}>
                    {person.display_name} — {person.title}
                    {person.ring === "anchor" ? " — head" : ""}
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
        <defs>
          {/* The stage: a deep-navy radial field. No blur — radial-gradient
              fills + opacity only (the no-filter law holds). */}
          <radialGradient id="og-canvas" cx="50%" cy="50%" r="72%">
            <stop offset="0%" stopColor={GRAPH_STAGE.canvasCenter} />
            <stop offset="100%" stopColor={GRAPH_STAGE.canvasEdge} />
          </radialGradient>
          <radialGradient id="og-core-glow" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor={GRAPH_STAGE.coreGlow} stopOpacity={0.55} />
            <stop offset="100%" stopColor={GRAPH_STAGE.coreGlow} stopOpacity={0} />
          </radialGradient>
          <radialGradient id="og-vignette" cx="50%" cy="50%" r="72%">
            <stop offset="60%" stopColor={GRAPH_STAGE.vignette} stopOpacity={0} />
            <stop offset="100%" stopColor={GRAPH_STAGE.vignette} stopOpacity={0.35} />
          </radialGradient>
        </defs>

        {/* Canvas + vignette fill the whole viewBox. */}
        <rect x={-bound} y={-bound} width={bound * 2} height={bound * 2} fill="url(#og-canvas)" data-testid="graph-canvas" />
        <rect x={-bound} y={-bound} width={bound * 2} height={bound * 2} fill="url(#og-vignette)" pointerEvents="none" />

        <g transform={transform.toString()} data-testid="graph-scene">
          {/* Rim spokes (person→center): the barely-there radial texture. Each
              is a real member_of payload edge, drawn to center for fabric. */}
          {!hidden.has("people") && (
            <g data-testid="graph-rim-spokes" pointerEvents="none">
              {graph.edges.map((edge, index) => {
                if (!isRimSpoke(edge) || edgeHidden(edge)) return null;
                const person = at(edge.from);
                const lit = touchesEgo(edge) || litInFocusDept(edge);
                return (
                  <line
                    key={`spoke-${edge.from}-${index}`}
                    x1={person.x}
                    y1={person.y}
                    x2={0}
                    y2={0}
                    stroke={lit ? C.warm : GRAPH_STAGE.rimSpoke}
                    strokeWidth={lit ? EDGE_LIT.width : RIM_SPOKE.width}
                    strokeOpacity={lit ? EDGE_LIT.opacity : dimming && !emphasized(edge.from) ? 0.04 : RIM_SPOKE.opacity}
                    style={{ transition: edgeTransition }}
                    data-testid="graph-rim-spoke"
                    data-kind="member_of"
                    data-lit={lit ? "true" : "false"}
                  />
                );
              })}
            </g>
          )}

          {/* Relationship chords + hub edges. At rest ONLY the dotted-amber
              center→hub "signals unavailable" edges draw; chords light on the
              ego's focus. */}
          <g data-testid="graph-edges">
            {graph.edges.map((edge, index) => {
              if (edgeHidden(edge) || isRimSpoke(edge)) return null;
              const hubEdge = isHubEdge(edge);
              const lit = touchesEgo(edge) || litInFocusDept(edge);
              if (!hubEdge && !lit) return null;
              const from = at(edge.from);
              const to = at(edge.to);
              const dx = to.x - from.x;
              const dy = to.y - from.y;
              const curve = hubEdge ? 0 : (edge.from < edge.to ? 1 : -1) * Math.min(Math.hypot(dx, dy) * 0.08, 44);
              const dimmed = dimming && !lit;
              return (
                <path
                  key={`${edge.from}-${edge.kind}-${edge.to}-${index}`}
                  d={chordPath(from, to, curve)}
                  fill="none"
                  stroke={lit ? C.warm : C.warm}
                  strokeWidth={lit ? EDGE_LIT.width : HUB_EDGE.width}
                  strokeOpacity={lit ? EDGE_LIT.opacity : dimmed ? GEOMETRY.graphDimOpacity : HUB_EDGE.opacity}
                  strokeDasharray={hubEdge ? "1 5" : EDGE_DASH[edge.kind]}
                  strokeLinecap="round"
                  style={{ transition: edgeTransition }}
                  data-testid="graph-edge"
                  data-kind={edge.kind}
                  data-from={edge.from}
                  data-to={edge.to}
                  data-hub={hubEdge ? "true" : "false"}
                  data-lit={lit ? "true" : "false"}
                />
              );
            })}
          </g>

          {/* Department arc strokes — each group embraced by its pastel. */}
          <g data-testid="graph-dept-arcs">
            {graph.departments.map((dept) => {
              const arc = deptArcs.get(dept.id);
              if (!arc || arc.count === 0) return null;
              const pad = (STAGE.arcPadDeg * Math.PI) / 180;
              return (
                <path
                  key={dept.id}
                  d={arcPath(STAGE.arcRing, arc.start + pad, arc.end - pad)}
                  fill="none"
                  stroke={pastelOf(dept.id)}
                  strokeWidth={STAGE.arcStroke}
                  strokeOpacity={focusDept === null || focusDept === dept.id ? 0.65 : 0.12}
                  strokeLinecap="round"
                  data-testid="graph-dept-arc"
                  data-dept={dept.id}
                />
              );
            })}
          </g>

          {/* CENTER — glowing organization core (first tab stop). */}
          {(() => {
            const active = selectedId === graph.center.id || traceRelated(graph.center.id) || hover === graph.center.id;
            const half = STAGE.coreSize / 2;
            const activate = () => onSelectNode({ id: graph.center.id, kind: "org", label: graph.center.label });
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
                <circle r={half * 2.4} fill="url(#og-core-glow)" pointerEvents="none" />
                <circle r={half} fill={GRAPH_STAGE.canvasCenter} stroke={active ? C.affordance : GRAPH_STAGE.coreGlow} strokeWidth={active ? 2.5 : 1.5} strokeOpacity={0.9} />
                <path
                  d="M-10 6h20M-6 6v-10h12v10M-14 12h28v6h-28z"
                  fill="none"
                  stroke={GRAPH_STAGE.coreGlow}
                  strokeWidth={2}
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  opacity={0.92}
                />
                <text
                  y={half + lab(20)}
                  textAnchor="middle"
                  fill={GRAPH_STAGE.label}
                  paintOrder="stroke"
                  stroke={GRAPH_STAGE.canvasEdge}
                  strokeWidth={GEOMETRY.graphLabelHalo}
                  style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.sm), fontWeight: 600 }}
                  data-testid="graph-center-mark"
                >
                  {graph.center.label}
                </text>
                <text
                  y={half + lab(34)}
                  textAnchor="middle"
                  fill={GRAPH_STAGE.labelSoft}
                  paintOrder="stroke"
                  stroke={GRAPH_STAGE.canvasEdge}
                  strokeWidth={GEOMETRY.graphLabelHalo}
                  style={{ fontFamily: FONT.evidence, fontSize: lab(TYPE.scale.xs - 2) }}
                >
                  live graph payload
                </text>
                <text
                  x={0}
                  y={-half - lab(2)}
                  textAnchor="middle"
                  fill={GRAPH_STAGE.coreGlow}
                  style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs), fontWeight: 800, opacity: 0 }}
                >
                  {monogram(graph.center.label)}
                </text>
              </g>
            );
          })()}

          {/* DEPARTMENT HUBS — pastel tiles, name + count beneath. */}
          {graph.departments.map((dept) => {
            const point = at(dept.id);
            const pastel = pastelOf(dept.id);
            const count = deptArcs.get(dept.id)?.count ?? 0;
            const active =
              focusDept === dept.id || selectedId === dept.id || traceRelated(dept.id) || hover === dept.id || matches(dept.id);
            const half = STAGE.hubTile / 2;
            const activate = () => {
              onFocusDept(focusDept === dept.id ? null : dept.id);
              onSelectNode({ id: dept.id, kind: "department", label: dept.label });
            };
            return (
              <g
                key={dept.id}
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
                <rect
                  x={-half}
                  y={-half}
                  width={STAGE.hubTile}
                  height={STAGE.hubTile}
                  rx={16}
                  fill={`color-mix(in srgb, ${pastel} 22%, ${GRAPH_STAGE.canvasCenter})`}
                  stroke={pastel}
                  strokeWidth={2}
                  strokeOpacity={0.85}
                />
                <path
                  d="M-9 5h18M-5 5v-9h10v9M-13 10h26v6h-26z"
                  fill="none"
                  stroke={pastel}
                  strokeWidth={2}
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  opacity={0.95}
                />
                <circle
                  r={half + 6}
                  fill="none"
                  stroke={active ? C.affordance : pastel}
                  strokeWidth={active ? 2 : 0}
                  strokeOpacity={active ? 0.9 : 0}
                />
                <text
                  y={half + lab(16)}
                  textAnchor="middle"
                  fill={GRAPH_STAGE.label}
                  paintOrder="stroke"
                  stroke={GRAPH_STAGE.canvasEdge}
                  strokeWidth={GEOMETRY.graphLabelHalo}
                  style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs), fontWeight: 600 }}
                  data-testid="graph-dept-label"
                >
                  {dept.label}
                </text>
                <text
                  y={half + lab(29)}
                  textAnchor="middle"
                  fill={GRAPH_STAGE.labelSoft}
                  paintOrder="stroke"
                  stroke={GRAPH_STAGE.canvasEdge}
                  strokeWidth={GEOMETRY.graphLabelHalo}
                  style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 3), fontWeight: 500 }}
                  data-testid="graph-dept-count"
                >
                  {peoplePlural(count)}
                </text>
              </g>
            );
          })}

          {/* MID-FIELD sources — labeled circles between hub gaps. */}
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
                  opacity={op(source.id)}
                  style={{ cursor: "pointer", transition: nodeTransition }}
                  onMouseEnter={() => setHover(source.id)}
                  onMouseLeave={() => setHover(null)}
                  onClick={activate}
                  data-testid="graph-source"
                  data-id={source.id}
                  {...nodeKeyProps(source.id, `${source.label}, system of record`, activate)}
                >
                  <circle
                    r={STAGE.sourceRadius + 4}
                    fill={GRAPH_STAGE.canvasCenter}
                    stroke={active ? C.affordance : C.affordance}
                    strokeWidth={active ? 2 : 1}
                    strokeOpacity={active ? 0.9 : 0.6}
                  />
                  <circle r={STAGE.sourceRadius} fill={C.affordance} fillOpacity={0.85} stroke={GRAPH_STAGE.canvasCenter} strokeWidth={1} />
                  <text
                    y={0.5}
                    textAnchor="middle"
                    dominantBaseline="central"
                    fill={GRAPH_STAGE.canvasEdge}
                    style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 4), fontWeight: 800 }}
                  >
                    {shortMark(source.label)}
                  </text>
                  <text
                    y={STAGE.sourceRadius + lab(13)}
                    textAnchor="middle"
                    fill={GRAPH_STAGE.labelSoft}
                    paintOrder="stroke"
                    stroke={GRAPH_STAGE.canvasEdge}
                    strokeWidth={GEOMETRY.graphLabelHalo}
                    style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 2) }}
                  >
                    {source.label}
                  </text>
                </g>
              );
            })}

          {/* Agents — glyph squares adjacent to their department hub. */}
          {!hidden.has("agents") &&
            graph.tools.map((tool) => {
              const point = at(tool.id);
              const pastel = pastelOf(tool.department_id);
              const active = selectedId === tool.id || traceRelated(tool.id) || hover === tool.id || matches(tool.id);
              const size = active ? STAGE.agentSize + 2 : STAGE.agentSize;
              const activate = () => onSelectNode({ id: tool.id, kind: "agent", label: tool.label });
              return (
                <g
                  key={tool.id}
                  transform={`translate(${point.x},${point.y})`}
                  opacity={op(tool.id)}
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
                  <rect x={-size} y={-size} width={size * 2} height={size * 2} rx={RADIUS.glyph} fill={pastel} stroke={GRAPH_STAGE.canvasCenter} strokeWidth={1} strokeOpacity={0.5} />
                  <text
                    textAnchor="middle"
                    dominantBaseline="central"
                    fill={GRAPH_STAGE.canvasCenter}
                    style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 3), fontWeight: 800 }}
                  >
                    A
                  </text>
                  <text
                    y={size + lab(12)}
                    textAnchor="middle"
                    fill={GRAPH_STAGE.labelSoft}
                    paintOrder="stroke"
                    stroke={GRAPH_STAGE.canvasEdge}
                    strokeWidth={GEOMETRY.graphLabelHalo}
                    style={{ fontFamily: FONT.chrome, fontSize: lab(TYPE.scale.xs - 3) }}
                    data-testid="graph-tool-label"
                  >
                    {tool.label}
                  </text>
                  <circle r={size + 4} fill="none" stroke={C.affordance} strokeWidth={active ? 2 : 0} strokeOpacity={active ? 0.86 : 0} />
                </g>
              );
            })}

          {/* THE PEOPLE RIM — every payload MEMBER, an 8px dot in its arc. The
              department heads are NOT drawn here; they are promoted to avatars
              below (one node per person, so a head is one tab stop, announced
              once, with a single self-marker). */}
          {!hidden.has("people") &&
            graph.people.filter((p) => p.ring !== "anchor").map(renderPersonDot)}

          {/* Promoted department heads. */}
          {graph.departments.map((dept) => renderHead(dept.id))}
        </g>
      </svg>
    </div>
  );
}
