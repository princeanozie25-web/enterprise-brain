"use client";

import { useMemo, useRef, useState } from "react";
import * as api from "@/lib/api";
import type { ProjectWorkflowResponse, WorkflowItem } from "@/lib/api";
import { DARK, GRAPH_STAGE, TYPE } from "@/lib/tokens";
import { ConstellationBackdrop } from "./ConstellationBackdrop";
import { HumanGatePanel } from "./HumanGatePanel";
import { PipelineCard } from "./PipelineCard";
import { PipelineDrawer } from "./PipelineDrawer";
import { STAGES, groupItems } from "./pipeline";

/**
 * SHOWCASE-2 (Track B1/B3) — the pipeline room. Horizontal stage columns over
 * the payload's status groups (flow order), on the fixed deep-navy stage with
 * the constellation behind it. The active-path is the amber OUTLINE on "active"
 * items (the payload has no route history, so there is NO cross-stage spline —
 * flagged). The Waiting column IS the human gate. The drawer opens on any card.
 * Every column/card is payload-derived; the constellation is the sole (aria-
 * hidden) exception.
 */
export function PipelineBoard({
  workflow,
  actor,
  onReload,
}: {
  workflow: ProjectWorkflowResponse | null;
  actor: string | null;
  onReload: () => void | Promise<void>;
}) {
  const [selected, setSelected] = useState<WorkflowItem | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [feedbackFor, setFeedbackFor] = useState<{ id: string; kind: "success" | "error"; text: string } | null>(null);
  const columnRefs = useRef<Array<HTMLDivElement | null>>([]);

  const grouped = useMemo(() => (workflow ? groupItems(workflow.items) : null), [workflow]);
  const actorId = workflow?.actor_id ?? actor ?? null;

  // Live decision is offered only when the viewer is THIS item's approver and it
  // is a pending access_request (the one kind with a real, approver-gated
  // decision endpoint). The server re-checks ApprovalGated regardless.
  const canDecide = (item: WorkflowItem): boolean =>
    actorId !== null &&
    item.kind === "access_request" &&
    item.approver_id === actorId &&
    item.status.toLowerCase() === "pending";

  const decide = async (item: WorkflowItem, decision: "approve" | "deny") => {
    if (actorId === null) return;
    setBusyId(item.item_id);
    setFeedbackFor(null);
    try {
      await api.postAccessRequestDecision(actorId, item.item_id, decision);
      setFeedbackFor({
        id: item.item_id,
        kind: "success",
        text: decision === "approve" ? "Approved. Recorded in the Review Queue." : "Rejected. Recorded in the Review Queue.",
      });
      await onReload();
    } catch {
      setFeedbackFor({ id: item.item_id, kind: "error", text: "Decision was not recorded. Refresh and try again." });
    } finally {
      setBusyId(null);
    }
  };

  const focusColumn = (index: number) => columnRefs.current[index]?.focus();

  const stageBg = `radial-gradient(130% 100% at 50% -10%, ${GRAPH_STAGE.canvasCenter}, ${GRAPH_STAGE.canvasEdge})`;

  if (workflow === null || grouped === null) {
    return (
      <p className="py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="pipeline-unavailable">
        Pipeline is not available for this project.
      </p>
    );
  }

  return (
    <section
      className="relative overflow-hidden rounded-2xl"
      data-testid="pipeline-board"
      style={{ background: stageBg, color: GRAPH_STAGE.label }}
    >
      <ConstellationBackdrop />

      {workflow.items.length === 0 ? (
        <div
          className="relative p-10 text-center"
          data-testid="pipeline-empty"
          style={{ fontSize: TYPE.scale.sm, color: GRAPH_STAGE.labelSoft }}
        >
          No work items are in scope for this project yet.
        </div>
      ) : (
        <div
          className="relative flex overflow-x-auto p-5"
          style={{ gap: 48, scrollSnapType: "x mandatory" }}
          data-testid="pipeline-columns"
        >
          {STAGES.map((stage, index) => {
            const items = grouped.get(stage.key) ?? [];
            return (
              <div
                key={stage.key}
                ref={(el) => {
                  columnRefs.current[index] = el;
                }}
                role="group"
                tabIndex={0}
                aria-label={`Stage ${stage.ordinal}: ${stage.name}, ${items.length} ${items.length === 1 ? "item" : "items"}`}
                data-testid="pipeline-column"
                data-stage={stage.key}
                onKeyDown={(event) => {
                  // Only arrows landing ON the column itself drive column nav.
                  // A card/button inside the column bubbles its keydown here;
                  // without this guard, arrowing a focused card would yank focus
                  // to a sibling column and swallow the browser's scroll.
                  if (event.target !== event.currentTarget) return;
                  if (event.key === "ArrowRight" || event.key === "ArrowDown") {
                    event.preventDefault();
                    focusColumn(Math.min(index + 1, STAGES.length - 1));
                  } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
                    event.preventDefault();
                    focusColumn(Math.max(index - 1, 0));
                  }
                }}
                className="shrink-0 outline-none"
                style={{ width: 320, scrollSnapAlign: "start" }}
              >
                <div className="mb-3">
                  <p
                    className="uppercase"
                    data-testid="pipeline-stage-eyebrow"
                    style={{ fontSize: TYPE.scale.xs - 2, fontWeight: 700, letterSpacing: "0.14em", color: GRAPH_STAGE.labelSoft }}
                  >
                    STAGE {stage.ordinal}
                  </p>
                  <div className="mt-1 flex items-center justify-between gap-2">
                    <h2
                      data-testid="pipeline-stage-name"
                      style={{ fontSize: TYPE.scale.md + 2, fontWeight: 650, lineHeight: TYPE.line.display, color: GRAPH_STAGE.label }}
                    >
                      {stage.name}
                    </h2>
                    <span
                      className="shrink-0 rounded-full border px-2 py-0.5"
                      data-testid="pipeline-stage-count"
                      style={{ borderColor: DARK.hairline, fontSize: TYPE.scale.xs, color: GRAPH_STAGE.labelSoft }}
                    >
                      {items.length}
                    </span>
                  </div>
                  <p
                    className="mt-1"
                    data-testid="pipeline-stage-subtitle"
                    style={{ fontSize: TYPE.scale.xs, color: GRAPH_STAGE.labelSoft }}
                  >
                    {stage.humanGate
                      ? "the only checkpoint — a human decides"
                      : `${items.length} ${items.length === 1 ? "item" : "items"}`}
                  </p>
                </div>

                <div className="space-y-2">
                  {items.length === 0 ? (
                    <p
                      className="rounded-lg border px-3 py-4 text-center"
                      data-testid="pipeline-column-empty"
                      style={{ borderColor: DARK.hairline, fontSize: TYPE.scale.xs, color: GRAPH_STAGE.labelSoft }}
                    >
                      Nothing in this stage.
                    </p>
                  ) : (
                    items.map((item) =>
                      stage.humanGate && item.kind === "access_request" ? (
                        <HumanGatePanel
                          key={item.item_id}
                          item={item}
                          canDecide={canDecide(item)}
                          busy={busyId === item.item_id}
                          feedback={feedbackFor?.id === item.item_id ? feedbackFor : null}
                          onDecide={(decision) => decide(item, decision)}
                          onOpen={() => setSelected(item)}
                        />
                      ) : (
                        <PipelineCard key={item.item_id} item={item} onOpen={() => setSelected(item)} />
                      ),
                    )
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}

      <PipelineDrawer item={selected} onClose={() => setSelected(null)} />
    </section>
  );
}
