"use client";

import { TYPE } from "@/lib/tokens";

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

const CONTRACT = [
  "ledger.v1.1 expected",
  "read-only report surface",
  "producer not connected in this UI lane",
  "no live rows available yet",
];

const BOUNDARIES = [
  "Enterprise Brain governs what the model may know and do.",
  "Bursar governs what the model may spend.",
  "Employee and workflow surfaces do not expose this room.",
  "Read grants do not create Bursar authority.",
];

function bursarPanelStyle(): React.CSSProperties {
  return {
    backdropFilter: "blur(18px)",
    background: "color-mix(in srgb, var(--paper) 88%, transparent)",
    boxShadow: "inset 0 1px 0 color-mix(in srgb, var(--ink) 8%, transparent)",
  };
}

function StatusChip({ children }: { children: React.ReactNode }) {
  return (
    <span
      className="ap-register-evidence ap-soft ap-hairline rounded border px-2 py-1"
      style={{ fontSize: TYPE.scale.xs }}
    >
      {children}
    </span>
  );
}

function DoctrineCard({ detail, label }: { detail: string; label: string }) {
  return (
    <article className="ap-card rounded border p-3" data-testid="bursar-doctrine-card" style={bursarPanelStyle()}>
      <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
        {label}
      </p>
      <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
        {detail}
      </p>
    </article>
  );
}

export function BursarSurface() {
  return (
    <main className="min-w-0 flex-1" data-testid="bursar-surface">
      <header className="ap-card mb-4 overflow-hidden rounded border p-5" style={bursarPanelStyle()}>
        <div className="grid grid-cols-1 gap-5 lg:grid-cols-[1.35fr_0.65fr]">
          <div className="max-w-3xl">
            <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
              Governed spend axis
            </p>
            <h1
              className="ap-register-chrome mt-2"
              style={{ fontSize: TYPE.scale.xl, fontWeight: 600, lineHeight: TYPE.line.display }}
            >
              Bursar Ledger Room
            </h1>
            <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.md, lineHeight: TYPE.line.body }}>
              Governed spend for Enterprise Brain model actions.
            </p>
            <p className="ap-soft mt-3 max-w-2xl" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
              This route is a UI-only placeholder for the future Ledger room. It will sit beside
              answers when real ledger data is connected, showing the governed spend that was capped,
              authorized, audited, and reconciled.
            </p>
          </div>
          <div className="flex content-start items-start gap-2 lg:flex-col">
            <StatusChip>route /admin/bursar</StatusChip>
            <StatusChip>UI-only lane</StatusChip>
            <StatusChip>no ledger fixture</StatusChip>
          </div>
        </div>
      </header>

      <section className="grid grid-cols-1 gap-4 lg:grid-cols-[1fr_1fr]" aria-label="Bursar doctrine">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          {DOCTRINE.map((item) => (
            <DoctrineCard key={item.label} detail={item.detail} label={item.label} />
          ))}
        </div>

        <section className="ap-card rounded border p-4" data-testid="bursar-contract-panel" style={bursarPanelStyle()}>
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Ledger contract panel
          </p>
          <h2 className="ap-register-chrome mt-2" style={{ fontSize: TYPE.scale.lg, fontWeight: 600 }}>
            ledger.v1.1 is expected before rows render.
          </h2>
          <div className="mt-4 grid grid-cols-1 gap-2">
            {CONTRACT.map((line) => (
              <div key={line} className="ap-hairline flex items-center justify-between gap-3 border-t py-2">
                <span className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm }}>
                  {line}
                </span>
                <StatusChip>{line === "ledger.v1.1 expected" ? "expected" : "unavailable"}</StatusChip>
              </div>
            ))}
          </div>
        </section>
      </section>

      <section className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-[0.88fr_1.12fr]">
        <section className="ap-card rounded border p-4" data-testid="bursar-empty-state" style={bursarPanelStyle()}>
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Honest unavailable state
          </p>
          <h2 className="ap-register-chrome mt-2" style={{ fontSize: TYPE.scale.lg, fontWeight: 600 }}>
            No ledger fixture is connected in this workspace yet.
          </h2>
          <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
            When the producer is connected, this room will show authorized model actions, their
            compiled budget envelope, cap posture, reconciliation status, and append-only audit trail.
            Until then it renders no live rows, no charts, and no totals.
          </p>
          <div className="mt-4 flex flex-wrap gap-2">
            <StatusChip>no fake data</StatusChip>
            <StatusChip>no charts</StatusChip>
            <StatusChip>no totals</StatusChip>
          </div>
        </section>

        <section className="ap-card rounded border p-4" data-testid="bursar-future-beat" style={bursarPanelStyle()}>
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Future demo beat
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
                className="ap-hairline rounded border px-3 py-2"
                style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
              >
                {boundary}
              </p>
            ))}
          </div>
        </section>
      </section>
    </main>
  );
}
