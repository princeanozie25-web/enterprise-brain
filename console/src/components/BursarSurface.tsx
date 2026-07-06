"use client";

import { useCallback, useEffect, useState } from "react";

import { TYPE } from "@/lib/tokens";
import { getLedgerSummary } from "@/lib/ledger";
import type { DenialReason, LedgerResult, LedgerSummary } from "@/lib/ledger";
import { MotionArticle, MotionSection } from "./MotionPrimitives";

const DOCTRINE = [
  {
    detail: "A model action must have spend authority before work begins.",
    label: "Authorization before spend",
  },
  {
    detail: "No compiled budget envelope means no model call.",
    label: "Fail closed by default",
  },
  {
    detail: "Attempts are recorded before any permitted action proceeds.",
    label: "Audit before effect",
  },
  {
    detail: "Ledger data will reconcile authorization, model, effort, token, and USD caps.",
    label: "Reconcile every call",
  },
];

const BOUNDARIES = [
  "Enterprise Brain governs what the model may know and do.",
  "The Spend Ledger governs what the model may spend.",
  "Employee and workflow surfaces do not expose this room.",
  "Read grants do not create spend authority.",
];

/** Every canonical reason code, humanized. Typed as a total record so a new
 * producer reason fails the build here instead of rendering raw. */
const DENIAL_LABELS: Record<DenialReason, string> = {
  over_budget: "Over budget",
  model_not_allowed: "Model not allowed",
  effort_exceeded: "Effort exceeded",
  tokens_exceeded: "Tokens exceeded",
  expired: "Envelope expired",
  tampered: "Envelope tampered",
  no_envelope: "No envelope",
  clock_skew: "Clock skew",
  stale_snapshot: "Stale snapshot",
  envelope_reused: "Envelope reused",
  task_class_faulted: "Task class faulted",
  store_unavailable: "Store unavailable",
  unmapped: "Unmapped reason",
};

/** MONEY LAW: only a number renders as money. null renders as an em dash —
 * never "$0.00" — with the reason discoverable in the not-priced section.
 * (Callers must not invoke this at all when pricing is unverified.) */
function money(usd: number | null): string {
  return usd === null ? "—" : `$${usd.toFixed(2)}`;
}

function bursarPanelStyle(): React.CSSProperties {
  return {
    background: "var(--glass-fill)",
    boxShadow: "var(--shadow-2), inset 0 1px 0 var(--glass-highlight)",
  };
}

function StatusChip({ children }: { children: React.ReactNode }) {
  return (
    <span
      className="ap-chip ap-register-evidence rounded-lg px-2 py-1"
      style={{ fontSize: TYPE.scale.xs }}
    >
      {children}
    </span>
  );
}

function DoctrineCard({ delayIndex, detail, label }: { delayIndex: number; detail: string; label: string }) {
  return (
    <MotionArticle
      className="ap-card rounded-2xl p-4"
      data-testid="bursar-doctrine-card"
      delayIndex={delayIndex}
      style={bursarPanelStyle()}
    >
      <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
        {label}
      </p>
      <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
        {detail}
      </p>
    </MotionArticle>
  );
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
      {children}
    </p>
  );
}

function StatRow({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="ap-hairline flex items-center justify-between gap-3 border-t py-2">
      <span className="ap-soft" style={{ fontSize: TYPE.scale.sm }}>
        {label}
      </span>
      <span className="ap-register-evidence" style={{ fontSize: TYPE.scale.sm }}>
        {value}
      </span>
    </div>
  );
}

/** STATE 1 / STATE 2: the live report. Honest zeros are data — a reachable
 * producer with zero calls renders this frame with zeros, never the
 * unavailable state. */
function LiveReport({ data }: { data: LedgerSummary }) {
  const priced = data.pricing_verified;
  const emptyWindow = data.window.calls === 0;
  return (
    <MotionSection
      className="ap-card rounded-2xl p-4"
      data-testid="bursar-live-report"
      delayIndex={4}
      style={bursarPanelStyle()}
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <SectionLabel>Live report</SectionLabel>
        <div className="flex flex-wrap gap-2">
          <span data-testid="bursar-mode-chip">
            <StatusChip>{data.mode}</StatusChip>
          </span>
          <span data-testid="bursar-scope-chip">
            <StatusChip>
              {data.governance.scope === "all_time"
                ? "all-time counters"
                : "window counters"}
            </StatusChip>
          </span>
        </div>
      </div>

      <p
        className="ap-register-chrome mt-2"
        style={{ fontSize: TYPE.scale.md, fontWeight: 600 }}
        data-testid="bursar-window-line"
      >
        {data.window.label} window · {data.window.calls} call{data.window.calls === 1 ? "" : "s"} ·
        ordinals {data.window.first_ordinal} → {data.window.last_ordinal}
      </p>
      {emptyWindow && (
        <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
          Zero calls in this window. Honest zeros are data, not an error.
        </p>
      )}

      <div className="mt-4" data-testid="bursar-baseline">
        <SectionLabel>Baseline spend</SectionLabel>
        {priced ? (
          <StatRow label="Total" value={<span data-testid="bursar-total-usd">{money(data.baseline.total_usd)}</span>} />
        ) : (
          <p
            className="ap-soft mt-2"
            style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}
            data-testid="bursar-pricing-withheld"
          >
            Pricing unverified — figures withheld.
          </p>
        )}
        {data.baseline.by_model.map((row) => (
          <div
            key={row.model}
            className="ap-hairline flex flex-wrap items-center justify-between gap-3 border-t py-2"
            data-testid="bursar-model-row"
          >
            <span className="ap-register-evidence" style={{ fontSize: TYPE.scale.sm }}>
              {row.model}
            </span>
            <span className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
              {row.calls} calls · {row.input_tokens} in · {row.output_tokens} out ·{" "}
              {row.cache_read_tokens} cache read · {row.cache_write_tokens} cache write
              {priced && <> · {money(row.usd)}</>}
            </span>
          </div>
        ))}
        {data.baseline.skipped_unpriced.length > 0 && (
          <div className="mt-3" data-testid="bursar-skipped">
            <SectionLabel>Not priced (listed, never guessed)</SectionLabel>
            {data.baseline.skipped_unpriced.map((row) => (
              <div
                key={row.model}
                className="ap-hairline flex items-center justify-between gap-3 border-t py-2"
              >
                <span className="ap-register-evidence" style={{ fontSize: TYPE.scale.sm }}>
                  {row.model}
                </span>
                <span className="ap-soft ap-register-evidence" style={{ fontSize: TYPE.scale.xs }}>
                  {row.calls} calls · {row.reason}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="mt-4" data-testid="bursar-governance">
        <SectionLabel>Governance</SectionLabel>
        <StatRow label="Envelopes issued" value={data.governance.envelopes_issued} />
        <StatRow label="Calls authorized" value={data.governance.calls_authorized} />
        <StatRow label="Calls denied" value={data.governance.calls_denied} />
        <StatRow label="Effort ceilings applied" value={data.governance.effort_ceiling_applied} />
        {data.governance.denials_by_reason.map((entry) => (
          <div
            key={entry.reason}
            className="ap-hairline flex items-center justify-between gap-3 border-t py-2"
            data-testid="bursar-denial-row"
          >
            <span className="ap-soft" style={{ fontSize: TYPE.scale.sm }}>
              {DENIAL_LABELS[entry.reason]}{" "}
              <span className="ap-register-evidence" style={{ fontSize: TYPE.scale.xs }}>
                {entry.reason}
              </span>
            </span>
            <span className="ap-register-evidence" style={{ fontSize: TYPE.scale.sm }}>
              {entry.count}
            </span>
          </div>
        ))}
        {data.governance.drift_flags.length > 0 && (
          <div className="mt-3" data-testid="bursar-drift">
            <SectionLabel>Drift flags (display-only)</SectionLabel>
            {data.governance.drift_flags.map((flag) => (
              <div
                key={flag.task_class}
                className="ap-hairline flex items-center justify-between gap-3 border-t py-2"
              >
                <span className="ap-register-evidence" style={{ fontSize: TYPE.scale.sm }}>
                  {flag.task_class}
                </span>
                <span className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
                  {flag.mean_variance_pct}% mean variance · {flag.calls} calls
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="mt-4" data-testid="bursar-delta">
        <SectionLabel>Delta</SectionLabel>
        {data.delta.available ? (
          <div data-testid="bursar-delta-block">
            <StatRow label="Baseline" value={priced ? money(data.delta.baseline_usd) : "—"} />
            <StatRow label="Governed" value={priced ? money(data.delta.governed_usd) : "—"} />
            <StatRow
              label="Savings"
              value={data.delta.savings_pct === null ? "—" : `${data.delta.savings_pct}%`}
            />
          </div>
        ) : (
          <p
            className="ap-soft mt-2"
            style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
            data-testid="bursar-delta-note"
          >
            {data.delta.note}
          </p>
        )}
      </div>
    </MotionSection>
  );
}

/** STATE 3: the honest unavailable state — a finding, not a bug. No cached
 * or stale numbers; one calm card, the producer's detail, one retry. */
function UnavailableState({ detail, onRetry }: { detail: string; onRetry: () => void }) {
  return (
    <MotionSection
      className="ap-card rounded-2xl p-4"
      data-testid="bursar-unavailable"
      delayIndex={4}
      style={bursarPanelStyle()}
    >
      <SectionLabel>Honest unavailable state</SectionLabel>
      <h2 className="ap-register-chrome mt-2" style={{ fontSize: TYPE.scale.lg, fontWeight: 600 }}>
        Ledger unavailable — the spend producer is not running.
      </h2>
      <p
        className="ap-soft ap-register-evidence mt-3"
        style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
        data-testid="bursar-unavailable-detail"
      >
        {detail}
      </p>
      <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
        Nothing is rendered from cache and nothing is estimated. When the producer answers again,
        this room shows its live report.
      </p>
      <div className="mt-4 flex flex-wrap items-center gap-2">
        <button
          type="button"
          className="ap-affordance-button ap-register-chrome min-h-11 rounded-full px-5 py-2.5"
          style={{ fontSize: TYPE.scale.sm, fontWeight: 700 }}
          onClick={onRetry}
          data-testid="bursar-retry"
        >
          Retry
        </button>
        <StatusChip>no fake data</StatusChip>
        <StatusChip>no charts</StatusChip>
        <StatusChip>no totals</StatusChip>
      </div>
    </MotionSection>
  );
}

export function BursarSurface() {
  const [result, setResult] = useState<LedgerResult | null>(null);

  const load = useCallback(() => {
    setResult(null);
    void getLedgerSummary().then(setResult);
  }, []);

  // One fetch on mount; one per explicit Retry. No polling, no auto-retry.
  useEffect(() => {
    load();
  }, [load]);

  const live = result?.state === "ok" ? result.data : null;

  return (
    <main className="min-w-0 flex-1" data-testid="bursar-surface">
      <MotionSection className="ap-hero mb-4 overflow-hidden rounded-2xl p-5 md:p-6">
        <div className="grid grid-cols-1 gap-5 lg:grid-cols-[1.35fr_0.65fr]">
          <div className="max-w-3xl">
            <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
              Governed spend axis
            </p>
            <h1
              className="ap-register-chrome mt-2"
              style={{ fontSize: TYPE.scale.xl, fontWeight: 600, lineHeight: TYPE.line.display }}
            >
              Spend Ledger
            </h1>
            <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.md, lineHeight: TYPE.line.body }}>
              What AI assistance costs, and who authorized it.
            </p>
            <p className="ap-soft mt-3 max-w-2xl" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
              This room reads the local spend producer over a loopback contract and renders exactly
              what it reports — capped, authorized, audited, reconciled. It is read-only: it renders
              spend; it never authorizes it.
            </p>
          </div>
          <div className="flex content-start items-start gap-2 lg:flex-col">
            {result === null && <StatusChip>contacting producer…</StatusChip>}
            {result?.state === "ok" && (
              <>
                <StatusChip>{result.data.mode}</StatusChip>
                <StatusChip>producer reachable</StatusChip>
              </>
            )}
            {result?.state === "unavailable" && <StatusChip>producer unreachable</StatusChip>}
            <StatusChip>read-only</StatusChip>
          </div>
        </div>
      </MotionSection>

      {/* A4: the page's single demo-status line is the shell's notice. */}
      <section className="grid grid-cols-1 gap-4 lg:grid-cols-[1fr_1fr]" aria-label="Spend Ledger doctrine">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          {DOCTRINE.map((item, index) => (
            <DoctrineCard key={item.label} delayIndex={index + 1} detail={item.detail} label={item.label} />
          ))}
        </div>

        <MotionSection
          className="ap-card rounded-2xl p-4"
          data-testid="bursar-contract-panel"
          delayIndex={3}
        >
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Ledger contract panel
          </p>
          <h2 className="ap-register-chrome mt-2" style={{ fontSize: TYPE.scale.lg, fontWeight: 600 }}>
            ledger.v1.1, reported live — never hardcoded.
          </h2>
          <div className="mt-4 grid grid-cols-1 gap-2">
            <div className="ap-hairline flex items-center justify-between gap-3 border-t py-2">
              <span className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm }}>
                schema ledger.v1.1
              </span>
              <StatusChip>
                {result === null ? "checking" : result.state === "ok" ? "ok" : "unverified"}
              </StatusChip>
            </div>
            <div className="ap-hairline flex items-center justify-between gap-3 border-t py-2">
              <span className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm }}>
                producer reachable
              </span>
              <StatusChip>
                {result === null ? "checking" : result.state === "ok" ? "yes" : "no"}
              </StatusChip>
            </div>
            <div className="ap-hairline flex items-center justify-between gap-3 border-t py-2">
              <span className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm }}>
                read-only report surface
              </span>
              <StatusChip>always</StatusChip>
            </div>
          </div>
        </MotionSection>
      </section>

      <section className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-[1.12fr_0.88fr]">
        {result === null && (
          <MotionSection
            className="ap-card rounded-2xl p-4"
            data-testid="bursar-loading"
            delayIndex={4}
            style={bursarPanelStyle()}
          >
            <SectionLabel>Live report</SectionLabel>
            <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
              Contacting the spend producer…
            </p>
          </MotionSection>
        )}
        {live !== null && <LiveReport data={live} />}
        {result?.state === "unavailable" && (
          <UnavailableState detail={result.detail} onRetry={load} />
        )}

        <MotionSection
          className="ap-card rounded-2xl p-4"
          data-testid="bursar-future-beat"
          delayIndex={5}
          style={bursarPanelStyle()}
        >
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            The demo beat
          </p>
          <blockquote
            className="ap-register-chrome mt-2"
            style={{ fontSize: TYPE.scale.lg, fontWeight: 600, lineHeight: TYPE.line.display }}
          >
            Same console: the answer, and the governed spend it cost - capped, authorized, audited.
          </blockquote>
          <div className="mt-4 grid grid-cols-1 gap-2 md:grid-cols-2">
            {BOUNDARIES.map((boundary) => (
              <p
                key={boundary}
                className="ap-chip rounded-lg px-3 py-2"
                style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
              >
                {boundary}
              </p>
            ))}
          </div>
        </MotionSection>
      </section>
    </main>
  );
}
