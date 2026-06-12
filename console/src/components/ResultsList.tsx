import type { EnrichedResult } from "@/lib/api";
import { COLOR, DERIVED, TYPE } from "@/lib/tokens";
import { SensitivityBadge } from "./SensitivityBadge";

/**
 * The results table: chrome register for titles, evidence register for
 * ranks and doc ids, sensitivity badges from tokens, hairline dividers.
 * Empty scope is a plain statement of fact — no counts, no hints, no
 * suggestions that imply hidden content.
 */
export function ResultsList({
  results,
  onOpenDoc,
}: {
  results: EnrichedResult[];
  onOpenDoc: (docId: string) => void;
}) {
  if (results.length === 0) {
    return (
      <p
        className="ap-soft py-6 text-center"
        style={{ fontSize: TYPE.scale.sm }}
        data-testid="empty-results"
      >
        Nothing in your scope matches
      </p>
    );
  }
  return (
    <ol data-testid="results-list">
      {results.map((result, index) => (
        <li
          key={result.document_id}
          style={index > 0 ? { borderTop: `1px solid ${DERIVED.hairline}` } : undefined}
        >
          <button
            type="button"
            onClick={() => onOpenDoc(result.document_id)}
            className="ap-washable flex w-full items-center gap-3 px-2 py-2 text-left"
            data-testid="result-row"
          >
            <span
              className="ap-register-evidence w-6 shrink-0 text-right"
              style={{ fontSize: TYPE.scale.xs, color: COLOR.inkSoft }}
            >
              {result.score_rank}
            </span>
            <span className="min-w-0 flex-1">
              <span
                className="ap-register-chrome block truncate"
                style={{ fontSize: TYPE.scale.sm }}
              >
                {result.title}
              </span>
              <span
                className="ap-register-evidence"
                style={{ fontSize: TYPE.scale.xs, color: COLOR.inkSoft }}
              >
                {result.document_id}
              </span>
            </span>
            <SensitivityBadge sensitivity={result.sensitivity} />
          </button>
        </li>
      ))}
    </ol>
  );
}
