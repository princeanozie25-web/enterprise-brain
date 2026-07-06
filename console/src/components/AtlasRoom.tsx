"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "@/lib/api";
import type { AtlasCapability, AtlasDoc, AtlasResponse, DocCard } from "@/lib/api";
import { COLOR, GEOMETRY, TYPE } from "@/lib/tokens";
import { DocInspector } from "./DocInspector";
import { ExportButton } from "./ExportButton";
import { RoomActor } from "./PersonAvatar";
import { SensitivityBadge } from "./SensitivityBadge";
import { Skeleton } from "./Skeleton";

/**
 * THE ATLAS ROOM — the capability surface. STRUCTURE IS INTERNAL-GRADE;
 * EVIDENCE IS GOVERNED: the whole BRM renders for any actor with standing;
 * each capability carries the viewer's OWN visible documents and no signal
 * about anyone else's. A capability with no visible evidence renders an
 * em-dash — the entire vocabulary of absence. The "+N more" expander counts
 * the viewer's own remainder, which is their produce and legal.
 */
export function AtlasRoom({
  actor,
  entryCapability = null,
}: {
  actor: string | null;
  /** /atlas?cap=… — the ego-ring click-through; opens the sheet once. */
  entryCapability?: string | null;
}) {
  const [atlas, setAtlas] = useState<AtlasResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [available, setAvailable] = useState(true);
  const [sheetCapabilityId, setSheetCapabilityId] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const entrySpent = useRef(false);
  const [inspector, setInspector] = useState<{
    open: boolean;
    loading: boolean;
    card: DocCard | null;
  }>({ open: false, loading: false, card: null });

  useEffect(() => {
    if (actor === null) {
      setAtlas(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setAtlas(null);
    setAvailable(true);
    api
      .getAtlas(actor)
      .then((response) => {
        if (!cancelled) {
          setAtlas(response);
          setAvailable(response !== null);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setAtlas(null);
          setAvailable(false);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [actor]);

  const capabilityIndex = useMemo(() => {
    const index = new Map<string, AtlasCapability>();
    if (atlas) {
      for (const strategy of atlas.strategies) {
        for (const initiative of strategy.initiatives) {
          for (const workflow of initiative.workflows) {
            for (const capability of workflow.capabilities) {
              index.set(capability.id, capability);
            }
          }
        }
      }
    }
    return index;
  }, [atlas]);

  // The entry door is spent on first load — and only if the capability is
  // actually in the actor's structure (no standing, no sheet).
  useEffect(() => {
    if (entrySpent.current || entryCapability === null || atlas === null) {
      return;
    }
    entrySpent.current = true;
    if (capabilityIndex.has(entryCapability)) {
      setSheetCapabilityId(entryCapability);
    }
  }, [atlas, capabilityIndex, entryCapability]);

  const openDoc = useCallback(
    async (docId: string) => {
      if (actor === null) {
        return;
      }
      setInspector({ open: true, loading: true, card: null });
      try {
        const card = await api.getDoc(actor, docId);
        setInspector({ open: true, loading: false, card });
      } catch {
        setInspector({ open: true, loading: false, card: null });
      }
    },
    [actor],
  );

  const sheetCapability =
    sheetCapabilityId === null ? null : (capabilityIndex.get(sheetCapabilityId) ?? null);

  if (actor === null) {
    // B2: the identity-less state (the prerendered shell) keeps the room's
    // real masthead — one h1 per routed surface, present pre-hydration.
    return (
      <div data-testid="atlas-room-empty">
        <header className="mb-3">
          <h1
            className="ap-register-chrome"
            style={{ fontSize: TYPE.scale.lg, lineHeight: TYPE.line.display, fontWeight: 600 }}
          >
            Company Map
          </h1>
          <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
            How the company is organized.
          </p>
        </header>
        <p className="ap-soft py-2" style={{ fontSize: TYPE.scale.sm }}>
          Choose a Work Identity to begin.
        </p>
      </div>
    );
  }

  return (
    <div data-testid="atlas-room">
      <header className="mb-3">
        <h1
          className="ap-register-chrome"
          style={{ fontSize: TYPE.scale.lg, lineHeight: TYPE.line.display, fontWeight: 600 }}
        >
          Company Map
        </h1>
        <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
          How the company is organized.
        </p>
      </header>

      {/* AR-1: who you are viewing as (display only; absent with no layer). */}
      <RoomActor card={atlas?.actor ?? null} />

      {loading && (
        <div className="ap-card rounded-lg p-4">
          <Skeleton lines={4} />
        </div>
      )}

      {!loading && !available && (
        <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="atlas-unavailable">
          The atlas isn&apos;t available.
        </p>
      )}

      {!loading && atlas && atlas.strategies.length === 0 && (
        <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="atlas-empty">
          Nothing is visible for this Work Identity.
        </p>
      )}

      {!loading &&
        atlas &&
        atlas.strategies.map((strategy) => (
          <section key={strategy.id} className="ap-card mt-4 rounded-lg p-3" data-testid="strategy-band">
            <div className="flex items-baseline justify-between gap-3 px-1 pb-2">
              <h2
                className="ap-register-chrome"
                style={{ fontSize: TYPE.scale.md, fontWeight: 600 }}
                data-testid="band-name"
              >
                {strategy.name}
              </h2>
              <span
                className="ap-register-evidence ap-soft shrink-0"
                style={{ fontSize: TYPE.scale.xs }}
                data-testid="band-id"
              >
                {strategy.id}
              </span>
            </div>
            <div className="flex flex-wrap items-start gap-3">
              {strategy.initiatives.map((initiative) => (
                <div
                  key={initiative.id}
                  className="min-w-0 flex-1"
                  style={{ minWidth: GEOMETRY.atlasColumnMin }}
                  data-testid="initiative-column"
                >
                  <div className="flex items-baseline justify-between gap-2 px-1">
                    <h3
                      className="ap-register-chrome"
                      style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
                    >
                      {initiative.name}
                    </h3>
                    <span
                      className="ap-register-evidence ap-soft shrink-0"
                      style={{ fontSize: TYPE.scale.xs }}
                      data-testid="column-id"
                    >
                      {initiative.id}
                    </span>
                  </div>
                  {initiative.workflows.map((workflow) => (
                    <div key={workflow.id} className="mt-2" data-testid="workflow-group">
                      <div className="flex items-baseline justify-between gap-2 px-1">
                        <h4
                          className="ap-soft uppercase tracking-wide"
                          style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
                        >
                          {workflow.name}
                        </h4>
                        <span
                          className="ap-register-evidence ap-soft shrink-0"
                          style={{ fontSize: TYPE.scale.xs }}
                          data-testid="group-id"
                        >
                          {workflow.id}
                        </span>
                      </div>
                      <div className="mt-1 space-y-2">
                        {workflow.capabilities.map((capability) => (
                          <CapabilityCard
                            key={capability.id}
                            capability={capability}
                            expanded={expanded.has(capability.id)}
                            onExpand={() =>
                              setExpanded((prev) => new Set(prev).add(capability.id))
                            }
                            onOpen={() => setSheetCapabilityId(capability.id)}
                          />
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              ))}
            </div>
          </section>
        ))}

      {sheetCapability && atlas && (
        <CapabilitySheet
          capability={sheetCapability}
          actor={actor}
          snapshotVersion={atlas.snapshot_version}
          onClose={() => setSheetCapabilityId(null)}
          onOpenDoc={openDoc}
        />
      )}

      <DocInspector
        open={inspector.open}
        loading={inspector.loading}
        card={inspector.card}
        onClose={() => setInspector({ open: false, loading: false, card: null })}
        onOpenDoc={openDoc}
      />
    </div>
  );
}

function CapabilityCard({
  capability,
  expanded,
  onExpand,
  onOpen,
}: {
  capability: AtlasCapability;
  expanded: boolean;
  onExpand: () => void;
  onOpen: () => void;
}) {
  const shown = expanded ? capability.docs : capability.docs.slice(0, GEOMETRY.atlasPreviewRows);
  // The viewer's OWN remainder — their produce, legal to count.
  const remainder = capability.docs.length - shown.length;
  return (
    <div className="ap-card rounded-lg p-2" data-testid="capability-card">
      <button
        type="button"
        onClick={onOpen}
        className="ap-washable block w-full rounded-lg text-left"
        data-testid="capability-open"
      >
        <span
          className="ap-register-chrome block truncate"
          style={{ fontSize: TYPE.scale.sm, fontWeight: 500 }}
        >
          {capability.name}
        </span>
        <span
          className="ap-register-evidence ap-soft"
          style={{ fontSize: TYPE.scale.xs }}
          data-testid="capability-id"
        >
          {capability.id}
        </span>
      </button>
      <div className="mt-1.5" data-testid="capability-evidence">
        {capability.docs.length === 0 ? (
          /* The em-dash is the ENTIRE vocabulary of absence: no count, no
             placeholder, no sentence implying hidden content. */
          <span className="ap-soft" style={{ fontSize: TYPE.scale.sm }} data-testid="capability-empty">
            —
          </span>
        ) : (
          <>
            {shown.map((doc) => (
              <CardDocRow key={doc.document_id} doc={doc} />
            ))}
            {remainder > 0 && (
              <button
                type="button"
                onClick={onExpand}
                className="ap-affordance-text ap-register-chrome mt-1 underline"
                style={{ fontSize: TYPE.scale.xs }}
                data-testid="capability-more"
              >
                +{remainder} more
              </button>
            )}
          </>
        )}
      </div>
    </div>
  );
}

function CardDocRow({ doc }: { doc: AtlasDoc }) {
  return (
    <div className="flex items-center gap-2 py-0.5" data-testid="card-doc-row">
      <span
        className="ap-register-chrome min-w-0 flex-1 truncate"
        style={{
          fontSize: TYPE.scale.xs,
          textDecoration: doc.superseded ? "line-through" : undefined,
          color: doc.superseded ? COLOR.inkSoft : undefined,
        }}
      >
        {doc.title}
      </span>
      <SensitivityBadge sensitivity={doc.sensitivity} />
    </div>
  );
}

/**
 * The capability sheet: same anatomy as the doc inspector's container
 * (fixed right, 420px, hairline header) — one layer beneath it, so a doc
 * row opens the inspector ON TOP of the sheet.
 */
function CapabilitySheet({
  capability,
  actor,
  snapshotVersion,
  onClose,
  onOpenDoc,
}: {
  capability: AtlasCapability;
  actor: string | null;
  snapshotVersion: string;
  onClose: () => void;
  onOpenDoc: (docId: string) => void;
}) {
  return (
    <aside
      className="ap-card fixed inset-y-0 right-0 z-10 flex flex-col border-y-0 border-r-0"
      style={{ width: GEOMETRY.atlasSheetWidth }}
      data-testid="capability-sheet"
    >
      <div className="ap-hairline flex items-center justify-between gap-2 border-b px-4 py-3">
        <h2
          className="ap-register-chrome ap-soft"
          style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
        >
          Capability
        </h2>
        <span className="ml-auto">
          <ExportButton
            actor={actor}
            request={{
              view: "atlas_capability",
              atlas_capability: { capability_id: capability.id },
            }}
            filename={api.exportFilename("atlas_capability", capability.id, snapshotVersion)}
          />
        </span>
        <button
          type="button"
          onClick={onClose}
          className="ap-washable ap-soft rounded-lg px-2 py-0.5"
          style={{ fontSize: TYPE.scale.sm }}
          data-testid="sheet-close"
        >
          Close
        </button>
      </div>
      <div className="flex-1 overflow-y-auto p-4">
        <h3
          className="ap-register-chrome"
          style={{ fontSize: TYPE.scale.md, fontWeight: 600 }}
          data-testid="sheet-name"
        >
          {capability.name}
        </h3>
        <p className="ap-register-evidence ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
          {capability.id}
        </p>
        <div className="mt-3">
          {capability.docs.length === 0 ? (
            <span className="ap-soft" style={{ fontSize: TYPE.scale.sm }} data-testid="sheet-empty">
              —
            </span>
          ) : (
            <ol>
              {capability.docs.map((doc) => (
                <SheetDocRow key={doc.document_id} doc={doc} onOpenDoc={onOpenDoc} />
              ))}
            </ol>
          )}
        </div>
      </div>
    </aside>
  );
}

function SheetDocRow({
  doc,
  onOpenDoc,
}: {
  doc: AtlasDoc;
  onOpenDoc: (docId: string) => void;
}) {
  return (
    <li>
      <div className="ap-washable flex w-full items-center gap-3 px-1 py-1.5">
        <button
          type="button"
          onClick={() => onOpenDoc(doc.document_id)}
          className="flex min-w-0 flex-1 items-center gap-3 text-left"
          data-testid="sheet-doc-row"
        >
          <span className="min-w-0 flex-1">
            <span
              className="ap-register-chrome block truncate"
              style={{
                fontSize: TYPE.scale.sm,
                textDecoration: doc.superseded ? "line-through" : undefined,
                color: doc.superseded ? COLOR.inkSoft : undefined,
              }}
            >
              {doc.title}
            </span>
            <span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
              {doc.document_id}
            </span>
          </span>
          <SensitivityBadge sensitivity={doc.sensitivity} />
        </button>
        {/* Redaction honored: a missing successor field renders NOTHING. */}
        {doc.superseded && doc.effective_successor && (
          <button
            type="button"
            onClick={() => onOpenDoc(doc.effective_successor!)}
            className="ap-register-evidence ap-affordance-text underline"
            style={{ fontSize: TYPE.scale.xs }}
            data-testid="sheet-successor-link"
          >
            {doc.effective_successor}
          </button>
        )}
      </div>
    </li>
  );
}
