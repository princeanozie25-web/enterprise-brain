"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "@/lib/api";
import type { AtlasResponse, DocCard, LensDoc, LensResponse, LensSection } from "@/lib/api";
import { PRINCIPALS } from "@/lib/principals";
import { COLOR, TYPE } from "@/lib/tokens";
import { AgentEmblem } from "./AgentEmblem";
import { DiffView } from "./DiffView";
import { DocInspector } from "./DocInspector";
import { EgoGraph } from "./EgoGraph";
import { ExportButton } from "./ExportButton";
import { SensitivityBadge } from "./SensitivityBadge";
import { Skeleton } from "./Skeleton";

/**
 * THE LENS ROOM — a principal's entire governed world, rendered. The ACTOR
 * is the lens bar's principal; the SUBJECT defaults to the actor and can be
 * any principal: crossing is permitted under demo identity but audited
 * server-side before anything renders, and stated plainly here — a quiet
 * line, neutral register, not a warning.
 */
export function LensRoom({
  actor,
  entryDiff = null,
}: {
  actor: string | null;
  /** /lens?diff=… — the compare entry door; opens the diff view once. */
  entryDiff?: string | null;
}) {
  const [subject, setSubject] = useState<string | null>(actor);
  const [lens, setLens] = useState<LensResponse | null>(null);
  const [atlas, setAtlas] = useState<AtlasResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [available, setAvailable] = useState(true);
  const [subjectSearch, setSubjectSearch] = useState("");
  const [diffRight, setDiffRight] = useState<string | null>(null);
  const [compareSearch, setCompareSearch] = useState("");
  const entryDiffSpent = useRef(false);
  const [inspector, setInspector] = useState<{
    open: boolean;
    loading: boolean;
    card: DocCard | null;
  }>({ open: false, loading: false, card: null });

  useEffect(() => {
    setSubject(actor);
  }, [actor]);

  // The entry door is spent on first use; an actor switch remounts the room
  // (key={principal}) and Console drops the door, so a diff never survives
  // a lens change.
  useEffect(() => {
    if (entryDiffSpent.current || entryDiff === null) {
      return;
    }
    entryDiffSpent.current = true;
    setDiffRight(entryDiff);
  }, [entryDiff]);

  useEffect(() => {
    if (diffRight !== null) {
      // The diff view owns the room; the lens body refetches fresh when it
      // returns (diffRight back to null re-runs this effect).
      return;
    }
    if (actor === null || subject === null) {
      setLens(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setLens(null);
    setAvailable(true);
    api
      .getLens(actor, subject)
      .then((response) => {
        if (!cancelled) {
          setLens(response);
          setAvailable(response !== null);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setLens(null);
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
  }, [actor, subject, diffRight]);

  // AP-3: the actor's own atlas, fetched alongside the lens. It feeds the
  // ego-graph's capability ring and nothing else in this room.
  useEffect(() => {
    if (actor === null) {
      setAtlas(null);
      return;
    }
    let cancelled = false;
    setAtlas(null);
    api
      .getAtlas(actor)
      .then((response) => {
        if (!cancelled) {
          setAtlas(response);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setAtlas(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [actor]);

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

  // AP-3 RING DOCTRINE: capability nodes render where the SUBJECT's visible
  // evidence is non-empty. The join uses only what this room already
  // legitimately holds — the audited lens body (the subject's docs) and the
  // ACTOR's own /atlas (the actor's visible docs per capability). On a self
  // view the join is exact; on a cross view it under-renders to
  // capabilities where BOTH lenses see evidence — fail closed, never a
  // signal about documents the actor cannot see.
  const ringCapabilities = useMemo(() => {
    if (lens === null || atlas === null) {
      return [];
    }
    const subjectDocs = new Set(
      lens.holdings.flatMap((section) => section.docs.map((doc) => doc.document_id)),
    );
    const out: string[] = [];
    for (const strategy of atlas.strategies) {
      for (const initiative of strategy.initiatives) {
        for (const workflow of initiative.workflows) {
          for (const capability of workflow.capabilities) {
            if (capability.docs.some((doc) => subjectDocs.has(doc.document_id))) {
              out.push(capability.id);
            }
          }
        }
      }
    }
    return out;
  }, [lens, atlas]);

  const openAtlasCapability = useCallback(
    (capabilityId: string) => {
      // Full-page door into the Atlas room, sheet opened on the capability,
      // same lens carried through.
      const carry = actor === null ? "" : `&as=${encodeURIComponent(actor)}`;
      window.location.href = `/atlas?cap=${encodeURIComponent(capabilityId)}${carry}`;
    },
    [actor],
  );

  const crossTo = useCallback((next: string) => {
    setSubjectSearch("");
    setSubject(next);
  }, []);

  const subjectMatches = useMemo(() => {
    const needle = subjectSearch.trim().toLowerCase();
    if (needle.length === 0) {
      return [];
    }
    return PRINCIPALS.filter((p) => p.toLowerCase().includes(needle)).slice(0, 10);
  }, [subjectSearch]);

  // AP-4: the Compare affordance. Choosing a principal renders the diff
  // view and reflects it in the address bar — /lens?diff=<right> is the
  // same entry door a fresh visit would use.
  const compareMatches = useMemo(() => {
    const needle = compareSearch.trim().toLowerCase();
    if (needle.length === 0) {
      return [];
    }
    return PRINCIPALS.filter(
      (p) => p !== subject && p.toLowerCase().includes(needle),
    ).slice(0, 10);
  }, [compareSearch, subject]);

  const chooseCompare = useCallback(
    (right: string) => {
      setCompareSearch("");
      setDiffRight(right);
      const carry = actor === null ? "" : `&as=${encodeURIComponent(actor)}`;
      window.history.replaceState(null, "", `/lens?diff=${encodeURIComponent(right)}${carry}`);
    },
    [actor],
  );

  const closeDiff = useCallback(() => {
    setDiffRight(null);
    const carry = actor === null ? "" : `?as=${encodeURIComponent(actor)}`;
    window.history.replaceState(null, "", `/lens${carry}`);
  }, [actor]);

  const scrollToSection = useCallback((groupId: string) => {
    // No motion: the budget animates the iris and nothing else.
    document
      .getElementById(`holdings-REBAC:${groupId}`)
      ?.scrollIntoView({ behavior: "auto", block: "start" });
  }, []);

  if (actor === null) {
    return (
      <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="lens-room-empty">
        Select a lens to begin.
      </p>
    );
  }

  // The diff view owns the room while open; leaving it returns to the
  // subject's lens (closeDiff), and an actor switch remounts everything.
  if (diffRight !== null && subject !== null) {
    return (
      <div data-testid="lens-room">
        <DiffView
          actor={actor}
          left={subject}
          right={diffRight}
          onClose={closeDiff}
          onOpenDoc={openDoc}
        />
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

  return (
    <div data-testid="lens-room">
      <div className="mb-3 flex items-center gap-2">
        <h1
          className="ap-register-chrome"
          style={{ fontSize: TYPE.scale.lg, lineHeight: TYPE.line.display, fontWeight: 600 }}
        >
          Lens
        </h1>
        {/* AP-5: masthead-adjacent home — the header row exists while the
            masthead loads, so the disabled state can too (not hidden). */}
        <span className="ml-auto">
          <ExportButton
            actor={actor}
            request={subject === null ? null : { view: "lens", lens: { subject_id: subject } }}
            filename={
              subject !== null && lens !== null
                ? api.exportFilename("lens", subject, lens.snapshot_version)
                : null
            }
            disabled={loading || lens === null}
          />
        </span>
        <div className="relative">
          <input
            value={subjectSearch}
            onChange={(e) => setSubjectSearch(e.target.value)}
            placeholder="View another principal…"
            className="w-56 rounded px-2 py-1"
            style={{ fontSize: TYPE.scale.xs }}
            data-testid="subject-search"
          />
          {subjectMatches.length > 0 && (
            <div className="ap-card ap-fade-view absolute right-0 z-10 mt-1 w-56 rounded">
              {subjectMatches.map((id) => (
                <button
                  key={id}
                  type="button"
                  onClick={() => crossTo(id)}
                  className="ap-washable ap-register-evidence block w-full truncate px-2 py-1 text-left"
                  style={{ fontSize: TYPE.scale.xs }}
                  data-testid="subject-row"
                >
                  {id}
                </button>
              ))}
            </div>
          )}
        </div>
      </div>

      {loading && (
        <div className="ap-card rounded p-4">
          <Skeleton lines={4} />
        </div>
      )}

      {!loading && !available && (
        <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="lens-unavailable">
          This lens isn&apos;t available.
        </p>
      )}

      {!loading && lens && (
        <>
          {/* MASTHEAD — the passport page. */}
          <section className="ap-card rounded p-4" data-testid="masthead">
            <div className="flex flex-wrap items-baseline gap-2">
              <h2
                className="ap-register-chrome"
                style={{
                  fontSize: TYPE.scale.lg,
                  lineHeight: TYPE.line.display,
                  fontWeight: 600,
                }}
                data-testid="masthead-name"
              >
                {lens.subject.name}
              </h2>
              <span
                className="ap-register-evidence ap-soft"
                style={{ fontSize: TYPE.scale.sm }}
                data-testid="masthead-id"
              >
                {lens.subject.id}
              </span>
              <span
                className="ap-hairline ap-register-chrome ap-soft rounded border px-1.5 py-0.5"
                style={{ fontSize: TYPE.scale.xs }}
                data-testid="masthead-kind"
              >
                {lens.subject.kind}
              </span>
              {lens.subject.department && (
                <span className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
                  {lens.subject.department}
                </span>
              )}
              {/* AP-4: the Compare affordance — the subject selector's
                  anatomy, scoped to choosing the diff's right side. */}
              <span className="relative ml-auto">
                <input
                  value={compareSearch}
                  onChange={(e) => setCompareSearch(e.target.value)}
                  placeholder="Compare with…"
                  className="w-44 rounded px-2 py-1"
                  style={{ fontSize: TYPE.scale.xs }}
                  data-testid="compare-search"
                />
                {compareMatches.length > 0 && (
                  <span className="ap-card ap-fade-view absolute right-0 z-10 mt-1 block w-44 rounded">
                    {compareMatches.map((id) => (
                      <button
                        key={id}
                        type="button"
                        onClick={() => chooseCompare(id)}
                        className="ap-washable ap-register-evidence block w-full truncate px-2 py-1 text-left"
                        style={{ fontSize: TYPE.scale.xs }}
                        data-testid="compare-row"
                      >
                        {id}
                      </button>
                    ))}
                  </span>
                )}
              </span>
            </div>
            {lens.cross_lens && (
              <p
                className="ap-soft mt-1"
                style={{ fontSize: TYPE.scale.xs }}
                data-testid="cross-lens-line"
              >
                Viewing as {lens.actor_id} — this view is audited.
              </p>
            )}
            <div className="mt-3 flex flex-wrap gap-1.5">
              {lens.subject.groups.map((group) => (
                <MastheadChip key={group} value={group} />
              ))}
              {lens.subject.sites.map((site) => (
                <MastheadChip key={site} value={site} />
              ))}
              {lens.subject.band !== undefined && lens.subject.band !== null && (
                <MastheadChip value={`band ${lens.subject.band}`} />
              )}
              {lens.subject.owner_user_id && (
                <MastheadChip value={`owner ${lens.subject.owner_user_id}`} />
              )}
            </div>
          </section>

          <div className="mt-4 flex flex-wrap items-start gap-4">
            <section className="ap-card rounded p-3" data-testid="ego-graph-panel">
              <h3
                className="ap-soft uppercase tracking-wide"
                style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
              >
                Connections
              </h3>
              <EgoGraph
                lens={lens}
                onGroupClick={scrollToSection}
                capabilities={ringCapabilities}
                onCapabilityClick={openAtlasCapability}
              />
            </section>

            {lens.agents.length > 0 && (
              <section className="ap-card rounded p-3" data-testid="agents-panel">
                <h3
                  className="ap-soft uppercase tracking-wide"
                  style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
                >
                  Agents
                </h3>
                <div className="mt-2 flex flex-wrap gap-3">
                  {lens.agents.map((agent) => (
                    <AgentEmblem key={agent.agent_id} agent={agent} onNavigate={crossTo} />
                  ))}
                </div>
              </section>
            )}
          </div>

          {/* HOLDINGS — the subject's world, grouped by reason. */}
          <div className="mt-4 space-y-4">
            {lens.holdings.map((section) => (
              <HoldingsSection key={section.reason} section={section} onOpenDoc={openDoc} />
            ))}
          </div>
        </>
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

function MastheadChip({ value }: { value: string }) {
  return (
    <span
      className="ap-card ap-register-chrome inline-block rounded px-2 py-1"
      style={{ fontSize: TYPE.scale.sm, fontWeight: 500 }}
      data-testid="masthead-chip"
    >
      {value}
    </span>
  );
}

function HoldingsSection({
  section,
  onOpenDoc,
}: {
  section: LensSection;
  onOpenDoc: (docId: string) => void;
}) {
  return (
    <section className="ap-card rounded p-3" id={`holdings-${section.reason}`}>
      <div className="flex items-baseline justify-between gap-3 px-2 pb-1">
        <h3 className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }} data-testid="section-sentence">
          {section.sentence}
        </h3>
        <span
          className="ap-hairline ap-register-evidence ap-soft shrink-0 rounded border px-1.5 py-0.5"
          style={{ fontSize: TYPE.scale.xs }}
          data-testid="section-rule"
        >
          {section.reason}
        </span>
      </div>
      <ol>
        {section.docs.map((doc) => (
          <LensDocRow key={doc.document_id} doc={doc} onOpenDoc={onOpenDoc} />
        ))}
      </ol>
    </section>
  );
}

function LensDocRow({
  doc,
  onOpenDoc,
}: {
  doc: LensDoc;
  onOpenDoc: (docId: string) => void;
}) {
  return (
    <li>
      <div className="ap-washable flex w-full items-center gap-3 px-2 py-1.5">
        <button
          type="button"
          onClick={() => onOpenDoc(doc.document_id)}
          className="flex min-w-0 flex-1 items-center gap-3 text-left"
          data-testid="lens-doc-row"
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
            <span
              className="ap-register-evidence ap-soft"
              style={{ fontSize: TYPE.scale.xs }}
            >
              {doc.document_id}
            </span>
          </span>
          <SensitivityBadge sensitivity={doc.sensitivity} />
        </button>
        <span className="flex shrink-0 items-center gap-1">
          {doc.superseded && doc.effective_successor && (
            <button
              type="button"
              onClick={() => onOpenDoc(doc.effective_successor!)}
              className="ap-register-evidence ap-affordance-text underline"
              style={{ fontSize: TYPE.scale.xs }}
              data-testid="effective-version-link"
            >
              {doc.effective_successor}
            </button>
          )}
          {doc.also_via.map((reason) => (
            <span
              key={reason}
              className="ap-hairline ap-register-evidence ap-soft rounded border px-1 py-0.5"
              style={{ fontSize: TYPE.scale.xs }}
              data-testid="also-via-chip"
            >
              {reason}
            </span>
          ))}
        </span>
      </div>
    </li>
  );
}
