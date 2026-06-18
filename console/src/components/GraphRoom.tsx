"use client";

import { useCallback, useEffect, useState, type ReactNode } from "react";
import * as api from "@/lib/api";
import type { AccessRequestRecord, AccessTarget, GraphResponse, NodeSummary, OrgStats, RoleScopeSummary } from "@/lib/api";
import { COLOR, DERIVED, FONT, TYPE } from "@/lib/tokens";
import { OrgGraph, type SelectedNode } from "./OrgGraph";
import { GraphSidebar } from "./GraphSidebar";
import { GraphInspector } from "./GraphInspector";
import { MotionAside, MotionPanel } from "./MotionPrimitives";
import { Skeleton } from "./Skeleton";
import { graphRelationshipRows } from "./graphDisplay";

/**
 * Click-to-lens is a CROSS-LENS act: the current actor flies into the clicked
 * principal's lens, audited server-side. The route mirrors the room switcher's
 * `?as` carry, so /lens receives both actor and subject explicitly.
 */
export function lensHref(actor: string, subject: string): string {
  return `/lens?as=${encodeURIComponent(actor)}&subject=${encodeURIComponent(subject)}`;
}

const SUMMARISED = new Set(["org", "human", "agent"]);

const hiddenRailStyle = {
  position: "absolute" as const,
  width: 1,
  height: 1,
  padding: 0,
  margin: -1,
  overflow: "hidden",
  clip: "rect(0 0 0 0)",
  whiteSpace: "nowrap" as const,
  border: 0,
};

export function GraphRoom({
  adminPreview = false,
  actor,
  reducedMotion = false,
}: {
  adminPreview?: boolean;
  actor: string | null;
  reducedMotion?: boolean;
}) {
  const [graph, setGraph] = useState<GraphResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [available, setAvailable] = useState(true);
  const [stats, setStats] = useState<OrgStats | null>(null);
  const [roleScope, setRoleScope] = useState<RoleScopeSummary | null>(null);

  const [selected, setSelected] = useState<SelectedNode | null>(null);
  const [summary, setSummary] = useState<NodeSummary | null>(null);
  const [summaryLoading, setSummaryLoading] = useState(false);
  const [focusDept, setFocusDept] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [hiddenKinds, setHiddenKinds] = useState<string[]>([]);
  const [accessAvailable, setAccessAvailable] = useState(false);
  const [accessLoading, setAccessLoading] = useState(false);
  const [accessRequests, setAccessRequests] = useState<AccessRequestRecord[]>([]);
  const [accessInbox, setAccessInbox] = useState<AccessRequestRecord[]>([]);
  const [accessBusyId, setAccessBusyId] = useState<string | null>(null);
  const [accessFeedback, setAccessFeedback] = useState<{ kind: "success" | "error"; text: string } | null>(null);

  const refreshAccessRequests = useCallback(async () => {
    if (actor === null) {
      setAccessAvailable(false);
      setAccessRequests([]);
      setAccessInbox([]);
      return;
    }
    setAccessLoading(true);
    try {
      const [mine, inbox] = await Promise.all([
        api.getAccessRequests(actor),
        api.getAccessRequestInbox(actor),
      ]);
      const available = mine !== null || inbox !== null;
      setAccessAvailable(available);
      setAccessRequests(mine?.requests ?? []);
      setAccessInbox(inbox?.requests ?? []);
    } catch {
      setAccessAvailable(true);
      setAccessFeedback({ kind: "error", text: "Access requests could not be loaded. Try again." });
    } finally {
      setAccessLoading(false);
    }
  }, [actor]);

  useEffect(() => {
    if (!adminPreview || actor === null) {
      setRoleScope(null);
      return;
    }
    let cancelled = false;
    api
      .getRoleScope(actor)
      .then((response) => {
        if (!cancelled) setRoleScope(response);
      })
      .catch(() => {
        if (!cancelled) setRoleScope(null);
      });
    return () => {
      cancelled = true;
    };
  }, [actor, adminPreview]);

  useEffect(() => {
    if (actor === null) {
      setGraph(null);
      setStats(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setGraph(null);
    setStats(null);
    setSelected(null);
    setFocusDept(null);
    setAvailable(true);
    api
      .getGraph(actor)
      .then((response) => {
        if (!cancelled) {
          setGraph(response);
          setAvailable(response !== null);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setGraph(null);
          setAvailable(false);
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    api
      .getNodeSummary(actor, "org")
      .then((s) => {
        if (!cancelled) setStats(s?.stats ?? null);
      })
      .catch(() => {
        if (!cancelled) setStats(null);
      });
    return () => {
      cancelled = true;
    };
  }, [actor]);

  useEffect(() => {
    void refreshAccessRequests();
  }, [refreshAccessRequests]);

  useEffect(() => {
    if (actor === null || selected === null || !SUMMARISED.has(selected.kind)) {
      setSummary(null);
      setSummaryLoading(false);
      return;
    }
    let cancelled = false;
    setSummaryLoading(true);
    setSummary(null);
    api
      .getNodeSummary(actor, selected.id)
      .then((s) => {
        if (!cancelled) setSummary(s);
      })
      .catch(() => {
        if (!cancelled) setSummary(null);
      })
      .finally(() => {
        if (!cancelled) setSummaryLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [actor, selected]);

  const enterLens = (id: string) => {
    if (actor !== null) window.location.href = lensHref(actor, id);
  };
  const toggleKind = (key: string) =>
    setHiddenKinds((cur) => (cur.includes(key) ? cur.filter((k) => k !== key) : [...cur, key]));
  const requestAccess = async (target: AccessTarget, justification: string) => {
    if (actor === null) return;
    setAccessBusyId(`request:${target.capability_id}`);
    setAccessFeedback(null);
    try {
      await api.postAccessRequest(actor, target, justification);
      setAccessFeedback({ kind: "success", text: "Request recorded for manager review." });
      await refreshAccessRequests();
    } catch {
      setAccessFeedback({ kind: "error", text: "Request was not recorded. Check the target and try again." });
    } finally {
      setAccessBusyId(null);
    }
  };
  const decideAccess = async (requestId: string, decision: "approve" | "deny") => {
    if (actor === null) return;
    setAccessBusyId(`${decision}:${requestId}`);
    setAccessFeedback(null);
    try {
      await api.postAccessRequestDecision(actor, requestId, decision);
      setAccessFeedback({
        kind: "success",
        text: decision === "approve" ? "Request approved. Access is not expanded yet." : "Request denied.",
      });
      await refreshAccessRequests();
    } catch {
      setAccessFeedback({ kind: "error", text: "Decision was not recorded. Refresh and try again." });
    } finally {
      setAccessBusyId(null);
    }
  };

  const connectionCount = graph?.edges.length ?? 0;
  const shellStyle = {
    background:
      "radial-gradient(circle at 50% 54%, color-mix(in srgb, var(--affordance) 18%, transparent), transparent 35%), linear-gradient(180deg, color-mix(in srgb, var(--ink) 4%, transparent), transparent 18%), var(--paper)",
  };

  return (
    <div
      data-testid="graph-room"
      className="fixed inset-0 z-50 overflow-hidden"
      style={shellStyle}
      aria-label="Enterprise Brain graph"
    >
      <header
        className="ap-card fixed inset-x-0 top-0 z-30 grid items-center gap-4 border-x-0 border-t-0 px-6"
        style={{
          height: 58,
          gridTemplateColumns: "minmax(168px, 1fr) minmax(220px, 380px) minmax(168px, 1fr)",
          background: "color-mix(in srgb, var(--paper) 88%, transparent)",
          backdropFilter: "blur(18px)",
        }}
      >
        <div className="ap-soft min-w-0 truncate" style={{ fontSize: TYPE.scale.xs }}>
          <span>
            {adminPreview ? "Enterprise Brain / Operating Map" : "Enterprise Brain"} / Connections{" "}
            ({connectionCount.toLocaleString("en-US")})
          </span>
          {adminPreview && (
            <span className="block truncate" data-testid="admin-graph-preview-banner">
              Demo Identity Mode: admin-side preview, production admin authority not connected /{" "}
              {roleScope?.admin_surface_allowed ? "admin allowed by preview scope" : "admin not granted"}
            </span>
          )}
        </div>
        <div className="relative">
          <svg
            width={14}
            height={14}
            viewBox="0 0 24 24"
            aria-hidden="true"
            className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2"
          >
            <circle cx={11} cy={11} r={6.6} fill="none" stroke={C.inkSoft} strokeWidth={2} />
            <path d="m16 16 4.2 4.2" fill="none" stroke={C.inkSoft} strokeWidth={2} strokeLinecap="round" />
          </svg>
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search"
            className="w-full rounded-full py-1.5 pl-9 pr-8 text-center"
            style={{
              fontSize: TYPE.scale.xs,
              background: "color-mix(in srgb, var(--wash) 72%, transparent)",
            }}
            data-testid="graph-search"
          />
          {query.length > 0 && (
            <button
              type="button"
              onClick={() => setQuery("")}
              className="ap-soft absolute right-2 top-1/2 -translate-y-1/2 rounded-full px-1"
              style={{ fontSize: TYPE.scale.xs }}
              aria-label="Clear search"
              data-testid="graph-search-clear"
            >
              x
            </button>
          )}
        </div>
        <div className="flex min-w-0 items-center justify-end gap-2">
          <ThemeToggle />
          <span
            className="ap-card ap-register-chrome inline-flex items-center gap-2 rounded-full px-3 py-1"
            style={{ fontSize: TYPE.scale.xs }}
            aria-hidden="true"
          >
            <svg width={13} height={13} viewBox="0 0 24 24" aria-hidden="true">
              <path
                d="M5 6h14v9H8l-3 3V6Z"
                fill="none"
                stroke={C.ink}
                strokeWidth={2}
                strokeLinejoin="round"
              />
            </svg>
            Chat
          </span>
        </div>
      </header>

      {actor === null ? (
        <div className="grid h-full place-items-center px-6 pt-16">
          <GraphEmpty
            testid="graph-room-empty"
            headline="Choose a Work Identity to open the Operating Map."
            sub="The map draws only company relationships visible to that identity. No selection means no permission scope."
          />
        </div>
      ) : loading ? (
        <div className="grid h-full place-items-center px-6 pt-16">
          <div className="ap-card w-full max-w-xl rounded p-4">
            <Skeleton lines={6} />
          </div>
        </div>
      ) : !available ? (
        <div className="grid h-full place-items-center px-6 pt-16">
          <GraphEmpty
            testid="graph-unavailable"
            headline="No organizational view in your scope."
            sub="This Work Identity does not include a company graph. Nothing is withheld here; there is simply nothing to draw."
          />
        </div>
      ) : graph !== null && graph.people.length === 0 ? (
        <div className="grid h-full place-items-center px-6 pt-16">
          <GraphEmpty
            testid="graph-empty"
            headline="No organizational view in your scope."
            sub="This Work Identity does not include a company graph. Nothing is withheld here; there is simply nothing to draw."
          />
        </div>
      ) : graph !== null ? (
        <>
          <section
            className="absolute inset-x-0 bottom-0 top-[58px] overflow-hidden"
            data-testid="graph-stage"
          >
            <OrgGraph
              graph={graph}
              onSelectNode={setSelected}
              onFocusDept={setFocusDept}
              selectedId={selected?.id ?? null}
              focusDept={focusDept}
              query={query}
              hiddenKinds={hiddenKinds}
              reducedMotion={reducedMotion}
            />
          </section>

          <div style={hiddenRailStyle}>
            <GraphSidebar
              orgName={graph.center.label}
              actor={actor}
              stats={stats}
              graph={graph}
              hiddenKinds={hiddenKinds}
              onToggleKind={toggleKind}
              focusDept={focusDept}
              onFocusDept={setFocusDept}
            />
          </div>

          <GraphAuditPanel actor={actor} graph={graph} selected={selected} />

          {accessAvailable && (
            <AccessRequestRail
              graph={graph}
              loading={accessLoading}
              requests={accessRequests}
              inbox={accessInbox}
              busyId={accessBusyId}
              feedback={accessFeedback}
              onDecide={decideAccess}
            />
          )}

          {selected !== null && (
            <MotionPanel className="fixed right-4 top-[74px] z-20">
              <GraphInspector
                actor={actor}
                node={selected}
                summary={summary}
                loading={summaryLoading}
                graph={graph}
                accessRequests={accessRequests}
                accessRequestBusy={accessBusyId === `request:${selected.id}`}
                accessRequestFeedback={accessFeedback}
                onRequestAccess={requestAccess}
                onEnterLens={enterLens}
                onClose={() => setSelected(null)}
              />
            </MotionPanel>
          )}

          <Legend systems={graph.sources.length} agents={graph.tools.length} projects={graph.projects.length} people={graph.people.length} />
        </>
      ) : null}
    </div>
  );
}

const C = {
  ink: "var(--ink)",
  inkSoft: "var(--ink-soft)",
  affordance: "var(--affordance)",
  warm: "var(--accent-warm)",
  hairline: "var(--hairline)",
};

function GraphAuditPanel({
  actor,
  graph,
  selected,
}: {
  actor: string;
  graph: GraphResponse;
  selected: SelectedNode | null;
}) {
  const relationships = graphRelationshipRows(graph, selected?.id).slice(0, 5);
  const total = selected === null ? graph.edges.length : graphRelationshipRows(graph, selected.id).length;
  const selectedLabel = selected?.label.replace(/^Capability:\s*/i, "") ?? "the current Work Identity";

  return (
    <MotionAside
      className="ap-card fixed left-4 top-[74px] z-20 flex w-[336px] max-w-[calc(100vw-32px)] flex-col gap-3 rounded p-3"
      style={{ background: "color-mix(in srgb, var(--paper) 87%, transparent)", backdropFilter: "blur(16px)" }}
      data-testid="graph-audit-panel"
      aria-label="Audited Operating Map context"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-soft uppercase tracking-wide" style={{ fontSize: TYPE.scale.xs, fontWeight: 700 }}>
            Audited view
          </p>
          <h2 className="ap-register-chrome mt-0.5 truncate" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
            Operating Map
          </h2>
        </div>
        <span
          className="ap-hairline ap-register-chrome ap-soft shrink-0 rounded-full border px-2 py-1"
          style={{ fontSize: TYPE.scale.xs }}
          data-testid="graph-acting-context"
        >
          Acting as {actor}
        </span>
      </div>

      <p className="ap-soft" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }} data-testid="graph-audited-line">
        This view is audited. It draws only relationship records returned for this Work Identity; hidden or restricted
        relationships are not shown.
      </p>

      <div className="grid grid-cols-3 gap-1.5" aria-label="Data-backed map counts">
        {[
          ["relationships", graph.edges.length],
          ["people", graph.people.length],
          ["projects", graph.projects.length],
        ].map(([label, value]) => (
          <div key={label} className="ap-hairline rounded border px-2 py-1.5">
            <p className="ap-register-evidence" style={{ fontSize: TYPE.scale.sm, fontWeight: 700 }}>
              {Number(value).toLocaleString("en-US")}
            </p>
            <p className="ap-soft truncate" style={{ fontSize: TYPE.scale.xs }}>
              {label}
            </p>
          </div>
        ))}
      </div>

      <section className="space-y-1.5" data-testid="graph-relationship-summary" aria-label="Relationship summary">
        <div className="flex items-baseline justify-between gap-2">
          <p className="ap-soft uppercase tracking-wide" style={{ fontSize: TYPE.scale.xs, fontWeight: 700 }}>
            {selected === null ? "Relationship summary" : "Selected trace"}
          </p>
          <span className="ap-soft shrink-0" style={{ fontSize: TYPE.scale.xs }}>
            {total.toLocaleString("en-US")} records
          </span>
        </div>
        <p className="ap-soft" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
          {selected === null
            ? "Keyboard-readable rows from the same graph payload."
            : `Relationships connected to ${selectedLabel}.`}
        </p>
        {relationships.length === 0 ? (
          <p className="ap-soft" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            No relationship records are available for this selection.
          </p>
        ) : (
          <ul className="space-y-1">
            {relationships.map((row) => (
              <li key={row.key} className="ap-hairline rounded border px-2 py-1.5" data-testid="graph-relationship-row">
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
      </section>
    </MotionAside>
  );
}

function Legend({ systems, agents, projects, people }: { systems: number; agents: number; projects: number; people: number }) {
  const itemStyle = { fontSize: TYPE.scale.xs - 2 };
  const dot = (background: string, square = false) => (
    <span
      aria-hidden="true"
      className="inline-block shrink-0"
      style={{
        width: square ? 9 : 8,
        height: square ? 9 : 8,
        borderRadius: square ? 2 : 999,
        background,
      }}
    />
  );
  return (
    <MotionAside
      className="ap-card fixed bottom-4 left-5 z-20 flex max-w-[calc(100vw-40px)] flex-wrap items-center gap-3 rounded-full px-3 py-2"
      style={{ background: "color-mix(in srgb, var(--paper) 84%, transparent)", backdropFilter: "blur(14px)" }}
      aria-label="Graph legend"
    >
      <span className="ap-soft inline-flex items-center gap-1.5" style={itemStyle}>
        {dot(C.affordance)} systems {systems}
      </span>
      <span className="ap-soft inline-flex items-center gap-1.5" style={itemStyle}>
        {dot(C.hairline, true)} agents {agents}
      </span>
      <span className="ap-soft inline-flex items-center gap-1.5" style={itemStyle}>
        {dot(C.warm, true)} projects {projects}
      </span>
      <span className="ap-soft inline-flex items-center gap-1.5" style={itemStyle}>
        {dot(C.ink)} people {people}
      </span>
    </MotionAside>
  );
}

function RailHeading({ children }: { children: ReactNode }) {
  return (
    <p className="ap-soft uppercase tracking-wide" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
      {children}
    </p>
  );
}

function AccessRequestRail({
  graph,
  loading,
  requests,
  inbox,
  busyId,
  feedback,
  onDecide,
}: {
  graph: GraphResponse;
  loading: boolean;
  requests: AccessRequestRecord[];
  inbox: AccessRequestRecord[];
  busyId: string | null;
  feedback: { kind: "success" | "error"; text: string } | null;
  onDecide: (requestId: string, decision: "approve" | "deny") => Promise<void>;
}) {
  const projectLabel = (request: AccessRequestRecord) => {
    const project = graph.projects.find((item) => item.id === request.target.capability_id);
    return project?.label.replace(/^Capability:\s*/i, "") ?? request.target.capability_id;
  };
  const recent = requests.slice(-3).reverse();

  return (
    <MotionAside
      className="ap-card fixed left-4 top-[386px] z-20 flex max-h-[280px] w-[292px] max-w-[calc(100vw-32px)] flex-col gap-3 overflow-y-auto rounded p-3"
      style={{ background: "color-mix(in srgb, var(--paper) 86%, transparent)", backdropFilter: "blur(14px)" }}
      data-testid="access-request-rail"
      aria-label="Access requests"
    >
      <div className="flex items-baseline justify-between gap-2">
        <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
          Access requests
        </p>
        {loading && (
          <span className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Loading
          </span>
        )}
      </div>

      {feedback && (
        <p
          role={feedback.kind === "error" ? "alert" : "status"}
          className={feedback.kind === "success" ? "ap-register-chrome" : "ap-soft"}
          style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
          data-testid="access-rail-feedback"
        >
          {feedback.text}
        </p>
      )}

      <section className="space-y-1.5">
        <RailHeading>My status</RailHeading>
        {recent.length === 0 ? (
          <p className="ap-soft" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            No access requests recorded.
          </p>
        ) : (
          recent.map((request) => (
            <div key={request.request_id} className="ap-hairline rounded border px-2 py-1.5" data-testid="access-request-row">
              <div className="flex items-baseline justify-between gap-2">
                <span className="ap-register-chrome truncate" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
                  {projectLabel(request)}
                </span>
                <span className="ap-soft shrink-0" style={{ fontSize: TYPE.scale.xs }}>
                  {request.status}
                </span>
              </div>
              <p className="ap-soft mt-1 truncate" style={{ fontSize: TYPE.scale.xs }}>
                Reviewer {request.approver_id}
              </p>
            </div>
          ))
        )}
      </section>

      <section className="space-y-1.5">
        <RailHeading>Review inbox</RailHeading>
        {inbox.length === 0 ? (
          <p className="ap-soft" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            No pending reviews.
          </p>
        ) : (
          inbox.map((request) => (
            <div key={request.request_id} className="ap-hairline rounded border px-2 py-2" data-testid="access-inbox-row">
              <div className="flex items-baseline justify-between gap-2">
                <span className="ap-register-chrome truncate" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
                  {projectLabel(request)}
                </span>
                <span className="ap-soft shrink-0" style={{ fontSize: TYPE.scale.xs }}>
                  {request.requester_id}
                </span>
              </div>
              <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
                {request.justification}
              </p>
              <div className="mt-2 grid grid-cols-2 gap-1.5">
                <button
                  type="button"
                  disabled={busyId === `approve:${request.request_id}` || busyId === `deny:${request.request_id}`}
                  onClick={() => void onDecide(request.request_id, "approve")}
                  className="ap-affordance-button ap-register-chrome rounded px-2 py-1.5 disabled:cursor-not-allowed disabled:opacity-50"
                  style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
                  data-testid="access-approve"
                >
                  Approve
                </button>
                <button
                  type="button"
                  disabled={busyId === `approve:${request.request_id}` || busyId === `deny:${request.request_id}`}
                  onClick={() => void onDecide(request.request_id, "deny")}
                  className="ap-washable ap-register-chrome rounded px-2 py-1.5 disabled:cursor-not-allowed disabled:opacity-50"
                  style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
                  data-testid="access-deny"
                >
                  Deny
                </button>
              </div>
            </div>
          ))
        )}
      </section>
    </MotionAside>
  );
}

function ThemeToggle() {
  const [theme, setTheme] = useState<"dark" | "light">("dark");
  useEffect(() => {
    const current = document.documentElement.getAttribute("data-theme");
    setTheme(current === "light" ? "light" : "dark");
  }, []);
  const flip = () => {
    const next = theme === "dark" ? "light" : "dark";
    setTheme(next);
    document.documentElement.setAttribute("data-theme", next);
    try {
      localStorage.setItem("ap-theme", next);
    } catch {
      /* private mode: the choice just will not persist */
    }
  };
  return (
    <button
      type="button"
      onClick={flip}
      className="ap-card ap-washable grid rounded-full"
      style={{ width: 34, height: 34, placeItems: "center" }}
      data-testid="theme-toggle"
      data-theme={theme}
      aria-label={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
      title={theme === "dark" ? "Light mode" : "Dark mode"}
    >
      <svg width={16} height={16} viewBox="0 0 24 24" aria-hidden="true">
        <circle cx={12} cy={12} r={4} fill="none" stroke={C.ink} strokeWidth={2} />
        <path
          d="M12 2v2.5M12 19.5V22M4.93 4.93 6.7 6.7M17.3 17.3l1.77 1.77M2 12h2.5M19.5 12H22M4.93 19.07 6.7 17.3M17.3 6.7l1.77-1.77"
          fill="none"
          stroke={C.ink}
          strokeWidth={2}
          strokeLinecap="round"
        />
      </svg>
    </button>
  );
}

function GraphEmpty({
  testid,
  headline,
  sub,
}: {
  testid: string;
  headline: string;
  sub: string;
}) {
  const satellites = [-Math.PI / 2, Math.PI / 6, (5 * Math.PI) / 6];
  return (
    <MotionPanel
      className="ap-card flex flex-col items-center gap-3 rounded px-6 py-12 text-center"
      data-testid={testid}
    >
      <svg width={64} height={64} viewBox="0 0 64 64" aria-hidden="true">
        <circle cx={32} cy={32} r={26} fill="none" stroke={DERIVED.hairline} strokeWidth={1} />
        {satellites.map((angle, index) => (
          <line
            key={index}
            x1={32}
            y1={32}
            x2={32 + 26 * Math.cos(angle)}
            y2={32 + 26 * Math.sin(angle)}
            stroke={DERIVED.hairline}
            strokeWidth={1}
            strokeOpacity={0.6}
          />
        ))}
        {satellites.map((angle, index) => (
          <circle
            key={`s${index}`}
            cx={32 + 26 * Math.cos(angle)}
            cy={32 + 26 * Math.sin(angle)}
            r={4}
            fill={DERIVED.wash}
            stroke={DERIVED.hairline}
            strokeWidth={1}
          />
        ))}
        <circle cx={32} cy={32} r={10} fill={COLOR.ink} opacity={0.55} />
      </svg>
      <p
        className="ap-register-chrome"
        style={{ fontFamily: FONT.chrome, fontSize: TYPE.scale.sm, fontWeight: 600 }}
      >
        {headline}
      </p>
      <p className="ap-soft" style={{ fontSize: TYPE.scale.xs, maxWidth: 300, lineHeight: TYPE.line.body }}>
        {sub}
      </p>
    </MotionPanel>
  );
}
