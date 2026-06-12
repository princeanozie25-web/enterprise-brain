import { COLOR, DERIVED, GEOMETRY, TYPE } from "@/lib/tokens";
import type { LensAgent } from "@/lib/api";

/**
 * The intersection emblem: an agent IS grant ∩ owner scope, drawn as two
 * fixed translucent fields whose overlap is solid affordance.
 *
 * EMBLEMATIC, NOT PROPORTIONAL: the geometry is fixed by token — the field
 * sizes and overlap encode NOTHING about how many documents fall on either
 * side. A chart that implies data it doesn't have is a lie; this is a
 * symbol, like a crest.
 */
export function AgentEmblem({
  agent,
  onNavigate,
}: {
  agent: LensAgent;
  onNavigate: (agentId: string) => void;
}) {
  const w = GEOMETRY.emblemFieldWidth;
  const h = GEOMETRY.emblemFieldHeight;
  const overlap = Math.round(w * GEOMETRY.emblemOverlap);
  const totalWidth = w * 2 - overlap;
  const labelY = h + TYPE.scale.xs + 4;

  return (
    <button
      type="button"
      onClick={() => onNavigate(agent.agent_id)}
      className="ap-washable rounded p-2 text-left"
      data-testid="agent-emblem"
    >
      <svg
        viewBox={`0 0 ${totalWidth} ${labelY + 6}`}
        width={totalWidth}
        height={labelY + 6}
        role="img"
        aria-label={`Intersection emblem for ${agent.agent_id}`}
      >
        <rect x={0} y={0} width={w} height={h} fill={DERIVED.wash} stroke={DERIVED.hairline} />
        <rect
          x={w - overlap}
          y={0}
          width={w}
          height={h}
          fill={DERIVED.wash}
          stroke={DERIVED.hairline}
        />
        <rect x={w - overlap} y={0} width={overlap} height={h} fill={COLOR.affordance} />
        <text
          x={(w - overlap) / 2}
          y={labelY}
          textAnchor="middle"
          fill={COLOR.inkSoft}
          fontFamily="var(--font-chrome)"
          fontSize={TYPE.scale.xs - 2}
        >
          grant
        </text>
        <text
          x={w - overlap / 2}
          y={labelY}
          textAnchor="middle"
          fill={COLOR.affordance}
          fontFamily="var(--font-chrome)"
          fontSize={TYPE.scale.xs - 2}
          fontWeight={500}
        >
          effective
        </text>
        <text
          x={w + (w - overlap) / 2}
          y={labelY}
          textAnchor="middle"
          fill={COLOR.inkSoft}
          fontFamily="var(--font-chrome)"
          fontSize={TYPE.scale.xs - 2}
        >
          owner scope
        </text>
      </svg>
      <div className="mt-1">
        <span className="ap-register-chrome block" style={{ fontSize: TYPE.scale.sm }}>
          {agent.name}
        </span>
        <span
          className="ap-register-evidence ap-soft"
          style={{ fontSize: TYPE.scale.xs }}
        >
          {agent.agent_id}
        </span>
      </div>
    </button>
  );
}
