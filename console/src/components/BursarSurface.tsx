"use client";

import { TYPE } from "@/lib/tokens";

const REQUIRED_SOURCES = [
  "supplier master",
  "invoice ledger",
  "purchase orders",
  "payment runs",
  "approval authority",
  "duplicate-spend rules",
];

const BOUNDARIES = [
  "No suppliers, invoices, spend, savings, or duplicate-payment facts are connected yet.",
  "Read grants for project context do not unlock this finance/admin surface.",
  "Current role scope is derived_only and does not create finance authority.",
];

function bursarPanelStyle(): React.CSSProperties {
  return {
    backdropFilter: "blur(18px)",
    background: "color-mix(in srgb, var(--paper) 86%, transparent)",
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

function ReadinessCard({
  detail,
  label,
  status,
}: {
  detail: string;
  label: string;
  status: string;
}) {
  return (
    <article className="ap-card rounded border p-3" style={bursarPanelStyle()}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            {label}
          </h2>
          <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            {detail}
          </p>
        </div>
        <StatusChip>{status}</StatusChip>
      </div>
    </article>
  );
}

export function BursarSurface() {
  return (
    <main className="min-w-0 flex-1" data-testid="bursar-surface">
      <header className="ap-card mb-4 overflow-hidden rounded p-4" style={bursarPanelStyle()}>
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="max-w-3xl">
            <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
              Admin finance domain
            </p>
            <h1
              className="ap-register-chrome mt-2"
              style={{ fontSize: TYPE.scale.xl, fontWeight: 600, lineHeight: TYPE.line.display }}
            >
              Finance intelligence surface
            </h1>
            <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
              Bursar belongs under admin finance. It is not exposed on the standard employee
              dashboard, and it is not unlocked by project read grants.
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <StatusChip>route /admin/bursar</StatusChip>
            <StatusChip>not server-enforced yet</StatusChip>
            <StatusChip>no spend data connected</StatusChip>
          </div>
        </div>
      </header>

      <section className="grid grid-cols-1 gap-3 lg:grid-cols-3" aria-label="Bursar readiness">
        <ReadinessCard
          label="Access posture"
          detail="Future visibility needs explicit finance/admin authorization. Today the role contract only reports derived_only posture."
          status="candidate only"
        />
        <ReadinessCard
          label="Data posture"
          detail="The repo has finance documents and a finance agent, but no structured supplier, invoice, payment, spend, savings, or duplicate-payment store."
          status="not connected"
        />
        <ReadinessCard
          label="Grant posture"
          detail="Approved project read grants stay scoped to project/capability context and do not become finance-domain grants."
          status="not granted"
        />
      </section>

      <section className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-[0.9fr_1.1fr]">
        <div className="ap-card rounded border p-4" style={bursarPanelStyle()} data-testid="bursar-empty-state">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Unavailable state
          </p>
          <h2 className="ap-register-chrome mt-2" style={{ fontSize: TYPE.scale.lg, fontWeight: 600 }}>
            Bursar data model is not connected.
          </h2>
          <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
            This placeholder is intentionally empty. It does not show fake supplier names, fake invoice
            counts, fake spend totals, fake savings, or fake duplicate-payment findings.
          </p>
          <div className="mt-4 flex flex-wrap gap-2">
            <StatusChip>no charts</StatusChip>
            <StatusChip>no metrics</StatusChip>
            <StatusChip>no synthetic rows</StatusChip>
          </div>
        </div>

        <div className="ap-card rounded border p-4" style={bursarPanelStyle()} data-testid="bursar-source-map">
          <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            Required source map
          </p>
          <div className="mt-3 grid grid-cols-1 gap-2 sm:grid-cols-2">
            {REQUIRED_SOURCES.map((source) => (
              <div key={source} className="ap-card rounded p-2">
                <div className="flex items-center justify-between gap-2">
                  <span className="ap-register-chrome" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
                    {source}
                  </span>
                  <StatusChip>missing</StatusChip>
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section className="ap-card mt-4 rounded border p-4" style={bursarPanelStyle()} data-testid="bursar-governance-warning">
        <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
          Governance boundary
        </p>
        <div className="mt-3 grid grid-cols-1 gap-2 md:grid-cols-3">
          {BOUNDARIES.map((boundary) => (
            <p
              key={boundary}
              className="ap-soft rounded border px-3 py-2"
              style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
            >
              {boundary}
            </p>
          ))}
        </div>
      </section>
    </main>
  );
}
