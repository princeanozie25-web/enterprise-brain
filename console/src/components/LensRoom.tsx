"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import * as api from "@/lib/api";
import type { DocCard, LensDoc, LensResponse, LensSection } from "@/lib/api";
import { PRINCIPALS } from "@/lib/principals";
import { COLOR, TYPE } from "@/lib/tokens";
import { AgentEmblem } from "./AgentEmblem";
import { DocInspector } from "./DocInspector";
import { EgoGraph } from "./EgoGraph";
import { SensitivityBadge } from "./SensitivityBadge";
import { Skeleton } from "./Skeleton";

/**
 * THE LENS ROOM — a principal's entire governed world, rendered. The ACTOR
 * is the lens bar's principal; the SUBJECT defaults to the actor and can be
 * any principal: crossing is permitted under demo identity but audited
 * server-side before anything renders, and stated plainly here — a quiet
 * line, neutral register, not a warning.
 */
export function LensRoom({ actor }: { actor: string | null }) {
  const [subject, setSubject] = useState<string | null>(actor);
  const [lens, setLens] = useState<LensResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [available, setAvailable] = useState(true);
  const [subjectSearch, setSubjectSearch] = useState("");
  const [inspector, setInspector] = useState<{
    open: boolean;
    loading: boolean;
    card: DocCard | null;
  }>({ open: false, loading: false, card: null });

  useEffect(() => {
    setSubject(actor);
  }, [actor]);

  useEffect(() => {
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
  }, [actor, subject]);

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

  return (
    <div data-testid="lens-room">
      <div className="mb-3 flex items-center gap-2">
        <h1
          className="ap-register-chrome"
          style={{ fontSize: TYPE.scale.lg, lineHeight: TYPE.line.display, fontWeight: 600 }}
        >
          Lens
        </h1>
        <div className="relative ml-auto">
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
              <EgoGraph lens={lens} onGroupClick={scrollToSection} />
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
