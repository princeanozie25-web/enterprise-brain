import type { AnswerEnvelope } from "@/lib/api";
import { CITATION_PATTERN } from "@/lib/constants";
import { ProvenanceStrip } from "./ProvenanceStrip";

/**
 * The answer card: provenance first, then the generated text with citation
 * chips [d0123] inline. A null answer is a QUIET state — degradation is
 * normal operation, not an error.
 */
export function AnswerCard({
  envelope,
  onOpenDoc,
}: {
  envelope: AnswerEnvelope;
  onOpenDoc: (docId: string) => void;
}) {
  return (
    <section className="rounded-lg border border-stone-200 bg-white p-4" data-testid="answer-card">
      <ProvenanceStrip envelope={envelope} />
      <div className="mt-3">
        {envelope.answer ? (
          <p className="text-sm leading-relaxed text-stone-800" data-testid="answer-text">
            {renderWithCitations(envelope.answer.text, onOpenDoc)}
          </p>
        ) : (
          <p className="text-sm italic text-stone-500" data-testid="no-answer">
            No generated answer
          </p>
        )}
      </div>
    </section>
  );
}

function renderWithCitations(text: string, onOpenDoc: (docId: string) => void) {
  const parts: React.ReactNode[] = [];
  let lastIndex = 0;
  let key = 0;
  for (const match of text.matchAll(CITATION_PATTERN)) {
    const index = match.index ?? 0;
    if (index > lastIndex) {
      parts.push(text.slice(lastIndex, index));
    }
    const docId = match[1];
    parts.push(
      <button
        key={key++}
        type="button"
        onClick={() => onOpenDoc(docId)}
        className="mx-0.5 inline-block rounded border border-stone-300 bg-stone-100 px-1 py-0 align-baseline font-mono text-[11px] text-stone-700 hover:bg-stone-200"
        data-testid="citation-chip"
      >
        [{docId}]
      </button>,
    );
    lastIndex = index + match[0].length;
  }
  if (lastIndex < text.length) {
    parts.push(text.slice(lastIndex));
  }
  return parts;
}
