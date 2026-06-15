"use client";

import type { GraphResponse, NodeSummary } from "@/lib/api";
import type { SelectedNode } from "./OrgGraph";
import { TYPE } from "@/lib/tokens";
import { Skeleton } from "./Skeleton";

function Chip({ children, mono = false }: { children: React.ReactNode; mono?: boolean }) {
  return (
    <span
      className={`ap-hairline ${mono ? "ap-register-evidence" : "ap-register-chrome"} ap-soft rounded border px-1.5 py-0.5`}
      style={{ fontSize: TYPE.scale.xs }}
    >
      {children}
    </span>
  );
}

function Heading({ children }: { children: React.ReactNode }) {
  return (
    <p className="ap-soft uppercase tracking-wide" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
      {children}
    </p>
  );
}

/** Sees X of N — the scope-filtering magnitude, honest at the count level
 * (the documents themselves never enter the graph; they open in the lens). */
function Reach({ visible, corpus }: { visible: number; corpus: number }) {
  const pct = corpus > 0 ? Math.round((visible / corpus) * 100) : 0;
  return (
    <div data-testid="inspector-reach">
      <Heading>Reach</Heading>
      <p className="mt-1" style={{ fontSize: TYPE.scale.sm }}>
        <span className="ap-register-evidence" style={{ fontWeight: 600 }}>{visible.toLocaleString("en-US")}</span>
        <span className="ap-soft"> of {corpus.toLocaleString("en-US")} documents </span>
        <span className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>({pct}%)</span>
      </p>
      <div className="mt-1 h-1.5 w-full overflow-hidden rounded" style={{ backgroundColor: "var(--wash)" }}>
        <div style={{ width: `${pct}%`, height: "100%", backgroundColor: "var(--affordance)" }} />
      </div>
      <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
        Hidden from this lens: {(corpus - visible).toLocaleString("en-US")}. The documents open in the audited lens.
      </p>
    </div>
  );
}

/**
 * THE RIGHT INSPECTOR — where the governance shows. Every block reads REAL
 * data: a person/agent's compiled scope and reason-grouped access (from the M1
 * artifacts), an agent's permitted/blocked actions (the M4 authority), the org
 * cardinalities. It never dumps another principal's documents — that is the
 * audited lens, one explicit click away.
 */
export function GraphInspector({
  node,
  summary,
  loading,
  graph,
  onEnterLens,
  onClose,
}: {
  node: SelectedNode;
  summary: NodeSummary | null;
  loading: boolean;
  graph: GraphResponse;
  onEnterLens: (id: string) => void;
  onClose: () => void;
}) {
  return (
    <aside
      className="ap-card flex shrink-0 flex-col gap-3 overflow-y-auto rounded p-3"
      style={{ width: 304, maxHeight: "82vh" }}
      data-testid="inspector-card"
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="ap-register-chrome truncate" style={{ fontSize: TYPE.scale.md, fontWeight: 600 }} data-testid="inspector-name">
            {node.label}
          </p>
          <span
            className="ap-hairline ap-register-chrome ap-soft mt-1 inline-block rounded border px-1.5 py-0.5"
            style={{ fontSize: TYPE.scale.xs }}
            data-testid="inspector-kind"
          >
            {node.kind}
          </span>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="ap-washable ap-soft rounded px-1.5"
          style={{ fontSize: TYPE.scale.md }}
          aria-label="Close inspector"
          data-testid="inspector-close"
        >
          ×
        </button>
      </div>

      {loading && <Skeleton lines={4} />}

      {!loading && node.kind === "org" && summary?.stats && (
        <div className="space-y-2" data-testid="inspector-org">
          <p className="ap-soft" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
            The whole company, scope-filtered. Every node is artifact-backed; the graph carries no holdings.
          </p>
          <div>
            <Heading>Corpus</Heading>
            {(
              [
                ["People", summary.stats.people],
                ["Departments", summary.stats.departments],
                ["Documents", summary.stats.document_total],
                ["Workflows", summary.stats.workflows],
                ["Capabilities", summary.stats.capabilities],
                ["Agents", summary.stats.agents],
                ["Sources", summary.stats.sources],
                ["Groups", summary.stats.groups],
                ["Permission edges", summary.stats.permission_edges],
                ["Compiled decisions", summary.stats.total_decisions],
              ] as const
            ).map(([label, value]) => (
              <div key={label} className="flex items-baseline justify-between py-0.5" data-testid="inspector-stat">
                <span className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>{label}</span>
                <span className="ap-register-evidence" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
                  {value.toLocaleString("en-US")}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {!loading && node.kind === "department" && (
        <div className="space-y-2" data-testid="inspector-department">
          <Heading>Department</Heading>
          <div className="flex flex-wrap gap-1.5">
            <Chip>{graph.people.filter((p) => p.department_id === node.id).length} people</Chip>
            {graph.people
              .filter((p) => p.department_id === node.id && p.ring === "anchor")
              .map((p) => (
                <Chip key={p.id}>Head: {p.display_name}</Chip>
              ))}
          </div>
          <p className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Click a person to inspect their compiled access.
          </p>
        </div>
      )}

      {!loading && node.kind === "source" && (
        <div className="space-y-2" data-testid="inspector-source">
          <Heading>System of record</Heading>
          <p style={{ fontSize: TYPE.scale.sm }}>{node.label}</p>
          <p className="ap-soft" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            A real org system. Which documents live here is never drawn in the graph — that is the audited
            lens&apos;s job (no holdings leak).
          </p>
        </div>
      )}

      {!loading && node.kind === "human" && summary && (
        <div className="space-y-3" data-testid="inspector-human">
          {summary.title && <p style={{ fontSize: TYPE.scale.sm }}>{summary.title}</p>}
          <div className="flex flex-wrap gap-1.5">
            {summary.department && <Chip>{summary.department}</Chip>}
            {summary.band !== undefined && <Chip mono>band {summary.band}</Chip>}
            {(summary.groups ?? []).map((g) => (
              <Chip key={g} mono>{g}</Chip>
            ))}
            {(summary.sites ?? []).map((s) => (
              <Chip key={s} mono>{s}</Chip>
            ))}
          </div>
          {(summary.reports_to || summary.manages !== undefined) && (
            <p className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
              {summary.reports_to ? `Reports to ${summary.reports_to}` : ""}
              {summary.reports_to && summary.manages ? " · " : ""}
              {summary.manages ? `Manages ${summary.manages}` : ""}
            </p>
          )}
          {summary.visible_documents !== undefined && summary.corpus_documents !== undefined && (
            <Reach visible={summary.visible_documents} corpus={summary.corpus_documents} />
          )}
          {(summary.access_by_reason ?? []).length > 0 && (
            <div>
              <Heading>Access granted via</Heading>
              <ul className="mt-1 space-y-1">
                {(summary.access_by_reason ?? []).map((r) => (
                  <li key={r.reason} className="flex items-baseline justify-between gap-2" data-testid="inspector-reason">
                    <span style={{ fontSize: TYPE.scale.xs }}>{r.sentence}</span>
                    <span className="ap-register-evidence ap-soft shrink-0" style={{ fontSize: TYPE.scale.xs }}>
                      {r.granted}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          )}
          {(summary.agents_owned ?? []).length > 0 && (
            <div>
              <Heading>Owns agents</Heading>
              <div className="mt-1 flex flex-wrap gap-1.5">
                {(summary.agents_owned ?? []).map((a) => (
                  <Chip key={a.id}>{a.name}</Chip>
                ))}
              </div>
            </div>
          )}
          <button
            type="button"
            onClick={() => onEnterLens(node.id)}
            className="ap-affordance-button ap-register-chrome w-full rounded px-3 py-1.5"
            style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
            data-testid="inspector-enter-lens"
          >
            Enter {node.label}&apos;s lens →
          </button>
          <p className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Opening the lens is a cross-lens act, audited server-side.
          </p>
        </div>
      )}

      {!loading && node.kind === "agent" && summary && (
        <div className="space-y-3" data-testid="inspector-agent">
          <div className="flex flex-wrap gap-1.5">
            {summary.owner_user_id && <Chip mono>owner {summary.owner_user_id}</Chip>}
            {(summary.grant_groups ?? []).map((g) => (
              <Chip key={g} mono>{g}</Chip>
            ))}
          </div>
          {summary.visible_documents !== undefined && summary.corpus_documents !== undefined && (
            <Reach visible={summary.visible_documents} corpus={summary.corpus_documents} />
          )}
          <div>
            <Heading>May</Heading>
            <ul className="mt-1 space-y-0.5">
              {(summary.permitted_actions ?? []).map((a) => (
                <li key={a} style={{ fontSize: TYPE.scale.xs }} data-testid="inspector-permitted">
                  ✓ {a.replace(/_/g, " ")}
                </li>
              ))}
            </ul>
          </div>
          <div>
            <Heading>May not</Heading>
            <ul className="mt-1 space-y-0.5">
              {(summary.blocked_actions ?? []).map((a) => (
                <li key={a} className="ap-soft" style={{ fontSize: TYPE.scale.xs }} data-testid="inspector-blocked">
                  ✕ {a.replace(/_/g, " ")}
                </li>
              ))}
            </ul>
          </div>
          <p className="ap-soft" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            Agent — every mutation is human-gated; it may only retrieve within its compiled allowlist and propose.
          </p>
        </div>
      )}
    </aside>
  );
}
