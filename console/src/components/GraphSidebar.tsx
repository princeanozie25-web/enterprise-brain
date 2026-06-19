"use client";

import type { GraphDept, GraphResponse, OrgStats } from "@/lib/api";
import { DEPARTMENT_TINT, TYPE } from "@/lib/tokens";

/** The node KINDS the graph actually draws and can filter. Documents (600) are
 * never graph nodes. Project/capability nodes are grouped from real person
 * assignments and stay hidden until search/focus/trace to avoid decorative
 * density. */
export const FILTER_KINDS: { key: string; label: string }[] = [
  { key: "people", label: "People" },
  { key: "projects", label: "Projects" },
  { key: "agents", label: "Agents" },
  { key: "sources", label: "Sources" },
];

function Stat({ label, value }: { label: string; value: number | null }) {
  return (
    <div className="flex items-baseline justify-between gap-2 py-0.5" data-testid="sidebar-stat" data-key={label}>
      <span className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
        {label}
      </span>
      <span
        className="ap-register-evidence"
        style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
        data-testid="sidebar-stat-value"
      >
        {value === null ? "—" : value.toLocaleString("en-US")}
      </span>
    </div>
  );
}

/**
 * THE LEFT RAIL — the org's real cardinalities (every count derives from the
 * compiled artifacts + fixtures, never invented), the acting lens, type
 * filters that hide nodes without disturbing the layout, and the department
 * list (click = focus mode).
 */
export function GraphSidebar({
  orgName,
  actor,
  stats,
  graph,
  hiddenKinds,
  onToggleKind,
  focusDept,
  onFocusDept,
}: {
  orgName: string;
  actor: string;
  stats: OrgStats | null;
  graph: GraphResponse;
  hiddenKinds: string[];
  onToggleKind: (key: string) => void;
  focusDept: string | null;
  onFocusDept: (id: string | null) => void;
}) {
  const hidden = new Set(hiddenKinds);
  const headcount = (deptId: string) =>
    graph.people.filter((p) => p.department_id === deptId).length;

  return (
    <aside
      className="ap-card ap-elevated flex shrink-0 flex-col gap-3 overflow-y-auto rounded p-3"
      style={{ width: 232, maxHeight: "82vh" }}
      data-testid="graph-sidebar"
    >
      <div>
        <h2 className="ap-register-chrome" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
          Operating Map
        </h2>
        <p className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
          {orgName}
        </p>
      </div>

      <div className="ap-hairline border-t pt-2">
        <Stat label="People" value={stats?.people ?? null} />
        <Stat label="Departments" value={stats?.departments ?? null} />
        <Stat label="Documents" value={stats?.document_total ?? null} />
        <Stat label="Workflows" value={stats?.workflows ?? null} />
        <Stat label="Capabilities" value={stats?.capabilities ?? null} />
        <Stat label="Graph projects" value={graph.projects.length} />
        <Stat label="Agents" value={stats?.agents ?? null} />
        <Stat label="Sources" value={stats?.sources ?? null} />
        <Stat label="Groups" value={stats?.groups ?? null} />
        <Stat label="Permission edges" value={stats?.permission_edges ?? null} />
      </div>

      <div className="ap-hairline border-t pt-2">
        <p className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
          Work Identity
        </p>
        <p className="ap-register-evidence" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }} data-testid="sidebar-actor">
          {actor}
        </p>
        <p className="ap-soft mt-0.5" style={{ fontSize: TYPE.scale.xs }}>
          The graph shows company relationships visible to this Work Identity.
        </p>
      </div>

      <div className="ap-hairline border-t pt-2">
        <p className="ap-soft mb-1 uppercase tracking-wide" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
          Show
        </p>
        <div className="flex flex-wrap gap-1.5">
          {FILTER_KINDS.map((f) => {
            const on = !hidden.has(f.key);
            return (
              <button
                key={f.key}
                type="button"
                onClick={() => onToggleKind(f.key)}
                className={`ap-register-chrome rounded px-2 py-0.5${on ? " ap-affordance-button" : " ap-chip"}`}
                style={{ fontSize: TYPE.scale.xs }}
                data-testid="filter-toggle"
                data-kind={f.key}
                data-on={on ? "true" : "false"}
                aria-pressed={on}
              >
                {f.label}
              </button>
            );
          })}
        </div>
      </div>

      <div className="ap-hairline border-t pt-2">
        <p className="ap-soft mb-1 uppercase tracking-wide" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
          Departments
        </p>
        <ul className="space-y-0.5">
          {graph.departments.map((d: GraphDept) => {
            const tint = DEPARTMENT_TINT[d.tint_key];
            const active = focusDept === d.id;
            return (
              <li key={d.id}>
                <button
                  type="button"
                  onClick={() => onFocusDept(active ? null : d.id)}
                  className="ap-washable flex w-full items-center gap-2 rounded px-1.5 py-1 text-left"
                  style={{ fontSize: TYPE.scale.xs, outline: active ? "1px solid var(--affordance)" : undefined }}
                  data-testid="sidebar-dept"
                  data-dept={d.id}
                  data-active={active ? "true" : "false"}
                >
                  <span
                    aria-hidden="true"
                    className="inline-block shrink-0 rounded-full"
                    style={{
                      width: 9,
                      height: 9,
                      backgroundColor: tint ? tint.background : "var(--wash)",
                      border: `1px solid ${tint ? tint.border : "var(--hairline)"}`,
                    }}
                  />
                  <span className="min-w-0 flex-1 truncate">{d.label}</span>
                  <span className="ap-register-evidence ap-soft shrink-0">{headcount(d.id)}</span>
                </button>
              </li>
            );
          })}
        </ul>
      </div>
    </aside>
  );
}
