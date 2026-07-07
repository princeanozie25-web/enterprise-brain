"use client";

import type { WorkflowItem } from "@/lib/api";
import { DARK, GRAPH_STAGE, TYPE } from "@/lib/tokens";
import { PersonAvatar } from "../PersonAvatar";
import { PASTEL_GREEN, statusLabel } from "./pipeline";

/**
 * SHOWCASE-2 (Track B4) — the human gate. A pending access-request item is the
 * one place a human decides. LIVE approve/reject appear ONLY when the viewer is
 * the item's approver (POST /access-requests/{id}/{approve|deny} — ApprovalGated,
 * server-enforced; item_id IS the request_id). For everyone else — and for
 * pending items with no decision path — there are STATUS CHIPS + one quiet line,
 * never a dead button. State changes only on a 2xx (the board refetches).
 */
export function HumanGatePanel({
  item,
  canDecide,
  busy,
  feedback,
  onDecide,
  onOpen,
}: {
  item: WorkflowItem;
  canDecide: boolean;
  busy: boolean;
  feedback: { kind: "success" | "error"; text: string } | null;
  onDecide: (decision: "approve" | "deny") => void;
  onOpen: () => void;
}) {
  const approver = item.approver_id ?? null;

  return (
    <div
      className="rounded-2xl border p-3"
      data-testid="pipeline-human-gate"
      data-item-id={item.item_id}
      data-can-decide={canDecide ? "true" : "false"}
      style={{ background: DARK.paper, borderColor: DARK.hairline, color: GRAPH_STAGE.label }}
    >
      <p
        className="uppercase"
        style={{ fontSize: TYPE.scale.xs - 2, fontWeight: 700, letterSpacing: "0.09em", color: GRAPH_STAGE.labelSoft }}
      >
        Human in the loop
      </p>

      <div className="mt-2 flex items-center gap-2">
        {approver && <PersonAvatar principalId={approver} size={28} />}
        <div className="min-w-0">
          <p style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            {approver ? `${approver} reviews` : "Awaiting a reviewer"}
          </p>
          <p className="truncate" style={{ fontSize: TYPE.scale.xs, color: GRAPH_STAGE.labelSoft }}>
            {item.title}
          </p>
        </div>
      </div>

      {canDecide ? (
        <div className="mt-3">
          <div className="flex gap-2">
            <button
              type="button"
              disabled={busy}
              onClick={() => onDecide("approve")}
              data-testid="pipeline-gate-approve"
              aria-label="Approve this access request"
              className="flex-1 rounded-lg border px-3 py-2"
              style={{
                borderColor: PASTEL_GREEN,
                color: GRAPH_STAGE.label,
                fontSize: TYPE.scale.xs,
                fontWeight: 600,
                opacity: busy ? 0.55 : 1,
              }}
            >
              Approve
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => onDecide("deny")}
              data-testid="pipeline-gate-reject"
              aria-label="Reject this access request"
              className="flex-1 rounded-lg border px-3 py-2"
              style={{
                borderColor: DARK.hairline,
                color: GRAPH_STAGE.labelSoft,
                fontSize: TYPE.scale.xs,
                fontWeight: 600,
                opacity: busy ? 0.55 : 1,
              }}
            >
              Reject
            </button>
          </div>
          {feedback && (
            <p
              role="status"
              aria-live="polite"
              data-testid="pipeline-gate-feedback"
              className="mt-2"
              style={{ fontSize: TYPE.scale.xs, color: GRAPH_STAGE.labelSoft }}
            >
              {feedback.text}
            </p>
          )}
        </div>
      ) : (
        <div className="mt-3">
          <span
            className="inline-block rounded-full border px-2 py-0.5"
            data-testid="pipeline-gate-chip"
            style={{ borderColor: DARK.hairline, color: GRAPH_STAGE.labelSoft, fontSize: TYPE.scale.xs }}
          >
            {statusLabel(item.status)}
          </span>
          <p
            className="mt-2"
            data-testid="pipeline-gate-note"
            style={{ fontSize: TYPE.scale.xs, color: GRAPH_STAGE.labelSoft }}
          >
            Decisions are recorded in the Review Queue.
          </p>
        </div>
      )}

      <button
        type="button"
        onClick={onOpen}
        data-testid="pipeline-gate-details"
        className="ap-washable mt-3 rounded-lg px-2 py-1"
        style={{ fontSize: TYPE.scale.xs, color: GRAPH_STAGE.labelSoft }}
      >
        Details
      </button>
    </div>
  );
}
