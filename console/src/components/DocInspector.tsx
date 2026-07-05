"use client";

import type { DocCard } from "@/lib/api";
import { INSPECTOR_WIDTH } from "@/lib/constants";
import { TYPE } from "@/lib/tokens";
import { useModalDialogFocus } from "./A11yDialog";
import { SensitivityBadge } from "./SensitivityBadge";
import { Skeleton } from "./Skeleton";

/**
 * The doc inspector side sheet (420px). Restyled into the Aperture language
 * (hairline elevation — the shadow is gone; whitespace and the 1px rule do
 * the work). A 404 — byte-identical for out-of-scope and nonexistent —
 * renders ONE empty state (U-5).
 *
 * B6 (comprehension pass): a real dialog — role/aria-modal, focus moves in on
 * open, Tab is trapped, Escape closes, focus returns to the opener. The
 * behavior comes from the shared useModalDialogFocus primitive (extracted
 * from the /me drawer), not a local copy.
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
  const { dialogRef, onKeyDown } = useModalDialogFocus({ open, onClose });
  if (!open) {
    return null;
  }
  return (
    <aside
      ref={dialogRef as React.MutableRefObject<HTMLElement | null>}
      role="dialog"
      aria-modal="true"
      aria-label="Document"
      tabIndex={-1}
      onKeyDown={onKeyDown}
      className="ap-card fixed inset-y-0 right-0 z-20 flex flex-col border-y-0 border-r-0"
      style={{ width: INSPECTOR_WIDTH }}
      data-testid="doc-inspector"
    >
      <div className="ap-hairline flex items-center justify-between border-b px-4 py-3">
        <h2
          className="ap-register-chrome ap-soft"
          style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
        >
          Document
        </h2>
        <button
          type="button"
          onClick={onClose}
          className="ap-washable ap-soft rounded-lg px-2 py-0.5"
          style={{ fontSize: TYPE.scale.sm }}
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
              <h3
                className="ap-register-chrome"
                style={{ fontSize: TYPE.scale.sm, fontWeight: 500 }}
              >
                {card.title}
              </h3>
              <SensitivityBadge sensitivity={card.sensitivity} />
            </div>
            <p
              className="ap-register-evidence ap-soft"
              style={{ fontSize: TYPE.scale.xs }}
            >
              {card.document_id}
            </p>
            {card.superseded && (
              <div
                className="ap-card ap-soft rounded-lg px-3 py-2"
                style={{ fontSize: TYPE.scale.xs }}
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
                      className="ap-register-evidence ap-affordance-text underline"
                      data-testid="successor-link"
                    >
                      {card.effective_successor}
                    </button>
                  </>
                )}
              </div>
            )}
            <p style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
              {card.snippet}
            </p>
          </div>
        ) : (
          <p
            className="ap-soft py-8 text-center"
            style={{ fontSize: TYPE.scale.sm }}
            data-testid="inspector-empty"
          >
            This document isn&apos;t available.
          </p>
        )}
      </div>
    </aside>
  );
}
