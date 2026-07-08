"use client";

import type React from "react";
import type { WorkflowItem } from "@/lib/api";
import { DARK, FONT, GRAPH_STAGE, TYPE } from "@/lib/tokens";
import { useModalDialogFocus } from "../A11yDialog";
import { PersonAvatar } from "../PersonAvatar";
import { KIND_LABEL, itemActors, statusLabel } from "./pipeline";

/**
 * SHOWCASE-2 (Track B5) — the detail drawer. Renders EVERY payload field and
 * nothing else: title, ids (evidence register), status, capability, the full
 * provenance chain, the item's people, and dependencies. Where the payload has
 * no data (it carries no document references, no timestamps, no status history),
 * the drawer says so plainly instead of inventing a section — its completeness
 * is bounded by the payload and it admits the bound. A11yDialog: focus trap,
 * Escape, focus return. Mounted after the columns so its <h2> follows the
 * surface <h1> and stage <h2>s (heading discipline holds when open).
 */
function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <h3
      className="uppercase"
      style={{ fontSize: TYPE.scale.xs - 2, fontWeight: 700, letterSpacing: "0.08em", color: GRAPH_STAGE.labelSoft }}
    >
      {children}
    </h3>
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return (
    <p className="mt-1" style={{ fontSize: TYPE.scale.xs, fontStyle: "italic", color: GRAPH_STAGE.labelSoft }}>
      {children}
    </p>
  );
}

function Row({ k, v, mono = false }: { k: string; v: string; mono?: boolean }) {
  return (
    <div className="mt-1.5 flex items-baseline justify-between gap-3">
      <span style={{ fontSize: TYPE.scale.xs, color: GRAPH_STAGE.labelSoft }}>{k}</span>
      <span
        className="min-w-0 truncate text-right"
        style={{
          fontSize: TYPE.scale.xs,
          fontWeight: 600,
          fontFamily: mono ? FONT.evidence : undefined,
          color: GRAPH_STAGE.label,
        }}
      >
        {v}
      </span>
    </div>
  );
}

export function PipelineDrawer({ item, onClose }: { item: WorkflowItem | null; onClose: () => void }) {
  const open = item !== null;
  const { dialogRef, onKeyDown } = useModalDialogFocus({ open, onClose });
  if (!open || item === null) return null;
  const actors = itemActors(item);

  return (
    <div
      className="fixed inset-0 z-50 flex justify-end"
      data-testid="pipeline-drawer-overlay"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="ap-glass-scrim absolute inset-0" aria-hidden="true" />
      <aside
        ref={dialogRef as React.RefObject<HTMLElement>}
        onKeyDown={onKeyDown}
        role="dialog"
        aria-modal="true"
        aria-label={`Details: ${item.title}`}
        tabIndex={-1}
        className="relative flex h-full w-full max-w-md flex-col gap-4 overflow-y-auto p-5"
        data-testid="pipeline-drawer"
        style={{ background: DARK.paper, color: GRAPH_STAGE.label, borderLeft: `1px solid ${DARK.hairline}` }}
      >
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <p
              className="uppercase"
              style={{ fontSize: TYPE.scale.xs - 2, letterSpacing: "0.08em", color: GRAPH_STAGE.labelSoft }}
            >
              {KIND_LABEL[item.kind]}
            </p>
            <h2 className="mt-1" style={{ fontSize: TYPE.scale.md, fontWeight: 700, lineHeight: TYPE.line.body }}>
              {item.title}
            </h2>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close details"
            data-testid="pipeline-drawer-close"
            className="ap-washable shrink-0 rounded-lg px-2 py-1"
            style={{ fontSize: TYPE.scale.xs, fontWeight: 600, color: GRAPH_STAGE.labelSoft }}
          >
            Close
          </button>
        </div>

        <section data-testid="pipeline-drawer-facts">
          <Row k="Status" v={statusLabel(item.status)} />
          <Row k="Item id" v={item.item_id} mono />
          <Row k="Capability id" v={item.capability_id} mono />
          <Row k="Kind" v={KIND_LABEL[item.kind]} />
          {item.proposal_id && <Row k="From proposal" v={item.proposal_id} mono />}
        </section>

        {item.description && (
          <section data-testid="pipeline-drawer-description">
            <SectionTitle>What to do</SectionTitle>
            <p className="mt-1" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body, color: GRAPH_STAGE.label }}>
              {item.description}
            </p>
          </section>
        )}

        <section data-testid="pipeline-drawer-provenance">
          <SectionTitle>Provenance</SectionTitle>
          <Row k="Strategy" v={item.provenance.strategy.name} />
          <Row k="Initiative" v={item.provenance.initiative.name} />
          <Row k="Workflow" v={item.provenance.workflow.name} />
          <Row k="Capability" v={item.provenance.capability.name} />
        </section>

        <section data-testid="pipeline-drawer-people">
          <SectionTitle>People</SectionTitle>
          {actors.length === 0 ? (
            <Empty>No people are attached to this item.</Empty>
          ) : (
            <div className="mt-2 space-y-2">
              {actors.map((actor) => (
                <div key={`${actor.role}:${actor.id}`} className="flex items-center gap-2">
                  <PersonAvatar principalId={actor.id} size={24} />
                  <span style={{ fontFamily: FONT.evidence, fontSize: TYPE.scale.xs, color: GRAPH_STAGE.label }}>
                    {actor.role} · {actor.id}
                  </span>
                </div>
              ))}
            </div>
          )}
        </section>

        <section data-testid="pipeline-drawer-deps">
          <SectionTitle>Dependencies</SectionTitle>
          {item.dependencies.length === 0 ? (
            <Empty>No dependencies in this payload.</Empty>
          ) : (
            <div className="mt-2 flex flex-wrap gap-1.5">
              {item.dependencies.map((dependency) => (
                <span
                  key={dependency}
                  className="rounded-full border px-2 py-0.5"
                  style={{
                    fontFamily: FONT.evidence,
                    fontSize: TYPE.scale.xs - 1,
                    borderColor: DARK.hairline,
                    color: GRAPH_STAGE.labelSoft,
                  }}
                >
                  {dependency}
                </span>
              ))}
            </div>
          )}
        </section>

        <section data-testid="pipeline-drawer-documents">
          <SectionTitle>Linked documents</SectionTitle>
          {item.anchors && item.anchors.length > 0 ? (
            <div className="mt-2 space-y-2" data-testid="pipeline-drawer-anchors">
              {item.anchors.map((anchor, index) =>
                anchor.visible && anchor.doc_id ? (
                  <div key={index}>
                    <p style={{ fontFamily: FONT.evidence, fontSize: TYPE.scale.xs, color: GRAPH_STAGE.label }}>
                      {anchor.doc_id}
                      {anchor.locator ? ` · ${anchor.locator}` : ""}
                    </p>
                    {anchor.quote && (
                      <p
                        className="mt-0.5"
                        style={{ fontFamily: FONT.evidence, fontSize: TYPE.scale.xs - 1, color: GRAPH_STAGE.labelSoft }}
                      >
                        &ldquo;{anchor.quote}&rdquo;
                      </p>
                    )}
                  </div>
                ) : (
                  /* S4: an out-of-scope anchor crosses as a marker, never content. */
                  <p
                    key={index}
                    data-testid="pipeline-drawer-anchor-withheld"
                    style={{ fontSize: TYPE.scale.xs, fontStyle: "italic", color: GRAPH_STAGE.labelSoft }}
                  >
                    A source outside your view — withheld.
                  </p>
                ),
              )}
              {typeof item.sources_outside_view === "number" && item.sources_outside_view > 0 && (
                <p style={{ fontSize: TYPE.scale.xs, fontStyle: "italic", color: GRAPH_STAGE.labelSoft }}>
                  {item.sources_outside_view}{" "}
                  {item.sources_outside_view === 1 ? "source stays" : "sources stay"} hidden from
                  your identity.
                </p>
              )}
            </div>
          ) : (
            <Empty>This workflow payload carries no document references, timestamps, or status history.</Empty>
          )}
        </section>
      </aside>
    </div>
  );
}
