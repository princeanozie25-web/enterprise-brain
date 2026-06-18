"use client";

import { useState, type FormEvent } from "react";
import type { GraphResponse, NodeSummary } from "@/lib/api";
import type { AccessRequestRecord, AccessTarget } from "@/lib/api";
import type { SelectedNode } from "./OrgGraph";
import { TYPE } from "@/lib/tokens";
import { Skeleton } from "./Skeleton";
import { graphRelationshipRows, type GraphRelationshipRow } from "./graphDisplay";

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

function RelationshipTrace({ rows }: { rows: GraphRelationshipRow[] }) {
  return (
    <div className="space-y-1.5" data-testid="inspector-relationship-trace">
      <Heading>Relationship trace</Heading>
      {rows.length === 0 ? (
        <p className="ap-soft" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
          No relationship records are returned for this node.
        </p>
      ) : (
        <ul className="space-y-1">
          {rows.map((row) => (
            <li key={row.key} className="ap-hairline rounded border px-2 py-1.5" data-testid="inspector-relationship-row">
              <p className="ap-register-chrome truncate" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
                {row.from.label}
              </p>
              <p className="ap-soft truncate" style={{ fontSize: TYPE.scale.xs }}>
                {row.relation} {row.to.label}
              </p>
            </li>
          ))}
        </ul>
      )}
    </div>
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
        Hidden from this Work Identity: {(corpus - visible).toLocaleString("en-US")}. Documents open in the audited Knowledge View.
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
  actor = null,
  node,
  summary,
  loading,
  graph,
  accessRequests = [],
  accessRequestBusy = false,
  accessRequestFeedback = null,
  onRequestAccess,
  onEnterLens,
  onClose,
}: {
  actor?: string | null;
  node: SelectedNode;
  summary: NodeSummary | null;
  loading: boolean;
  graph: GraphResponse;
  accessRequests?: AccessRequestRecord[];
  accessRequestBusy?: boolean;
  accessRequestFeedback?: { kind: "success" | "error"; text: string } | null;
  onRequestAccess?: (target: AccessTarget, justification: string) => Promise<void>;
  onEnterLens: (id: string) => void;
  onClose: () => void;
}) {
  const selectedProject = graph.projects.find((project) => project.id === node.id);
  const selectedRelationships = graphRelationshipRows(graph, node.id).slice(0, 5);

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

      {!loading && <RelationshipTrace rows={selectedRelationships} />}

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
            Select a person to inspect visible Work Identity details.
          </p>
        </div>
      )}

      {!loading && node.kind === "source" && (
        <div className="space-y-2" data-testid="inspector-source">
          <Heading>System of record</Heading>
          <p style={{ fontSize: TYPE.scale.sm }}>{node.label}</p>
          <p className="ap-soft" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            A real org system. Documents stay out of the graph and open through the audited Knowledge View.
          </p>
        </div>
      )}

      {!loading && node.kind === "project" && selectedProject && (
        <div className="space-y-3" data-testid="inspector-project">
          <div className="flex flex-wrap gap-1.5">
            <Chip mono>{selectedProject.id}</Chip>
            <Chip>{selectedProject.people} people</Chip>
            <Chip>{selectedProject.departments.length} departments</Chip>
          </div>
          <div>
            <Heading>Project trace</Heading>
            <p style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
              {selectedProject.workflow_name}
            </p>
            <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
              {selectedProject.initiative_name} / {selectedProject.strategy_name}
            </p>
          </div>
          <div>
            <Heading>Departments involved</Heading>
            <div className="mt-1 flex flex-wrap gap-1.5">
              {selectedProject.departments.map((department) => (
                <Chip key={department}>{department}</Chip>
              ))}
            </div>
          </div>
          <div>
            <Heading>Status mix</Heading>
            <div className="mt-1 flex flex-wrap gap-1.5">
              {Object.entries(selectedProject.status_counts).map(([status, count]) => (
                <Chip key={status}>
                  {status}: {count}
                </Chip>
              ))}
            </div>
          </div>
          {actor !== null && (
            <a
              href={`/project?cap=${encodeURIComponent(selectedProject.id)}&as=${encodeURIComponent(actor)}`}
              className="ap-affordance-button ap-register-chrome block rounded px-3 py-1.5 text-center"
              style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
              data-testid="project-surface-link"
            >
              Open project surface
            </a>
          )}
          {actor !== null && onRequestAccess && (
            <ProjectAccessRequest
              actor={actor}
              projectId={selectedProject.id}
              requests={accessRequests}
              busy={accessRequestBusy}
              feedback={accessRequestFeedback}
              onRequestAccess={onRequestAccess}
            />
          )}
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
            Open {node.label}&apos;s Knowledge View
          </button>
          <p className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Opening another Work Identity&apos;s Knowledge View is audited server-side.
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

function ProjectAccessRequest({
  actor,
  projectId,
  requests,
  busy,
  feedback,
  onRequestAccess,
}: {
  actor: string;
  projectId: string;
  requests: AccessRequestRecord[];
  busy: boolean;
  feedback: { kind: "success" | "error"; text: string } | null;
  onRequestAccess: (target: AccessTarget, justification: string) => Promise<void>;
}) {
  const [justification, setJustification] = useState("");
  const [localError, setLocalError] = useState<string | null>(null);
  const existing = requests.find(
    (request) =>
      request.requester_id === actor &&
      request.target.capability_id === projectId &&
      (request.target.kind === "project" || request.target.kind === "capability"),
  );
  const disabled = busy || existing?.status === "pending" || existing?.status === "approved";

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const text = justification.trim();
    if (text.length < 8) {
      setLocalError("Add a short reason for the reviewer.");
      return;
    }
    setLocalError(null);
    await onRequestAccess({ kind: "project", capability_id: projectId }, text);
    setJustification("");
  };

  return (
    <form className="space-y-2 border-t pt-3" style={{ borderColor: "var(--hairline)" }} onSubmit={submit}>
      <div>
        <Heading>Access request</Heading>
        <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
          Requests are reviewed and audited. Approval changes request status only.
        </p>
      </div>
      {existing && (
        <div className="ap-hairline ap-soft rounded border px-2 py-1.5" style={{ fontSize: TYPE.scale.xs }}>
          Current request: <span className="ap-register-chrome">{existing.status}</span>
        </div>
      )}
      <label className="block space-y-1">
        <span className="ap-soft block" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
          Justification
        </span>
        <textarea
          value={justification}
          onChange={(event) => setJustification(event.target.value)}
          disabled={disabled}
          rows={3}
          className="ap-card w-full resize-none rounded px-2 py-2 disabled:cursor-not-allowed disabled:opacity-50"
          style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
          data-testid="access-request-justification"
        />
      </label>
      {(localError || feedback) && (
        <p
          role={localError || feedback?.kind === "error" ? "alert" : "status"}
          className={feedback?.kind === "success" ? "ap-register-chrome" : "ap-soft"}
          style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
          data-testid="access-request-feedback"
        >
          {localError ?? feedback?.text}
        </p>
      )}
      <button
        type="submit"
        disabled={disabled}
        className="ap-affordance-button ap-register-chrome w-full rounded px-3 py-2 disabled:cursor-not-allowed disabled:opacity-50"
        style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
        data-testid="access-request-submit"
      >
        {busy ? "Submitting" : existing?.status === "pending" ? "Request pending" : "Request access"}
      </button>
    </form>
  );
}
