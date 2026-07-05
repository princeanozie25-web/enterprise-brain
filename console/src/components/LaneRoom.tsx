"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import * as api from "@/lib/api";
import type {
  DocCard,
  DiffDocRow,
  InboxResponse,
  LaneBox,
  LaneResponse,
  RollupResponse,
  ScopeStatement,
} from "@/lib/api";
import { COLOR, DERIVED, TYPE } from "@/lib/tokens";
import { DocInspector } from "./DocInspector";
import { RoomActor } from "./PersonAvatar";
import { SensitivityBadge } from "./SensitivityBadge";
import { Skeleton } from "./Skeleton";

/**
 * THE LANE (AP-6) — the v4a Workflow Surface, DISPLAY ONLY: a single calm
 * column of governed boxes. Status changes are the worker's own acts;
 * agent proposals sit in an inbox until explicitly accepted; a box bound
 * to a withdrawn procedure renders BLOCKED and hints at nothing; every box
 * carries its honesty line. No amber exists anywhere in this room — the
 * v4b door stays visibly shut (U-28).
 */
export function LaneRoom({ actor }: { actor: string | null }) {
  const [lane, setLane] = useState<LaneResponse | null>(null);
  const [inbox, setInbox] = useState<InboxResponse | null>(null);
  const [rollup, setRollup] = useState<RollupResponse | null>(null);
  const [showRollup, setShowRollup] = useState(false);
  const [showDismissed, setShowDismissed] = useState(false);
  const [explained, setExplained] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [available, setAvailable] = useState(true);
  const [inspector, setInspector] = useState<{
    open: boolean;
    loading: boolean;
    card: DocCard | null;
  }>({ open: false, loading: false, card: null });

  const refetch = useCallback(() => {
    if (actor === null) {
      setLane(null);
      setInbox(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    api
      .getLane(actor)
      .then((response) => {
        if (!cancelled) {
          setLane(response);
          setAvailable(response !== null);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setLane(null);
          setAvailable(false);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });
    api
      .getInbox(actor)
      .then((response) => {
        if (!cancelled) {
          setInbox(response);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setInbox(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [actor]);

  useEffect(() => {
    return refetch();
  }, [refetch]);

  useEffect(() => {
    if (!showRollup || actor === null) {
      return;
    }
    let cancelled = false;
    api
      .getRollup(actor)
      .then((response) => {
        if (!cancelled) {
          setRollup(response);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setRollup(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [showRollup, actor]);

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

  const setStatus = useCallback(
    async (boxId: string, to: "active" | "done" | "dismissed") => {
      if (actor === null) {
        return;
      }
      try {
        await api.postBoxStatus(actor, boxId, to);
      } catch {
        // The service refused (illegal transition, blocked box): the lane
        // re-renders the truth; the console explains nothing extra.
      }
      refetch();
    },
    [actor, refetch],
  );

  const decideInbox = useCallback(
    async (proposalId: string, decision: "accept" | "dismiss") => {
      if (actor === null) {
        return;
      }
      try {
        await api.postInboxDecision(actor, proposalId, decision);
      } catch {
        // Refusals render as the unchanged inbox.
      }
      refetch();
    },
    [actor, refetch],
  );

  // Status order: active, candidate, blocked, done; dismissed behind the
  // quiet toggle. Stable within a status (the service's derivation order).
  const ordered = useMemo(() => {
    if (lane === null) {
      return [];
    }
    const rank: Record<string, number> = {
      active: 0,
      candidate: 1,
      blocked: 2,
      done: 3,
      dismissed: 4,
    };
    return lane.boxes
      .filter((box) => showDismissed || box.status !== "dismissed")
      .map((box, index) => ({ box, index }))
      .sort(
        (a, b) =>
          (rank[a.box.status] ?? 9) - (rank[b.box.status] ?? 9) || a.index - b.index,
      )
      .map((entry) => entry.box);
  }, [lane, showDismissed]);

  // The in-lane anchors for explain-this-box: each non-capability crumb
  // jumps to the FIRST box bearing that segment.
  const firstBoxFor = useMemo(() => {
    const map = new Map<string, string>();
    for (const box of ordered) {
      for (const node of [
        box.provenance.strategy,
        box.provenance.initiative,
        box.provenance.workflow,
      ]) {
        if (!map.has(node.id)) {
          map.set(node.id, box.box_id);
        }
      }
    }
    return map;
  }, [ordered]);

  if (actor === null) {
    return (
      <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="lane-room-empty">
        Choose a Work Identity to begin.
      </p>
    );
  }

  return (
    <div data-testid="lane-room">
      <div className="mb-3 flex items-center gap-3">
        <div className="min-w-0">
          <h1
            className="ap-register-chrome"
            style={{ fontSize: TYPE.scale.lg, lineHeight: TYPE.line.display, fontWeight: 600 }}
          >
            Review Queue
          </h1>
          <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
            Items waiting for a human decision.
          </p>
        </div>
        {/* Permanent, same register as the demo caption — furniture. */}
        <span
          className="ap-soft"
          style={{ fontSize: TYPE.scale.xs }}
          data-testid="derived-demo-header"
        >
          Derived assignments (demo)
        </span>
        <span className="ml-auto flex items-center gap-2">
          <button
            type="button"
            onClick={() => setShowDismissed((value) => !value)}
            className="ap-washable ap-soft rounded-lg px-2 py-0.5"
            style={{ fontSize: TYPE.scale.xs }}
            data-testid="toggle-dismissed"
          >
            {showDismissed ? "Hide dismissed" : "Show dismissed"}
          </button>
          <button
            type="button"
            onClick={() => setShowRollup((value) => !value)}
            className="ap-washable ap-soft rounded-lg px-2 py-0.5"
            style={{ fontSize: TYPE.scale.xs }}
            data-testid="rollup-toggle"
          >
            Rollup
          </button>
        </span>
      </div>

      {/* AR-1: the worker's identity (the lane is self-only; display only). */}
      <RoomActor card={lane?.actor ?? null} />

      {showRollup && rollup && (
        <section className="ap-card mb-4 rounded-lg p-3" data-testid="rollup-panel">
          <table className="w-full" data-testid="rollup-table">
            <thead>
              <tr className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
                <th className="px-2 py-1 text-left">capability</th>
                <th className="px-2 py-1 text-right">candidate</th>
                <th className="px-2 py-1 text-right">active</th>
                <th className="px-2 py-1 text-right">done</th>
                <th className="px-2 py-1 text-right">dismissed</th>
                <th className="px-2 py-1 text-right">blocked</th>
              </tr>
            </thead>
            <tbody>
              {rollup.capabilities.map((row) => (
                <tr key={row.capability_id} data-testid="rollup-row">
                  <td
                    className="ap-register-evidence px-2 py-1"
                    style={{ fontSize: TYPE.scale.xs }}
                  >
                    {row.capability_id}
                  </td>
                  {(["candidate", "active", "done", "dismissed", "blocked"] as const).map(
                    (status) => (
                      <td
                        key={status}
                        className="px-2 py-1 text-right"
                        style={{ fontSize: TYPE.scale.xs }}
                      >
                        {row.status_counts[status] ?? 0}
                      </td>
                    ),
                  )}
                </tr>
              ))}
            </tbody>
          </table>
          <p
            className="ap-soft mt-2 px-2"
            style={{ fontSize: TYPE.scale.xs }}
            data-testid="rollup-honesty"
          >
            {rollup.honesty}
          </p>
        </section>
      )}

      {inbox && inbox.proposals.length > 0 && (
        <section className="ap-card ap-fade-view mb-4 rounded-lg p-3" data-testid="inbox-strip">
          <h2
            className="ap-soft px-1 pb-1 uppercase tracking-wide"
            style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
          >
            Inbox — proposals awaiting your decision
          </h2>
          {inbox.proposals.map((proposal) => (
            <div
              key={proposal.proposal_id}
              className="flex flex-wrap items-center gap-2 px-1 py-1"
              data-testid="inbox-proposal"
            >
              <span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
                {proposal.agent_id}
              </span>
              <span className="ap-register-chrome min-w-0 flex-1" style={{ fontSize: TYPE.scale.sm }}>
                {proposal.standing_query}
              </span>
              <span className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
                {proposal.citations.length} citations
              </span>
              <button
                type="button"
                onClick={() => decideInbox(proposal.proposal_id, "accept")}
                className="ap-affordance-button rounded-lg px-2 py-0.5"
                style={{ fontSize: TYPE.scale.xs }}
                data-testid="inbox-accept"
              >
                Accept
              </button>
              <button
                type="button"
                onClick={() => decideInbox(proposal.proposal_id, "dismiss")}
                className="ap-washable ap-soft rounded-lg px-2 py-0.5"
                style={{ fontSize: TYPE.scale.xs }}
                data-testid="inbox-dismiss"
              >
                Dismiss
              </button>
            </div>
          ))}
        </section>
      )}

      {loading && (
        <div className="ap-card rounded-lg p-4">
          <Skeleton lines={4} />
        </div>
      )}

      {!loading && !available && (
        <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="lane-unavailable">
          The lane isn&apos;t available.
        </p>
      )}

      {!loading && lane && ordered.length === 0 && (
        <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="lane-empty">
          Nothing is assigned for this Work Identity.
        </p>
      )}

      {!loading && (
        <div className="space-y-3">
          {ordered.map((box) => (
            <BoxCard
              key={box.box_id}
              box={box}
              actor={actor}
              explained={explained === box.box_id}
              onExplain={() =>
                setExplained((current) => (current === box.box_id ? null : box.box_id))
              }
              firstBoxFor={firstBoxFor}
              onOpenDoc={openDoc}
              onStatus={setStatus}
            />
          ))}
        </div>
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

function honestyLine(honesty: ScopeStatement): string {
  const groups = honesty.groups.length > 0 ? honesty.groups.join(", ") : "—";
  const sites = honesty.sites.length > 0 ? honesty.sites.join(", ") : "—";
  const band = honesty.band === null || honesty.band === undefined ? "—" : String(honesty.band);
  return `Scope: groups ${groups} · sites ${sites} · band ${band}`;
}

function BoxCard({
  box,
  actor,
  explained,
  onExplain,
  firstBoxFor,
  onOpenDoc,
  onStatus,
}: {
  box: LaneBox;
  actor: string;
  explained: boolean;
  onExplain: () => void;
  firstBoxFor: Map<string, string>;
  onOpenDoc: (docId: string) => void;
  onStatus: (boxId: string, to: "active" | "done" | "dismissed") => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const blocked = box.status === "blocked";
  const shown = expanded ? box.evidence : box.evidence.slice(0, 3);
  const remainder = box.evidence.length - shown.length;

  const crumb = (node: { id: string; name: string }, kind: "anchor" | "atlas") => {
    if (!explained) {
      return <span key={node.id}>{node.name}</span>;
    }
    if (kind === "atlas") {
      const carry = `&as=${encodeURIComponent(actor)}`;
      return (
        <a
          key={node.id}
          href={`/atlas?cap=${encodeURIComponent(node.id)}${carry}`}
          className="ap-affordance-text underline"
          data-testid="crumb-atlas"
        >
          {node.name}
        </a>
      );
    }
    const target = firstBoxFor.get(node.id);
    return (
      <a
        key={node.id}
        href={`#box-${target ?? box.box_id}`}
        className="ap-affordance-text underline"
        data-testid="crumb-anchor"
      >
        {node.name}
      </a>
    );
  };

  return (
    <section
      id={`box-${box.box_id}`}
      className="ap-card ap-fade-view rounded-lg p-3"
      style={blocked ? { borderLeft: `2px solid ${DERIVED.hairline}` } : undefined}
      data-testid="lane-box"
      data-status={box.status}
    >
      <div className="flex items-baseline gap-2">
        <h2
          className="ap-register-chrome min-w-0 flex-1 truncate"
          style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
          data-testid="box-name"
        >
          {box.capability.name}
        </h2>
        <span
          className="ap-hairline ap-register-evidence ap-soft shrink-0 rounded-lg border px-1.5 py-0.5"
          style={{ fontSize: TYPE.scale.xs }}
          data-testid="box-status"
        >
          {box.status}
        </span>
        <button
          type="button"
          onClick={onExplain}
          className="ap-washable ap-soft shrink-0 rounded-lg px-2 py-0.5"
          style={{ fontSize: TYPE.scale.xs }}
          data-testid="box-explain"
        >
          Explain this box
        </button>
      </div>

      <p
        className="ap-register-evidence ap-soft mt-1 truncate"
        style={{ fontSize: TYPE.scale.xs }}
        data-testid="box-breadcrumb"
      >
        {crumb(box.provenance.strategy, "anchor")}
        {" › "}
        {crumb(box.provenance.initiative, "anchor")}
        {" › "}
        {crumb(box.provenance.workflow, "anchor")}
        {" › "}
        {crumb(box.capability, "atlas")}
      </p>

      <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }} data-testid="box-why">
        {box.why}
      </p>

      {blocked && (
        <p
          className="ap-soft mt-2"
          style={{ fontSize: TYPE.scale.xs }}
          data-testid="box-deviation"
        >
          Bound procedure superseded — awaiting effective version
        </p>
      )}

      <div className="mt-2" data-testid="box-evidence">
        {shown.map((row) => (
          <EvidenceRow key={row.document_id} row={row} onOpenDoc={onOpenDoc} />
        ))}
        {!expanded && remainder > 0 && (
          <button
            type="button"
            onClick={() => setExpanded(true)}
            className="ap-affordance-text ap-register-chrome mt-1 underline"
            style={{ fontSize: TYPE.scale.xs }}
            data-testid="box-more"
          >
            +{remainder} more
          </button>
        )}
      </div>

      <div className="mt-2 flex items-center gap-2">
        {box.status === "candidate" && (
          <>
            <button
              type="button"
              onClick={() => onStatus(box.box_id, "active")}
              className="ap-affordance-button rounded-lg px-2 py-0.5"
              style={{ fontSize: TYPE.scale.xs }}
              data-testid="box-action-active"
            >
              Start
            </button>
            <button
              type="button"
              onClick={() => onStatus(box.box_id, "dismissed")}
              className="ap-washable ap-soft rounded-lg px-2 py-0.5"
              style={{ fontSize: TYPE.scale.xs }}
              data-testid="box-action-dismissed"
            >
              Dismiss
            </button>
          </>
        )}
        {box.status === "active" && (
          <button
            type="button"
            onClick={() => onStatus(box.box_id, "done")}
            className="ap-affordance-button rounded-lg px-2 py-0.5"
            style={{ fontSize: TYPE.scale.xs }}
            data-testid="box-action-done"
          >
            Done
          </button>
        )}
      </div>

      <p
        className="ap-soft mt-2"
        style={{ fontSize: TYPE.scale.xs }}
        data-testid="box-honesty"
      >
        {honestyLine(box.honesty)}
      </p>
    </section>
  );
}

function EvidenceRow({
  row,
  onOpenDoc,
}: {
  row: DiffDocRow;
  onOpenDoc: (docId: string) => void;
}) {
  return (
    <div className="ap-washable flex w-full items-center gap-3 px-1 py-1">
      <button
        type="button"
        onClick={() => onOpenDoc(row.document_id)}
        className="flex min-w-0 flex-1 items-center gap-3 text-left"
        data-testid="box-evidence-row"
      >
        <span className="min-w-0 flex-1">
          <span
            className="ap-register-chrome block truncate"
            style={{
              fontSize: TYPE.scale.sm,
              textDecoration: row.superseded ? "line-through" : undefined,
              color: row.superseded ? COLOR.inkSoft : undefined,
            }}
          >
            {row.title}
          </span>
          <span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            {row.document_id}
          </span>
        </span>
        <SensitivityBadge sensitivity={row.sensitivity} />
      </button>
      {/* Redaction honored: a missing successor renders NOTHING. */}
      {row.superseded && row.effective_successor && (
        <button
          type="button"
          onClick={() => onOpenDoc(row.effective_successor!)}
          className="ap-register-evidence ap-affordance-text underline"
          style={{ fontSize: TYPE.scale.xs }}
          data-testid="box-successor-link"
        >
          {row.effective_successor}
        </button>
      )}
    </div>
  );
}
