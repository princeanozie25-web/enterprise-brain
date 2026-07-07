"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "@/lib/api";
import type {
  AccessGrantRecord,
  AnswerEnvelope,
  DocCard,
  GrantedContextSummary,
  RoleScopeSummary,
  ScopeStatement,
} from "@/lib/api";
import { DERIVED, TYPE } from "@/lib/tokens";
import { AdminPreviewGate } from "./AdminPreviewGate";
import { AnswerCard } from "./AnswerCard";
import { AtlasRoom } from "./AtlasRoom";
import { BursarSurface } from "./BursarSurface";
import { DocInspector } from "./DocInspector";
import { EmployeeDashboard } from "./EmployeeDashboard";
import { ExportButton } from "./ExportButton";
import { FirstQuestionChip } from "./FirstQuestionChip";
import { GraphRoom } from "./GraphRoom";
import { GuidedJourney } from "./GuidedJourney";
import { IdentityRail } from "./IdentityRail";
import { LaneRoom } from "./LaneRoom";
import { LensBar } from "./LensBar";
import { LensRoom } from "./LensRoom";
import { MotionPanel, MotionSection } from "./MotionPrimitives";
import { ProjectSurface } from "./ProjectSurface";
import { ResultsList } from "./ResultsList";
import { Skeleton } from "./Skeleton";
import { DemoIdentityNotice } from "./TrustPosture";
import { RoomBoundary } from "./RoomBoundary";
import { ThemeToggle } from "./ThemeToggle";
import { useModalDialogFocus } from "./A11yDialog";
import iris from "./LensBar.module.css";
import * as session from "@/lib/session";

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
  view?: "adminBursar" | "adminGraph" | "ask" | "lens" | "atlas" | "lane" | "project" | "me";
}) {
  const [principal, setPrincipal] = useState<string | null>(null);
  // FC-A1: the principal we currently hold a valid server session for. Data
  // loads gate on this so nothing fetches before the session bearer exists.
  const [sessionFor, setSessionFor] = useState<string | null>(null);
  const [scope, setScope] = useState<ScopeStatement | null>(null);
  // Showcase-1 Track A: the identity's derived role posture (GET /me/scope) —
  // the EXISTING signal that drives admin affordances (isExecutiveCandidate).
  // It gates NAV VISIBILITY only; the server remains the boundary and every
  // route stays enforced exactly as today. No new authorization logic.
  const [roleScope, setRoleScope] = useState<RoleScopeSummary | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [query, setQuery] = useState("");
  // Ask request modes. Both stay OFF: the toggles that set them are rendered
  // DISABLED (see BROAD_SEARCH_AVAILABLE / VERIFIED_ANSWERS_AVAILABLE) because
  // the engine 500s on hybrid (AR-1b regen dropped the vector index) and on
  // judge (no judge model running). With the toggles disabled, /ask only ever
  // runs the working lexical-only path. Re-enabling is a one-line flag flip
  // once the engine supports each mode.
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
  const [entryGrantId, setEntryGrantId] = useState<string | null>(null);
  const [grantContext, setGrantContext] = useState<AccessGrantRecord | null>(null);
  const [grantContextUnavailable, setGrantContextUnavailable] = useState(false);
  const [reduced, setReduced] = useState(true);
  /** AR-2: ?subject=… — the click-to-lens door from the Org Graph; opens the
   * subject's lens (cross-lens, audited) once on the /lens page. */
  const [entrySubject, setEntrySubject] = useState<string | null>(null);
  const [inspector, setInspector] = useState<{
    open: boolean;
    loading: boolean;
    card: DocCard | null;
  }>({ open: false, loading: false, card: null });
  const lastEntrySearch = useRef<string | null>(null);

  const switchPrincipal = useCallback((next: string) => {
    if (typeof window !== "undefined") {
      const path = window.location.pathname || "/";
      const search = `?as=${encodeURIComponent(next)}`;
      window.history.replaceState(null, "", `${path}${search}`);
    }
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
    setEntryGrantId(null);
    setGrantContext(null);
    setGrantContextUnavailable(false);
    setEntrySubject(null);
    setSettingsOpen(false);
  }, []);

  // K3 Track 2 — SESSION EXPIRY. When any seam call 401s, the session layer
  // fires this once: capture the return intent (current room + the staged,
  // never-submitted query), then route to the identity picker. No auto-retry,
  // no refresh loop — one expiry, one human action, one restore. The picker
  // (ProductHome) reads `?expired=1` + the stashed intent, announces via
  // aria-live, and restores the room + staged query on re-pick.
  useEffect(() => {
    return session.onSessionExpired(() => {
      if (typeof window === "undefined") return;
      const path = window.location.pathname || "/";
      const staged = query.trim();
      session.captureReturnIntent({
        path,
        query: staged.length > 0 ? staged : null,
        principal,
      });
      window.location.href = "/?expired=1";
    });
  }, [query, principal]);

  // ENTRY DOORS (AP-3/AP-4): /atlas?cap=… opens the capability sheet once
  // the atlas loads; /lens?diff=… opens the diff view against the room's
  // subject; ?as=… carries the lens across a room change — and ONLY through
  // entryDoorActor above, the one function a real deployment swaps out.
  // Read when the browser URL changes; client navigation can keep this shell
  // mounted, so the demo entry door must be idempotent rather than one-shot.
  useEffect(() => {
    const search = window.location.search;
    if (lastEntrySearch.current === search) return;
    lastEntrySearch.current = search;
    const as = entryDoorActor(search);
    if (as !== null) {
      setPrincipal(as);
    }
    const params = new URLSearchParams(search);
    const cap = (params.get("cap") ?? "").trim();
    setEntryCapability(cap.length > 0 ? cap : null);
    const grant = (params.get("grant") ?? "").trim();
    setEntryGrantId(grant.length > 0 ? grant : null);
    const diff = (params.get("diff") ?? "").trim();
    setEntryDiff(diff.length > 0 ? diff : null);
    const subject = (params.get("subject") ?? "").trim();
    setEntrySubject(subject.length > 0 ? subject : null);
    // First-run door (A2): a suggested question arrives staged, never
    // auto-submitted — the person presses Ask themselves.
    const staged = (params.get("q") ?? "").trim();
    if (staged.length > 0) {
      setQuery(staged);
    }
  });

  useEffect(() => {
    setReduced(prefersReducedMotion());
  }, []);

  useEffect(() => {
    if (
      view !== "ask" ||
      principal === null ||
      sessionFor !== principal ||
      entryGrantId === null ||
      entryCapability === null
    ) {
      setGrantContext(null);
      setGrantContextUnavailable(false);
      return;
    }
    let cancelled = false;
    setGrantContext(null);
    setGrantContextUnavailable(false);
    api
      .getAccessGrant(principal, entryGrantId)
      .then((response) => {
        if (cancelled) return;
        const grant = response?.grant ?? null;
        if (
          grant &&
          grant.status === "active" &&
          grant.permission === "read" &&
          grant.grantee_id === principal &&
          grant.target.capability_id === entryCapability
        ) {
          setGrantContext(grant);
          setGrantContextUnavailable(false);
        } else {
          setGrantContext(null);
          setGrantContextUnavailable(true);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setGrantContext(null);
          setGrantContextUnavailable(true);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [entryCapability, entryGrantId, principal, sessionFor, view]);

  // FC-A1: when the chosen Work Identity changes, mint a server session for it
  // BEFORE any room fetches. sessionFor flips to `principal` only once login
  // resolves, gating the data-loading effects + rooms below (no 401 mid-login).
  useEffect(() => {
    if (principal === null) {
      setSessionFor(null);
      return;
    }
    let cancelled = false;
    setSessionFor(null);
    session
      .loginAs(principal)
      .then(() => {
        if (!cancelled) setSessionFor(principal);
      })
      .catch(() => {
        if (!cancelled) setSessionFor(null);
      });
    return () => {
      cancelled = true;
    };
  }, [principal]);

  useEffect(() => {
    if (principal === null || sessionFor !== principal) {
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
  }, [principal, sessionFor]);

  // Track A: read the identity's derived role posture to decide which NAV
  // doors are visible. Reads the same GET /me/scope the AdminPreviewGate and
  // dashboard already consume; 404 -> null (no posture) -> employee doors only.
  useEffect(() => {
    if (principal === null || sessionFor !== principal) {
      setRoleScope(null);
      return;
    }
    let cancelled = false;
    api
      .getRoleScope(principal)
      .then((response) => {
        if (!cancelled) setRoleScope(response);
      })
      .catch(() => {
        if (!cancelled) setRoleScope(null);
      });
    return () => {
      cancelled = true;
    };
  }, [principal, sessionFor]);

  const submitAsk = useCallback(async () => {
    const grantOptions =
      grantContext === null
        ? {}
        : {
            capabilityId: grantContext.target.capability_id,
            grantId: grantContext.grant_id,
          };
    if (
      principal === null ||
      query.trim().length === 0 ||
      asking ||
      (entryGrantId !== null && entryCapability !== null && grantContext === null)
    ) {
      return;
    }
    setAsking(true);
    setEnvelope(null);
    setSubmitted(null);
    try {
      const response = await api.ask(principal, query, { hybrid, judge, ...grantOptions });
      setEnvelope(response);
      setSubmitted({ query, hybrid, judge });
    } catch {
      // Internal errors render as the quiet no-answer state; the service
      // never explains absence and neither does the console.
      setEnvelope(null);
    } finally {
      setAsking(false);
    }
  }, [asking, entryCapability, entryGrantId, grantContext, hybrid, judge, principal, query]);

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

  const irisClass = reduced ? iris.fadeIn : iris.irisIn;
  const journeySurface =
    view === "me"
      ? "me"
      : view === "project"
        ? "project"
        : view === "ask"
          ? "ask"
          : view === "adminBursar"
            ? "bursar"
            : null;
  const showGuidedJourney = journeySurface !== null && view !== "me";
  // A4: exactly ONE demo-status line per page. The shell notice carries it
  // everywhere except the full-screen graph room, which has its own single
  // header banner (the shell notice would be hidden beneath it anyway).
  const showShellDemoIdentity = view !== "adminGraph";
  // FC-A1: the principal the rooms may fetch as — only once its server session
  // is minted. Until then it is null, so child rooms (whose effects run BEFORE
  // this shell's login effect) never fetch un-authenticated and 401. The Ask
  // shell still renders immediately against `principal`; its data effects gate
  // on sessionFor separately.
  const activePrincipal = sessionFor === principal ? principal : null;
  // Track A: the three-door law. Admin-class identities (the existing derived
  // signal — executive/super-admin candidate; the only per-identity admin
  // posture the model ever emits) additionally see the operations doors.
  // Everyone else sees exactly Home / Ask / Projects.
  const isAdminClass =
    roleScope?.derived_level === "executive_candidate" ||
    roleScope?.derived_level === "super_admin_candidate";
  // The admin doors (Operating Map, Spend Ledger, Company Map, Review Queue)
  // show for an admin-class identity from anywhere, and always on one of those
  // admin surfaces itself (you are already on it; the server gates the room via
  // AdminPreviewGate regardless of who deep-linked it).
  const showAdminDoors =
    isAdminClass ||
    view === "adminGraph" ||
    view === "adminBursar" ||
    view === "atlas" ||
    view === "lane";

  return (
    <div className="min-h-screen">
      <LensBar principal={principal} onSwitch={switchPrincipal} />

      {/* ONE VOCABULARY (A1): room labels come from the locked table, extended
          by the copy pass to cover all rooms: Projects (/project), Operating
          Map (/admin/graph), Spend Ledger (/admin/bursar). Internal product
          names never render. */}
      <nav className="ap-nav border-x-0 border-t-0" aria-label="Product surfaces" data-testid="view-switcher">
        <div className="mx-auto flex max-w-6xl flex-wrap items-center gap-2 px-4 py-1.5">
          {/* THE THREE-DOOR LAW (Track A): every identity sees Home / Ask /
              Projects. Nothing else in the base nav. */}
          <ViewDoor label="Home" href="/me" active={view === "me"} principal={principal} testId="view-door-me" />
          <ViewDoor label="Ask" href="/ask" active={view === "ask"} principal={principal} />
          <ViewDoor
            label="Projects"
            href="/project"
            active={view === "project"}
            principal={principal}
            testId="view-door-project"
          />

          {/* Admin-class identities additionally see the operations doors.
              Gated on the existing derived role signal — NAV VISIBILITY ONLY;
              the server enforces every route exactly as today, and a non-admin
              deep-linking an admin route gets today's AdminPreviewGate. */}
          {showAdminDoors && (
            <span className="flex flex-wrap items-center gap-2" data-testid="admin-doors">
              <ViewDoor
                label="Operating Map"
                href="/admin/graph"
                active={view === "adminGraph"}
                principal={principal}
                testId="view-door-admin-graph"
              />
              <ViewDoor
                label="Spend Ledger"
                href="/admin/bursar"
                active={view === "adminBursar"}
                principal={principal}
                testId="view-door-bursar"
              />
              <ViewDoor label="Company Map" href="/atlas" active={view === "atlas"} principal={principal} testId="view-door-atlas" />
              <ViewDoor label="Review Queue" href="/lane" active={view === "lane"} principal={principal} testId="view-door-lane" />
            </span>
          )}

          {/* Settings: My Access + Appearance + Identity live behind the gear. */}
          <button
            type="button"
            onClick={() => setSettingsOpen(true)}
            className="ap-washable ml-auto inline-flex min-h-8 items-center gap-1.5 rounded-lg px-2 py-0.5"
            style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
            aria-label="Settings"
            aria-haspopup="dialog"
            data-testid="settings-open"
          >
            <svg width={14} height={14} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} aria-hidden="true">
              <circle cx={12} cy={12} r={3} />
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
            Settings
          </button>
        </div>
      </nav>

      {showGuidedJourney && (
        <div className="mx-auto max-w-6xl px-4 pt-4">
          <GuidedJourney adminLinks={isAdminClass || view === "adminBursar"} current={journeySurface} principal={principal} />
        </div>
      )}

      {showShellDemoIdentity && (
        <div className="mx-auto max-w-6xl px-4 pt-4">
          <DemoIdentityNotice
            compact
            context={view === "adminBursar" ? "bursar" : "standard"}
            testId="shell-demo-identity-mode"
          />
        </div>
      )}

      <div
        key={
          (view === "adminGraph" || view === "adminBursar" ? principal : activePrincipal) ??
          "no-work-identity"
        }
        id="main"
        tabIndex={-1}
        className={`mx-auto flex ${view === "me" ? "max-w-7xl gap-3 px-4 py-3" : "max-w-6xl gap-6 p-4"} flex-col md:flex-row ${irisClass}`}
        data-testid="iris-stage"
      >
        {/* K3 Track 3: the room body is boundary-wrapped. In its normal
            (non-failed) state RoomBoundary is a pass-through fragment — zero
            DOM/landmark/heading delta, so axe/heading/GP stay pinned. The
            shell above #main (LensBar, guided journey, demo notice) is never
            inside the boundary, so a room crash never loses it. */}
        <RoomBoundary>
        {view === "adminGraph" ? (
          <AdminPreviewGate actor={principal} surface="admin">
            <main className="min-w-0 flex-1">
              <GraphRoom
                actor={activePrincipal}
                authPending={principal !== null && activePrincipal === null}
                reducedMotion={reduced}
                adminPreview
              />
            </main>
          </AdminPreviewGate>
        ) : view === "adminBursar" ? (
          <AdminPreviewGate actor={principal} surface="bursar">
            <BursarSurface />
          </AdminPreviewGate>
        ) : view === "me" ? (
          <div className="flex min-w-0 flex-1 flex-col gap-2">
            <FirstQuestionChip principal={activePrincipal} />
            <EmployeeDashboard actor={activePrincipal} />
          </div>
        ) : view === "lens" ? (
          <main className="min-w-0 flex-1">
            <LensRoom actor={activePrincipal} entryDiff={entryDiff} entrySubject={entrySubject} />
          </main>
        ) : view === "atlas" ? (
          <main className="min-w-0 flex-1">
            <AtlasRoom actor={activePrincipal} entryCapability={entryCapability} />
          </main>
        ) : view === "lane" ? (
          <main className="min-w-0 flex-1">
            <LaneRoom actor={activePrincipal} />
          </main>
        ) : view === "project" ? (
          <ProjectSurface actor={activePrincipal} capabilityId={entryCapability} />
        ) : (
          <>
            <aside className="w-full md:w-72 md:shrink-0">
              <IdentityRail principal={principal} scope={scope} />
            </aside>

            <main className="min-w-0 flex-1">
          <MotionPanel className="mb-3">
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
              Ask a question. Every answer shows its sources.
            </p>
          </MotionPanel>

          <MotionPanel className="ap-hero rounded-2xl p-4" delayIndex={1}>
            {(entryGrantId !== null || grantContext !== null || grantContextUnavailable) && (
              <GrantedAskContextPanel
                capabilityId={entryCapability}
                grant={grantContext}
                grantId={entryGrantId}
                serverContext={envelope?.granted_context}
                unavailable={grantContextUnavailable}
              />
            )}
            <textarea
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              aria-label="Ask a question"
              placeholder={
                principal === null
                  ? "Pick who you are first — answers depend on what you're allowed to see."
                  : grantContext
                    ? "Ask within this granted capability context..."
                    : "Ask within your scope..."
              }
              disabled={principal === null}
              rows={2}
              className="w-full resize-none rounded-lg px-3 py-2"
              style={{ fontSize: TYPE.scale.sm }}
              data-testid="query-input"
            />
            <div className="mt-3 flex flex-wrap items-start gap-x-6 gap-y-3">
              <AskToggle
                checked={hybrid}
                onChange={setHybrid}
                label="Broad search"
                helper="Finds documents by meaning, not only exact keywords."
                testId="toggle-hybrid"
                available={BROAD_SEARCH_AVAILABLE}
                reason={BROAD_SEARCH_UNAVAILABLE_REASON}
              />
              <AskToggle
                checked={judge}
                onChange={setJudge}
                label="Verified answers"
                helper="Only shows answers it can check against your documents; unverifiable claims are left out."
                testId="toggle-judge"
                available={VERIFIED_ANSWERS_AVAILABLE}
                reason={VERIFIED_ANSWERS_UNAVAILABLE_REASON}
              />
              <button
                type="button"
                onClick={submitAsk}
                disabled={
                  principal === null ||
                  asking ||
                  (entryGrantId !== null && entryCapability !== null && grantContext === null)
                }
                className="ap-affordance-button ml-auto min-h-10 self-center rounded-lg px-3 py-2"
                style={{ fontSize: TYPE.scale.xs, fontWeight: 500 }}
                data-testid="ask-button"
              >
                Ask
              </button>
            </div>
          </MotionPanel>

          {/* A3: async outcomes are announced — answer arrival and the quiet
              refusal both reach assistive tech without stealing focus. K1:
              the three generated-path states name anchoring (never semantic
              verification — R-B) and the all-refused grounding state is its
              own honest announcement. */}
          <p className="sr-only" role="status" aria-live="polite" data-testid="ask-live-status">
            {asking
              ? "Looking for an answer within your access."
              : envelope
                ? envelope.answer
                  ? `Answer ready: ${(envelope.answer.claims ?? envelope.answer.citations).length} claim${(envelope.answer.claims ?? envelope.answer.citations).length === 1 ? "" : "s"}, each anchored to a source you can open. ${envelope.results.length} source document${envelope.results.length === 1 ? "" : "s"}.`
                  : envelope.results.length > 0
                    ? envelope.grounding_applied
                      ? `Found ${envelope.results.length} source document${envelope.results.length === 1 ? "" : "s"}; no claim survived grounding — nothing unverifiable was invented.`
                      : `Found ${envelope.results.length} source document${envelope.results.length === 1 ? "" : "s"} within your access; no written answer was generated for this ask.`
                    : "Nothing within your access supports an answer, and nothing was invented."
                : ""}
          </p>

          <div className="mt-4 space-y-4">
            {asking && (
              <MotionPanel className="ap-card rounded-2xl p-4">
                <Skeleton lines={3} />
              </MotionPanel>
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
                {/* K1 drop-with-disclosure: removed draft claims get ONE
                    quiet line — calm honesty, never error-styled. */}
                {envelope.grounding && envelope.grounding.refused > 0 && (
                  <p
                    className="ap-register-chrome ap-soft px-1 italic"
                    style={{ fontSize: TYPE.scale.xs }}
                    data-testid="grounding-removed-line"
                  >
                    {envelope.grounding.refused === 1
                      ? "1 draft claim was removed: not verbatim-supported by your sources."
                      : `${envelope.grounding.refused} draft claims were removed: not verbatim-supported by your sources.`}
                  </p>
                )}
                <MotionSection className="ap-card rounded-2xl p-3" delayIndex={1}>
                  <h2
                    className="ap-soft px-2 pb-1 pt-1 uppercase tracking-wide"
                    style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
                  >
                    Results
                  </h2>
                  <ResultsList results={envelope.results} onOpenDoc={openDoc} />
                </MotionSection>
              </>
            )}
          </div>
            </main>
          </>
        )}
        </RoomBoundary>
      </div>

      {/* Track A settings drawer is mounted AFTER the #main surface so the
          surface's own <h1> always precedes the drawer's <h2>/<h3> in reading
          order — the heading-discipline law holds when the drawer is open.
          It is position:fixed, so DOM order never affects its visual place. */}
      <SettingsDrawer
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        principal={principal}
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

/**
 * Ask control availability. A toggle is only offered if its engine path can
 * actually run; flipping it otherwise 500s (a no-affordance / fail-closed
 * violation). Flip a flag back to true once the engine supports that mode:
 *   Broad search     -> needs the semantic/vector index rebuilt (dropped in the
 *                       AR-1b corpus regen)
 *   Verified answers -> needs a judge / verification model running
 * While a flag is false the toggle renders disabled with an honest reason and
 * only lexical-only Ask (both off) runs.
 */
const BROAD_SEARCH_AVAILABLE = false;
const VERIFIED_ANSWERS_AVAILABLE = false;
// A3: plain reasons. The second line exists because "Verified answers: OFF"
// was read as "hallucinations: ON" — every answer always carries its sources,
// with or without the verification pass.
const BROAD_SEARCH_UNAVAILABLE_REASON = "Not available in this build.";
const VERIFIED_ANSWERS_UNAVAILABLE_REASON =
  "Not available in this build — every answer always shows its sources either way.";

/**
 * A labelled toggle switch for the Ask controls. Keyboard-accessible
 * (role="switch", native button Enter/Space), visible focus via the global
 * :focus-visible ring, motion honours prefers-reduced-motion. When `available`
 * is false the switch is disabled (removed from the focus order, not flippable,
 * cursor-not-allowed, greyed) and shows an always-visible `reason`; the
 * plain-language helper stays. Colours come only from tokens (U-6).
 */
function AskToggle({
  checked,
  onChange,
  label,
  helper,
  testId,
  available = true,
  reason,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  helper: string;
  testId: string;
  available?: boolean;
  reason?: string;
}) {
  const disabled = !available;
  return (
    <div
      className="flex items-start gap-2.5"
      data-testid={`${testId}-control`}
      data-available={available ? "true" : "false"}
    >
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        aria-disabled={disabled || undefined}
        disabled={disabled}
        onClick={() => {
          if (!disabled) onChange(!checked);
        }}
        data-testid={testId}
        className={`inline-flex min-h-11 shrink-0 items-center rounded-full ${
          disabled ? "cursor-not-allowed opacity-50" : "ap-washable"
        }`}
      >
        <span
          className="flex h-6 w-11 items-center rounded-full p-0.5"
          style={{ background: checked ? "var(--affordance)" : "var(--surface-3)" }}
        >
          <span
            className="block h-5 w-5 rounded-full border motion-safe:transition-transform"
            style={{
              background: "var(--paper)",
              borderColor: "var(--hairline-strong)",
              transform: checked ? "translateX(20px)" : "translateX(0)",
            }}
          />
        </span>
      </button>
      <div className="min-w-0">
        <span className="ap-register-chrome block" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
          {label}
        </span>
        <span className="ap-soft block" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
          {helper}
        </span>
        {disabled && reason ? (
          <span
            className="ap-register-evidence ap-soft mt-1 block"
            style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
            data-testid={`${testId}-reason`}
          >
            {reason}
          </span>
        ) : null}
      </div>
    </div>
  );
}

function GrantedAskContextPanel({
  capabilityId,
  grant,
  grantId,
  serverContext,
  unavailable,
}: {
  capabilityId: string | null;
  grant: AccessGrantRecord | null;
  grantId: string | null;
  serverContext?: GrantedContextSummary;
  unavailable: boolean;
}) {
  const status = unavailable ? "unavailable" : grant?.status ?? "validating";
  const title = serverContext?.capability.name ?? capabilityId ?? "Granted capability";
  return (
    <MotionSection
      className="ap-card mb-3 rounded-2xl p-3"
      data-testid="ask-granted-context"
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Granted Knowledge
          </p>
          <h2 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            {title}
          </h2>
        </div>
        <span
          className="ap-chip ap-register-chrome rounded-lg px-2 py-1"
          style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
        >
          {status}
        </span>
      </div>
      <div className="mt-2 flex flex-wrap gap-1.5">
        {grantId && <GrantChip>grant {grantId}</GrantChip>}
        {capabilityId && <GrantChip>capability {capabilityId}</GrantChip>}
        {serverContext?.request_id && <GrantChip>request {serverContext.request_id}</GrantChip>}
        {serverContext?.approver_id && <GrantChip>approver {serverContext.approver_id}</GrantChip>}
      </div>
      <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
        {unavailable
          ? "This grant cannot be used for Ask context. Revoked, expired, mismatched, and unrelated grants are refused by the server."
          : grant
            ? "Ask will send this grant and capability to the server for validation. Results are constrained to the granted capability context."
            : "Checking the grant ledger before Ask can use this context."}
      </p>
    </MotionSection>
  );
}

function GrantChip({ children }: { children: React.ReactNode }) {
  return (
    <span
      className="ap-chip ap-register-evidence rounded-lg px-1.5 py-0.5"
      style={{ fontSize: TYPE.scale.xs }}
    >
      {children}
    </span>
  );
}

/**
 * One door in the shell's view switcher. The active door is a quiet fact,
 * not a link; the others carry the current Work Identity through `?as=`.
 */
function ViewDoor({
  label,
  href,
  active,
  principal,
  testId,
}: {
  label: string;
  href: string;
  active: boolean;
  principal: string | null;
  testId?: string;
}) {
  const testid = testId ?? `view-door-${label.toLowerCase().replace(/\s+/g, "-")}`;
  if (active) {
    return (
      <span
        className="ap-register-chrome rounded-lg px-2 py-0.5"
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
      className="ap-washable ap-register-chrome rounded-lg px-2 py-0.5"
      style={{ fontSize: TYPE.scale.xs, fontWeight: 500 }}
      data-testid={testid}
    >
      {label}
    </a>
  );
}

/**
 * Track A settings drawer: My Access, Appearance (theme), and Identity live
 * here behind the shell's gear — off the three-door nav. Uses the shared
 * A11yDialog primitive (focus trap, Escape, focus-return); the demo banner and
 * scope masthead stay structural in the shell behind it.
 */
function SettingsDrawer({
  open,
  onClose,
  principal,
}: {
  open: boolean;
  onClose: () => void;
  principal: string | null;
}) {
  const { dialogRef, onKeyDown } = useModalDialogFocus({ open, onClose });
  if (!open) return null;
  const carry = principal === null ? "" : `?as=${encodeURIComponent(principal)}`;
  return (
    <div
      className="fixed inset-0 z-50 flex justify-end"
      data-testid="settings-drawer-overlay"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      {/* Overlay scrim — the ONE place glass is allowed (overlay-only law). */}
      <div className="ap-glass-scrim absolute inset-0" aria-hidden="true" />
      <aside
        ref={dialogRef as React.RefObject<HTMLElement>}
        onKeyDown={onKeyDown}
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        tabIndex={-1}
        className="ap-elevated relative flex h-full w-full max-w-sm flex-col gap-4 overflow-y-auto p-5"
        data-testid="settings-drawer"
      >
        <div className="flex items-center justify-between">
          <h2 className="ap-register-chrome" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
            Settings
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="ap-washable rounded-lg px-2 py-1"
            style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
            aria-label="Close settings"
            data-testid="settings-close"
          >
            Close
          </button>
        </div>

        <section className="space-y-1.5" data-testid="settings-my-access">
          <h3 className="ap-soft uppercase tracking-wide" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
            My Access
          </h3>
          <p className="ap-soft" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            What you can see, and the reason you can see it.
          </p>
          <a
            href={`/lens${carry}`}
            className="ap-affordance-button ap-register-chrome inline-block rounded-lg px-3 py-2"
            style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
            data-testid="settings-my-access-link"
          >
            Open My Access
          </a>
        </section>

        <section className="space-y-1.5" data-testid="settings-appearance">
          <h3 className="ap-soft uppercase tracking-wide" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
            Appearance
          </h3>
          <div className="flex items-center gap-2">
            <ThemeToggle />
          </div>
        </section>

        <section className="space-y-1.5" data-testid="settings-identity">
          <h3 className="ap-soft uppercase tracking-wide" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
            Identity
          </h3>
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            {principal === null ? "No Work Identity selected" : `Acting as ${principal}`}
          </p>
          <a
            href="/"
            className="ap-washable ap-register-chrome inline-block rounded-lg border px-3 py-2"
            style={{ borderColor: "var(--hairline)", fontSize: TYPE.scale.xs, fontWeight: 600 }}
            data-testid="settings-switch-identity"
          >
            Switch on the front door
          </a>
        </section>
      </aside>
    </div>
  );
}
