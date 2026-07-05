import type { AnswerEnvelope } from "@/lib/api";
import { CITATION_PATTERN } from "@/lib/constants";
import { TYPE } from "@/lib/tokens";
import { ProvenanceStrip } from "./ProvenanceStrip";

/**
 * The answer card: provenance first, then the generated text in the ANSWER
 * register (serif — the model's voice is its own register, visibly distinct
 * from fact) with citation chips in the EVIDENCE register (mono). A null
 * answer is a quiet chrome-register state — degradation is normal
 * operation, not an error.
 */
export function AnswerCard({
  envelope,
  onOpenDoc,
}: {
  envelope: AnswerEnvelope;
  onOpenDoc: (docId: string) => void;
}) {
  return (
    <section className="ap-card rounded-lg p-4" data-testid="answer-card">
      <ProvenanceStrip envelope={envelope} />
      <div className="mt-3">
        {envelope.answer ? (
          <p
            className="ap-register-answer"
            style={{ fontSize: TYPE.scale.md, lineHeight: TYPE.line.body }}
            data-testid="answer-text"
          >
            {renderWithCitations(envelope.answer.text, onOpenDoc)}
          </p>
        ) : (
          <p
            className="ap-register-chrome ap-soft italic"
            style={{ fontSize: TYPE.scale.sm }}
            data-testid="no-answer"
          >
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
        className="ap-card ap-register-evidence ap-washable mx-0.5 inline-block rounded-lg px-1 py-0 align-baseline"
        style={{ fontSize: TYPE.scale.xs }}
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
