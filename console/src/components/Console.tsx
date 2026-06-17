"use client";

import { useCallback, useEffect, useState } from "react";
import * as api from "@/lib/api";
import type { AnswerEnvelope, DocCard, ScopeStatement } from "@/lib/api";
import { DERIVED, TYPE } from "@/lib/tokens";
import { AnswerCard } from "./AnswerCard";
import { AtlasRoom } from "./AtlasRoom";
import { DocInspector } from "./DocInspector";
import { EmployeeDashboard } from "./EmployeeDashboard";
import { ExportButton } from "./ExportButton";
import { GraphRoom } from "./GraphRoom";
import { IdentityRail } from "./IdentityRail";
import { LaneRoom } from "./LaneRoom";
import { LensBar } from "./LensBar";
import { LensRoom } from "./LensRoom";
import { ProjectSurface } from "./ProjectSurface";
import { ResultsList } from "./ResultsList";
import { Skeleton } from "./Skeleton";
import iris from "./LensBar.module.css";

/**
 * The Aperture shell: lens bar on top (the navigation primitive), the Ask
 * view beneath. Switching lenses fires the IRIS — a circular clip-path wipe
 * centered on the lens bar — during which the answer/results state clears
 * (the M3b residue rule, now with a face). prefers-reduced-motion swaps the
 * iris for fade-view.
 */
function prefersReducedMotion(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    // No way to ask: choose the calmer path.
    return true;
  }
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * THE ENTRY-DOOR SWAP POINT (carried requirement from the AP-3 review).
 * demo_identity_mode ONLY: a URL-borne actor (?as=…) is honored here so the
 * rooms — separate static pages — can hand a lens across a door. This is
 * the console twin of the service's authorize_cross_lens seam: in a real
 * deployment the actor derives from the SESSION (OIDC), a URL-borne actor
 * is REFUSED, and this function returns null unconditionally. Swap THIS
 * function and nothing else moves.
 */
function entryDoorActor(search: string): string | null {
  const as = (new URLSearchParams(search).get("as") ?? "").trim();
  return as.length > 0 ? as : null;
}

export function Console({
  view = "ask",
}: {
  view?: "adminGraph" | "ask" | "lens" | "atlas" | "lane" | "project" | "me";
}) {
  const [principal, setPrincipal] = useState<string | null>(null);
  const [scope, setScope] = useState<ScopeStatement | null>(null);
  const [query, setQuery] = useState("");
  const [hybrid, setHybrid] = useState(false);
  const [judge, setJudge] = useState(false);
  const [envelope, setEnvelope] = useState<AnswerEnvelope | null>(null);
  const [asking, setAsking] = useState(false);
  /** AP-5: the params that produced the displayed envelope — what an
   * export must name (the textarea may have moved on). */
  const [submitted, setSubmitted] = useState<{
    query: string;
    hybrid: boolean;
    judge: boolean;
  } | null>(null);
  const [entryCapability, setEntryCapability] = useState<string | null>(null);
  const [entryDiff, setEntryDiff] = useState<string | null>(null);
  /** AR-2: ?subject=… — the click-to-lens door from the Org Graph; opens the
   * subject's lens (cross-lens, audited) once on the /lens page. */
  const [entrySubject, setEntrySubject] = useState<string | null>(null);
  const [inspector, setInspector] = useState<{
    open: boolean;
    loading: boolean;
    card: DocCard | null;
  }>({ open: false, loading: false, card: null });

  const switchPrincipal = useCallback((next: string) => {
    setPrincipal(next);
    // Clear EVERYTHING the previous lens saw, before any fetch: the iris
    // reveals a clean world.
    setScope(null);
    setEnvelope(null);
    setSubmitted(null);
    setInspector({ open: false, loading: false, card: null });
    // The entry doors are spent: a lens switch never re-opens a sheet and
    // never re-enters a diff or a graph-borne subject (the residue rule).
    setEntryCapability(null);
    setEntryDiff(null);
    setEntrySubject(null);
  }, []);

  // ENTRY DOORS (AP-3/AP-4): /atlas?cap=… opens the capability sheet once
  // the atlas loads; /lens?diff=… opens the diff view against the room's
  // subject; ?as=… carries the lens across a room change — and ONLY through
  // entryDoorActor above, the one function a real deployment swaps out.
  // Read once on mount; absent in tests and direct visits.
  useEffect(() => {
    const search = window.location.search;
    const as = entryDoorActor(search);
    if (as !== null) {
      setPrincipal(as);
    }
    const params = new URLSearchParams(search);
    const cap = (params.get("cap") ?? "").trim();
    if (cap.length > 0) {
      setEntryCapability(cap);
    }
    const diff = (params.get("diff") ?? "").trim();
    if (diff.length > 0) {
      setEntryDiff(diff);
    }
    const subject = (params.get("subject") ?? "").trim();
    if (subject.length > 0) {
      setEntrySubject(subject);
    }
  }, []);

  useEffect(() => {
    if (principal === null) {
      return;
    }
    let cancelled = false;
    api
      .getScope(principal)
      .then((response) => {
        if (!cancelled) {
          setScope(response.scope_statement);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setScope(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [principal]);

  const submitAsk = useCallback(async () => {
    if (principal === null || query.trim().length === 0 || asking) {
      return;
    }
    setAsking(true);
    setEnvelope(null);
    setSubmitted(null);
    try {
      const response = await api.ask(principal, query, { hybrid, judge });
      setEnvelope(response);
      setSubmitted({ query, hybrid, judge });
    } catch {
      // Internal errors render as the quiet no-answer state; the service
      // never explains absence and neither does the console.
      setEnvelope(null);
    } finally {
      setAsking(false);
    }
  }, [principal, query, hybrid, judge, asking]);

  const openDoc = useCallback(
    async (docId: string) => {
      if (principal === null) {
        return;
      }
      setInspector({ open: true, loading: true, card: null });
      try {
        const card = await api.getDoc(principal, docId);
        setInspector({ open: true, loading: false, card });
      } catch {
        setInspector({ open: true, loading: false, card: null });
      }
    },
    [principal],
  );

  const reduced = prefersReducedMotion();
  const irisClass = reduced ? iris.fadeIn : iris.irisIn;

  return (
    <div className="min-h-screen">
      <LensBar principal={principal} onSwitch={switchPrincipal} />

      <nav className="ap-card border-x-0 border-t-0" aria-label="Rooms" data-testid="view-switcher">
        <div className="mx-auto flex max-w-6xl items-center gap-2 px-4 py-1.5">
          <ViewDoor label="Me" href="/me" active={view === "me"} principal={principal} />
          <ViewDoor label="Project" href="/project" active={view === "project"} principal={principal} />
          <ViewDoor label="Ask" href="/ask" active={view === "ask"} principal={principal} />
          <ViewDoor label="Admin Graph" href="/admin/graph" active={view === "adminGraph"} principal={principal} />
          <span
            className="ap-register-evidence ap-soft rounded px-1.5 py-0.5"
            style={{ fontSize: TYPE.scale.xs }}
            data-testid="admin-preview-badge"
          >
            derived access posture only / not full auth enforced yet
          </span>
          <ViewDoor label="Lens" href="/lens" active={view === "lens"} principal={principal} />
          <ViewDoor label="Atlas" href="/atlas" active={view === "atlas"} principal={principal} />
          <ViewDoor label="Lane" href="/lane" active={view === "lane"} principal={principal} />
          {/* THE RESERVED DOOR — flagged in the AP-3 closeout: "Ledger —
              reserved" is placeholder copy for a room that does not exist
              yet. Disabled, not hidden: the shell states its own shape. */}
          <span
            className="ap-soft ml-2 cursor-default"
            style={{ fontSize: TYPE.scale.xs }}
            aria-disabled="true"
            data-testid="ledger-door"
          >
            Ledger — reserved
          </span>
        </div>
      </nav>

      <div
        key={principal ?? "no-lens"}
        className={`mx-auto flex max-w-6xl gap-6 p-4 ${irisClass}`}
        data-testid="iris-stage"
      >
        {view === "adminGraph" ? (
          <main className="min-w-0 flex-1">
            <GraphRoom actor={principal} reducedMotion={reduced} adminPreview />
          </main>
        ) : view === "me" ? (
          <EmployeeDashboard actor={principal} />
        ) : view === "lens" ? (
          <main className="min-w-0 flex-1">
            <LensRoom actor={principal} entryDiff={entryDiff} entrySubject={entrySubject} />
          </main>
        ) : view === "atlas" ? (
          <main className="min-w-0 flex-1">
            <AtlasRoom actor={principal} entryCapability={entryCapability} />
          </main>
        ) : view === "lane" ? (
          <main className="min-w-0 flex-1">
            <LaneRoom actor={principal} />
          </main>
        ) : view === "project" ? (
          <ProjectSurface actor={principal} capabilityId={entryCapability} />
        ) : (
          <>
            <aside className="w-72 shrink-0">
              <IdentityRail principal={principal} scope={scope} />
            </aside>

            <main className="min-w-0 flex-1">
          <header className="mb-3">
            <h1
              className="ap-register-chrome"
              style={{
                fontSize: TYPE.scale.lg,
                lineHeight: TYPE.line.display,
                fontWeight: 600,
              }}
            >
              Ask
            </h1>
            <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
              Scope, provenance, and honest degradation, at a glance.
            </p>
          </header>

          <div className="ap-card rounded p-3">
            <textarea
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={
                principal === null ? "Select a lens first" : "Ask within your scope…"
              }
              disabled={principal === null}
              rows={2}
              className="w-full resize-none rounded px-2 py-1.5"
              style={{ fontSize: TYPE.scale.sm }}
              data-testid="query-input"
            />
            <div className="mt-2 flex items-center gap-4">
              <label
                className="ap-soft flex items-center gap-1.5"
                style={{ fontSize: TYPE.scale.xs }}
              >
                <input
                  type="checkbox"
                  checked={hybrid}
                  onChange={(e) => setHybrid(e.target.checked)}
                  data-testid="toggle-hybrid"
                />
                hybrid
              </label>
              <label
                className="ap-soft flex items-center gap-1.5"
                style={{ fontSize: TYPE.scale.xs }}
              >
                <input
                  type="checkbox"
                  checked={judge}
                  onChange={(e) => setJudge(e.target.checked)}
                  data-testid="toggle-judge"
                />
                judge
              </label>
              <button
                type="button"
                onClick={submitAsk}
                disabled={principal === null || asking}
                className="ap-affordance-button ml-auto rounded px-3 py-1"
                style={{ fontSize: TYPE.scale.xs, fontWeight: 500 }}
                data-testid="ask-button"
              >
                Ask
              </button>
            </div>
          </div>

          <div className="mt-4 space-y-4">
            {asking && (
              <div className="ap-card rounded p-4">
                <Skeleton lines={3} />
              </div>
            )}
            {envelope && (
              <>
                {/* AP-5: the answer card's export home — present only when
                    an envelope is, naming the params that produced it. */}
                <div className="flex justify-end">
                  <ExportButton
                    actor={principal}
                    request={submitted === null ? null : { view: "ask", ask: submitted }}
                    filename={api.exportFilename(
                      "ask",
                      envelope.query_hash.slice(0, 8),
                      envelope.snapshot_version,
                    )}
                    disabled={asking}
                  />
                </div>
                <AnswerCard envelope={envelope} onOpenDoc={openDoc} />
                <section className="ap-card rounded p-2">
                  <h2
                    className="ap-soft px-2 pb-1 pt-1 uppercase tracking-wide"
                    style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
                  >
                    Results
                  </h2>
                  <ResultsList results={envelope.results} onOpenDoc={openDoc} />
                </section>
              </>
            )}
          </div>
            </main>
          </>
        )}
      </div>

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

/**
 * One door in the shell's view switcher. The active door is a quiet fact,
 * not a link; the others carry the current lens through `?as=` so a room
 * change keeps the same eyes.
 */
function ViewDoor({
  label,
  href,
  active,
  principal,
}: {
  label: string;
  href: string;
  active: boolean;
  principal: string | null;
}) {
  const testid = `view-door-${label.toLowerCase().replace(/\s+/g, "-")}`;
  if (active) {
    return (
      <span
        className="ap-register-chrome rounded px-2 py-0.5"
        style={{ fontSize: TYPE.scale.xs, fontWeight: 600, backgroundColor: DERIVED.wash }}
        aria-current="page"
        data-testid={testid}
      >
        {label}
      </span>
    );
  }
  const carry = principal === null ? "" : `?as=${encodeURIComponent(principal)}`;
  return (
    <a
      href={`${href}${carry}`}
      className="ap-washable ap-register-chrome rounded px-2 py-0.5"
      style={{ fontSize: TYPE.scale.xs, fontWeight: 500 }}
      data-testid={testid}
    >
      {label}
    </a>
  );
}
