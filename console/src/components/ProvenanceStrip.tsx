import type { AnswerEnvelope } from "@/lib/api";

/**
 * The provenance strip: four factual badges from the envelope, always
 * rendered. Badge states are labels, never warnings — an elided judge or a
 * lexical fallback is the system being honest, and honesty is not styled as
 * failure (neutral tones only; no red for degradation states).
 */
function Badge({ children, testid }: { children: string; testid: string }) {
  return (
    <span
      className="inline-block rounded border border-stone-300 bg-stone-100 px-2 py-0.5 text-[11px] font-medium text-stone-700"
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
      {envelope.aggregation_bounded && (
        <Badge testid="badge-aggregation">aggregation rule applied</Badge>
      )}
    </div>
  );
}
