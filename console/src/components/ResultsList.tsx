import type { EnrichedResult } from "@/lib/api";
import { SensitivityBadge } from "./SensitivityBadge";

/**
 * The results list. Empty scope renders a plain statement of fact — no
 * counts, no hints, no suggestions that imply hidden content. Unknown fields
 * on a result are simply never read (the no-dark-counts rule at runtime).
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
      <p className="py-6 text-center text-sm text-stone-500" data-testid="empty-results">
        Nothing in your scope matches
      </p>
    );
  }
  return (
    <ol className="divide-y divide-stone-100" data-testid="results-list">
      {results.map((result) => (
        <li key={result.document_id}>
          <button
            type="button"
            onClick={() => onOpenDoc(result.document_id)}
            className="flex w-full items-center gap-3 px-2 py-2 text-left hover:bg-stone-50"
            data-testid="result-row"
          >
            <span className="w-6 shrink-0 text-right font-mono text-[11px] text-stone-400">
              {result.score_rank}
            </span>
            <span className="min-w-0 flex-1">
              <span className="block truncate text-sm text-stone-800">{result.title}</span>
              <span className="font-mono text-[11px] text-stone-400">{result.document_id}</span>
            </span>
            <SensitivityBadge sensitivity={result.sensitivity} />
          </button>
        </li>
      ))}
    </ol>
  );
}
