import type { AnswerEnvelope } from "@/lib/api";
import { TYPE } from "@/lib/tokens";

/**
 * The provenance strip: factual badges in the neutral language — ink-soft
 * outlines, no fills, no icons-as-alarm. An elided judge or a lexical
 * fallback is the system being honest; honesty is never styled as failure,
 * and "aggregation rule applied" gets the same calm treatment as "hybrid".
 */
function Badge({ children, testid }: { children: string; testid: string }) {
  return (
    <span
      className="ap-hairline ap-register-chrome ap-soft inline-block rounded-lg border px-2 py-0.5"
      style={{ fontSize: TYPE.scale.xs, fontWeight: 500 }}
      data-testid={testid}
    >
      {children}
    </span>
  );
}

export function ProvenanceStrip({ envelope }: { envelope: AnswerEnvelope }) {
  return (
    <div className="flex flex-wrap items-center gap-1.5" data-testid="provenance-strip">
      <Badge testid="badge-retrieval">
        {envelope.retrieval_mode === "hybrid" ? "retrieval: hybrid" : "retrieval: lexical only"}
      </Badge>
      <Badge testid="badge-judge">
        {envelope.judge_applied ? "judge: applied" : "judge: elided"}
      </Badge>
      <Badge testid="badge-generation">
        {envelope.generation_applied ? "generation: applied" : "generation: skipped"}
      </Badge>
      {/* K1: anchoring is the whole claim — ruling R-B forbids any stronger
          wording on this surface. The counts are the model's own draft
          claims, disclosed, in the same calm register as every other
          honesty badge. */}
      {envelope.grounding_applied && envelope.grounding && (
        <Badge testid="badge-grounding">
          {`grounding: anchored (${envelope.grounding.admitted} admitted · ${envelope.grounding.refused} removed)`}
        </Badge>
      )}
      {envelope.aggregation_bounded && (
        <Badge testid="badge-aggregation">aggregation rule applied</Badge>
      )}
    </div>
  );
}
