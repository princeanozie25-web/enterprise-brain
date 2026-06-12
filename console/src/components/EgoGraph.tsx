import { COLOR, DERIVED, GEOMETRY, TYPE } from "@/lib/tokens";
import type { LensResponse } from "@/lib/api";

/**
 * The bounded ego-graph: subject at center, one ring — groups, sites, owned
 * agents (or the owner, for agents). One inline SVG, no library, no motion.
 * Deterministic radial layout: nodes sorted lexicographically, equal angles.
 *
 * HARD CAP 21 nodes (center + ring). Beyond it the LIST fallback renders
 * instead — never a truncated graph, because truncation is a dark count
 * wearing a graph.
 */
type RingNode = {
  id: string;
  label: string;
  kind: "group" | "site" | "agent" | "owner" | "capability";
};

export function EgoGraph({
  lens,
  onGroupClick,
  capabilities = [],
  onCapabilityClick,
}: {
  lens: LensResponse;
  onGroupClick: (groupId: string) => void;
  /** AP-3: capability ids where the SUBJECT's visible evidence is
   * non-empty (computed by the caller, fail closed). They join the ring
   * and count against the same hard cap. */
  capabilities?: string[];
  onCapabilityClick?: (capabilityId: string) => void;
}) {
  const ring: RingNode[] = [
    ...lens.subject.groups.map((g) => ({ id: g, label: g, kind: "group" as const })),
    ...lens.subject.sites.map((s) => ({ id: s, label: s, kind: "site" as const })),
    ...(lens.subject.kind === "human"
      ? lens.agents.map((a) => ({ id: a.agent_id, label: a.agent_id, kind: "agent" as const }))
      : lens.subject.owner_user_id
        ? [{ id: lens.subject.owner_user_id, label: lens.subject.owner_user_id, kind: "owner" as const }]
        : []),
    ...capabilities.map((c) => ({ id: c, label: c, kind: "capability" as const })),
  ].sort((a, b) => a.id.localeCompare(b.id));

  const total = ring.length + 1;
  if (total > GEOMETRY.egoNodeCap) {
    return (
      <ul className="space-y-1" data-testid="ego-fallback">
        {ring.map((node) => (
          <li
            key={node.id}
            className="ap-register-evidence ap-soft"
            style={{ fontSize: TYPE.scale.xs }}
          >
            {node.kind}: {node.label}
          </li>
        ))}
      </ul>
    );
  }

  const size = GEOMETRY.egoViewport;
  const center = size / 2;
  const radius = GEOMETRY.egoRingRadius;
  const nodeRadius = GEOMETRY.egoNodeRadius;

  return (
    <svg
      viewBox={`0 0 ${size} ${size}`}
      width={size}
      height={size}
      role="img"
      aria-label={`Ego graph for ${lens.subject.id}`}
      data-testid="ego-graph"
    >
      {ring.map((node, index) => {
        const angle = (-90 + (index * 360) / ring.length) * (Math.PI / 180);
        const x = center + radius * Math.cos(angle);
        const y = center + radius * Math.sin(angle);
        const interactive =
          node.kind === "group" || (node.kind === "capability" && onCapabilityClick !== undefined);
        const onClick =
          node.kind === "group"
            ? () => onGroupClick(node.id)
            : node.kind === "capability" && onCapabilityClick !== undefined
              ? () => onCapabilityClick(node.id)
              : undefined;
        return (
          <g
            key={node.id}
            onClick={onClick}
            style={interactive ? { cursor: "pointer" } : undefined}
            data-testid={`ego-node-${node.kind}`}
          >
            <line
              x1={center}
              y1={center}
              x2={x}
              y2={y}
              stroke={DERIVED.hairline}
              strokeWidth={1}
            />
            <circle
              cx={x}
              cy={y}
              r={nodeRadius}
              fill={interactive ? COLOR.affordance : COLOR.paper}
              stroke={interactive ? COLOR.affordance : COLOR.inkSoft}
              strokeWidth={1}
            />
            <text
              x={x}
              y={y + nodeRadius + TYPE.scale.xs}
              textAnchor="middle"
              fill={COLOR.inkSoft}
              fontFamily="var(--font-evidence)"
              fontSize={TYPE.scale.xs - 2}
            >
              {node.label}
            </text>
          </g>
        );
      })}
      <circle cx={center} cy={center} r={nodeRadius + 2} fill={COLOR.ink} />
      <text
        x={center}
        y={center + nodeRadius + TYPE.scale.xs + 2}
        textAnchor="middle"
        fill={COLOR.ink}
        fontFamily="var(--font-evidence)"
        fontSize={TYPE.scale.xs - 2}
        fontWeight={500}
      >
        {lens.subject.id}
      </text>
    </svg>
  );
}
