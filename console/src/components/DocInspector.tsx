import type { DocCard } from "@/lib/api";
import { INSPECTOR_WIDTH } from "@/lib/constants";
import { SensitivityBadge } from "./SensitivityBadge";
import { Skeleton } from "./Skeleton";

/**
 * The doc inspector side sheet (420px). A 404 — which the service guarantees
 * is byte-identical for out-of-scope and nonexistent ids — renders ONE empty
 * state; the console cannot know more, so it cannot say more (U-5).
 */
export function DocInspector({
  open,
  loading,
  card,
  onClose,
  onOpenDoc,
}: {
  open: boolean;
  loading: boolean;
  /** null = the service said 404 (unavailable, whatever the reason). */
  card: DocCard | null;
  onClose: () => void;
  onOpenDoc: (docId: string) => void;
}) {
  if (!open) {
    return null;
  }
  return (
    <aside
      className="fixed inset-y-0 right-0 z-20 flex flex-col border-l border-stone-200 bg-white shadow-xl"
      style={{ width: INSPECTOR_WIDTH }}
      data-testid="doc-inspector"
    >
      <div className="flex items-center justify-between border-b border-stone-200 px-4 py-3">
        <h2 className="text-sm font-semibold text-stone-700">Document</h2>
        <button
          type="button"
          onClick={onClose}
          className="rounded px-2 py-0.5 text-sm text-stone-500 hover:bg-stone-100"
          data-testid="inspector-close"
        >
          Close
        </button>
      </div>
      <div className="flex-1 overflow-y-auto p-4">
        {loading ? (
          <Skeleton lines={4} />
        ) : card ? (
          <div className="space-y-3" data-testid="inspector-card">
            <div className="flex items-start justify-between gap-2">
              <h3 className="text-sm font-medium text-stone-900">{card.title}</h3>
              <SensitivityBadge sensitivity={card.sensitivity} />
            </div>
            <p className="font-mono text-[11px] text-stone-400">{card.document_id}</p>
            {card.superseded && (
              <div
                className="rounded border border-stone-300 bg-stone-50 px-3 py-2 text-xs text-stone-600"
                data-testid="superseded-notice"
              >
                This version is superseded.
                {card.effective_successor && (
                  <>
                    {" "}
                    Effective version:{" "}
                    <button
                      type="button"
                      onClick={() => onOpenDoc(card.effective_successor!)}
                      className="font-mono underline hover:text-stone-900"
                      data-testid="successor-link"
                    >
                      {card.effective_successor}
                    </button>
                  </>
                )}
              </div>
            )}
            <p className="text-sm leading-relaxed text-stone-700">{card.snippet}</p>
          </div>
        ) : (
          <p className="py-8 text-center text-sm text-stone-500" data-testid="inspector-empty">
            This document isn&apos;t available.
          </p>
        )}
      </div>
    </aside>
  );
}
