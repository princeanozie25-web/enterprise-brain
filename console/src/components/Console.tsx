"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "@/lib/api";
import type {
  AccessGrantRecord,
  AnswerEnvelope,
  DocCard,
  GrantedContextSummary,
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
  view?: "adminBursar" | "adminGraph" | "ask" | "lens" | "atlas" | "lane" | "project" | "me";
}) {
  const [principal, setPrincipal] = useState<string | null>(null);
  const [scope, setScope] = useState<ScopeStatement | null>(null);
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
  }, []);

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
  });

  useEffect(() => {
    setReduced(prefersReducedMotion());
  }, []);

  useEffect(() => {
    if (
      view !== "ask" ||
      principal === null ||
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
  }, [entryCapability, entryGrantId, principal, view]);

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
  const showShellDemoIdentity = view !== "adminGraph" && view !== "me";
  const showAdminPreviewBadge = view === "adminGraph" || view === "adminBursar";

  return (
    <div className="min-h-screen">
      <LensBar principal={principal} onSwitch={switchPrincipal} />

      <nav className="ap-glass-nav border-x-0 border-t-0" aria-label="Product surfaces" data-testid="view-switcher">
        <div className="mx-auto flex max-w-6xl flex-wrap items-center gap-2 px-4 py-1.5">
          <ViewDoor label="Work Identity" href="/me" active={view === "me"} principal={principal} testId="view-door-me" />
          <ViewDoor
            label="Workflow Command"
            href="/project"
            active={view === "project"}
            principal={principal}
            testId="view-door-project"
          />
          <ViewDoor label="Ask" href="/ask" active={view === "ask"} principal={principal} />
          <ViewDoor
            label="Operating Map"
            href="/admin/graph"
            active={view === "adminGraph"}
            principal={principal}
            testId="view-door-admin-graph"
          />
          {(view === "adminBursar" || view === "adminGraph") && (
            <ViewDoor
              label="Bursar Ledger Room"
              href="/admin/bursar"
              active={view === "adminBursar"}
              principal={principal}
              testId="view-door-bursar"
            />
          )}
          {showAdminPreviewBadge && (
            <span
              className="ap-register-evidence ap-soft rounded px-1.5 py-0.5"
              style={{ fontSize: TYPE.scale.xs }}
              data-testid="admin-preview-badge"
            >
              Demo Identity Mode: admin and spend rooms are previews; production authority binding is not connected
            </span>
          )}
          <ViewDoor label="Knowledge View" href="/lens" active={view === "lens"} principal={principal} testId="view-door-lens" />
          <ViewDoor label="Capability Map" href="/atlas" active={view === "atlas"} principal={principal} testId="view-door-atlas" />
          <ViewDoor label="Review Queue" href="/lane" active={view === "lane"} principal={principal} testId="view-door-lane" />
        </div>
      </nav>

      {showGuidedJourney && (
        <div className="mx-auto max-w-6xl px-4 pt-4">
          <GuidedJourney adminLinks={view === "adminBursar"} current={journeySurface} principal={principal} />
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
        key={principal ?? "no-work-identity"}
        className={`mx-auto flex ${view === "me" ? "max-w-7xl gap-3 px-4 py-3" : "max-w-6xl gap-6 p-4"} flex-col md:flex-row ${irisClass}`}
        data-testid="iris-stage"
      >
        {view === "adminGraph" ? (
          <AdminPreviewGate actor={principal} surface="admin">
            <main className="min-w-0 flex-1">
              <GraphRoom actor={principal} reducedMotion={reduced} adminPreview />
            </main>
          </AdminPreviewGate>
        ) : view === "adminBursar" ? (
          <AdminPreviewGate actor={principal} surface="bursar">
            <BursarSurface />
          </AdminPreviewGate>
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
              Permission-aware Ask with Work Identity scope, provenance, and fail-closed grant checks.
            </p>
          </MotionPanel>

          <MotionPanel className="ap-glass-elevated rounded-2xl p-4" delayIndex={1}>
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
              placeholder={
                principal === null
                  ? "Choose a Work Identity first"
                  : grantContext
                    ? "Ask within this granted capability context..."
                    : "Ask within your scope..."
              }
              disabled={principal === null}
              rows={2}
              className="w-full resize-none rounded px-3 py-2"
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
                className="ap-affordance-button ml-auto min-h-10 self-center rounded px-3 py-2"
                style={{ fontSize: TYPE.scale.xs, fontWeight: 500 }}
                data-testid="ask-button"
              >
                Ask
              </button>
            </div>
          </MotionPanel>

          <div className="mt-4 space-y-4">
            {asking && (
              <MotionPanel className="ap-glass-panel rounded-2xl p-4">
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
                <MotionSection className="ap-glass-panel rounded-2xl p-3" delayIndex={1}>
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
const BROAD_SEARCH_UNAVAILABLE_REASON = "Unavailable in this build — semantic index not loaded";
const VERIFIED_ANSWERS_UNAVAILABLE_REASON = "Unavailable in this build — verification model not loaded";

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
      className="ap-glass-panel mb-3 rounded-2xl p-3"
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
          className="ap-chip ap-register-chrome rounded px-2 py-1"
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
      className="ap-chip ap-register-evidence rounded px-1.5 py-0.5"
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
