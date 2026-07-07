"use client";

import type { WorkflowItem } from "@/lib/api";
import { DARK, FONT, GRAPH_STAGE, TYPE } from "@/lib/tokens";
import { PersonAvatar } from "../PersonAvatar";
import {
  KIND_LABEL,
  PASTEL_GREEN,
  isActive,
  isDone,
  itemActors,
  stageDot,
  stageForStatus,
  statusLabel,
} from "./pipeline";

/**
 * SHOWCASE-2 (Track B2) — a pipeline card. Every element is payload-derived:
 * status dot + title, the mono item_id, the kind label (there is NO description
 * field in the payload, so that slot is omitted, not invented), and a footer of
 * the item's owner/approver (PersonAvatar 20px) + dependency count. Active items
 * (status "active") wear the amber outline (the reserved signal); done items get
 * the soft-green check. The whole card is a button — click / Enter opens the
 * detail drawer. Solid fixed-navy surface + pinned light ink (both themes).
 */
export function PipelineCard({ item, onOpen }: { item: WorkflowItem; onOpen: () => void }) {
  const active = isActive(item);
  const done = isDone(item);
  const dot = stageDot(stageForStatus(item.status));
  const actor = itemActors(item)[0] ?? null;

  return (
    <button
      type="button"
      onClick={onOpen}
      className="ap-washable block w-full rounded-2xl border p-3 text-left"
      style={{
        background: DARK.paper,
        borderColor: active ? "var(--accent-warm)" : DARK.hairline,
        borderWidth: active ? 1.5 : 1,
        color: GRAPH_STAGE.label,
      }}
      aria-label={`${item.title}, ${statusLabel(item.status)}. Open details.`}
      data-testid="pipeline-card"
      data-item-id={item.item_id}
      data-status={item.status}
      data-active={active ? "true" : "false"}
      data-kind={item.kind}
    >
      <div className="flex items-start gap-2">
        <span
          className="mt-1 shrink-0 rounded-full"
          aria-hidden="true"
          data-testid="pipeline-card-dot"
          style={{
            height: 9,
            width: 9,
            background: dot.filled ? dot.fill : "transparent",
            border: `1.5px solid ${dot.fill}`,
          }}
        />
        <span
          className="min-w-0 flex-1"
          style={{ fontSize: TYPE.scale.sm, fontWeight: 600, lineHeight: TYPE.line.body }}
          data-testid="pipeline-card-title"
        >
          {item.title}
        </span>
        {done && (
          <svg
            width={16}
            height={16}
            viewBox="0 0 24 24"
            aria-hidden="true"
            data-testid="pipeline-card-done"
            style={{ opacity: 0.7, flexShrink: 0 }}
          >
            <path
              d="M20 6L9 17l-5-5"
              fill="none"
              stroke={PASTEL_GREEN}
              strokeWidth={2.5}
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        )}
      </div>

      <p
        className="mt-2 truncate"
        data-testid="pipeline-card-id"
        style={{ fontFamily: FONT.evidence, fontSize: TYPE.scale.xs - 2, color: GRAPH_STAGE.labelSoft }}
      >
        {item.item_id}
      </p>
      <p className="mt-0.5" style={{ fontSize: TYPE.scale.xs, color: GRAPH_STAGE.labelSoft }}>
        {KIND_LABEL[item.kind]}
      </p>

      <div className="mt-3 flex items-center justify-between gap-2">
        {actor ? (
          <span className="flex min-w-0 items-center gap-1.5" data-testid="pipeline-card-actor">
            <PersonAvatar principalId={actor.id} size={20} />
            <span
              className="truncate"
              style={{ fontFamily: FONT.evidence, fontSize: TYPE.scale.xs - 2, color: GRAPH_STAGE.labelSoft }}
            >
              {actor.role} {actor.id}
            </span>
          </span>
        ) : (
          <span />
        )}
        {item.dependencies.length > 0 && (
          <span
            className="shrink-0"
            data-testid="pipeline-card-deps"
            style={{ fontSize: TYPE.scale.xs - 1, color: GRAPH_STAGE.labelSoft }}
          >
            {item.dependencies.length} dep{item.dependencies.length === 1 ? "" : "s"}
          </span>
        )}
      </div>
    </button>
  );
}
