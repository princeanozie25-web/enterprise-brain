import { useMemo, useState } from "react";
import type { ScopeStatement } from "@/lib/api";
import {
  PRINCIPAL_LIST_HEIGHT,
  PRINCIPAL_ROW_HEIGHT,
  VIRTUAL_OVERSCAN,
} from "@/lib/constants";
import { PRINCIPALS } from "@/lib/principals";

/**
 * The identity rail: the permanent demo-identity banner (furniture, not a
 * tooltip), the searchable principal switcher (virtualized over the 124 demo
 * principals), and the always-visible "What I can see" scope panel.
 */
export function IdentityRail({
  principal,
  scope,
  onSwitch,
}: {
  principal: string | null;
  /** null while loading or when no principal is selected. */
  scope: ScopeStatement | null;
  onSwitch: (principal: string) => void;
}) {
  const [search, setSearch] = useState("");
  const [scrollTop, setScrollTop] = useState(0);

  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return needle.length === 0
      ? PRINCIPALS
      : PRINCIPALS.filter((p) => p.toLowerCase().includes(needle));
  }, [search]);

  // Minimal fixed-height virtualization — 124 rows, no dependency needed.
  const first = Math.max(0, Math.floor(scrollTop / PRINCIPAL_ROW_HEIGHT) - VIRTUAL_OVERSCAN);
  const visibleCount =
    Math.ceil(PRINCIPAL_LIST_HEIGHT / PRINCIPAL_ROW_HEIGHT) + VIRTUAL_OVERSCAN * 2;
  const visible = filtered.slice(first, first + visibleCount);

  return (
    <div className="flex h-full flex-col gap-3" data-testid="identity-rail">
      <div
        className="rounded border border-amber-300 bg-amber-50 px-3 py-2 text-[11px] font-semibold uppercase tracking-wide text-amber-900"
        data-testid="demo-banner"
      >
        Demo identity mode — not an authentication system
      </div>

      <div className="rounded-lg border border-stone-200 bg-white">
        <div className="border-b border-stone-100 px-3 py-2">
          <label htmlFor="principal-search" className="text-xs font-medium text-stone-500">
            Principal
          </label>
          <input
            id="principal-search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search 124 demo principals"
            className="mt-1 w-full rounded border border-stone-200 px-2 py-1 text-sm"
            data-testid="principal-search"
          />
        </div>
        <div
          className="overflow-y-auto"
          style={{ height: PRINCIPAL_LIST_HEIGHT }}
          onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
          data-testid="principal-list"
        >
          <div
            style={{ height: filtered.length * PRINCIPAL_ROW_HEIGHT, position: "relative" }}
          >
            {visible.map((id, i) => (
              <button
                key={id}
                type="button"
                onClick={() => onSwitch(id)}
                className={`absolute left-0 w-full truncate px-3 text-left font-mono text-xs leading-8 hover:bg-stone-100 ${
                  id === principal ? "bg-stone-200 font-semibold" : ""
                }`}
                style={{ top: (first + i) * PRINCIPAL_ROW_HEIGHT, height: PRINCIPAL_ROW_HEIGHT }}
                data-testid="principal-row"
              >
                {id}
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="rounded-lg border border-stone-200 bg-white p-3" data-testid="scope-panel">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-stone-500">
          What I can see
        </h2>
        {principal === null ? (
          <p className="mt-2 text-sm text-stone-500" data-testid="rail-empty-state">
            Select a demo principal to begin.
          </p>
        ) : scope === null ? (
          <p className="mt-2 text-sm text-stone-400">Loading scope…</p>
        ) : (
          <div className="mt-2 space-y-2 text-xs">
            <ScopeChips label="Groups" values={scope.groups} />
            <ScopeChips label="Sites" values={scope.sites} />
            <div>
              <span className="text-stone-500">Band</span>
              <div className="mt-1">
                {scope.band === null ? (
                  <span className="text-stone-400">none</span>
                ) : (
                  <span className="inline-block rounded border border-stone-300 bg-stone-100 px-1.5 py-0.5 font-mono">
                    {scope.band}
                  </span>
                )}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function ScopeChips({ label, values }: { label: string; values: string[] }) {
  return (
    <div>
      <span className="text-stone-500">{label}</span>
      <div className="mt-1 flex flex-wrap gap-1">
        {values.length === 0 ? (
          <span className="text-stone-400">none</span>
        ) : (
          values.map((value) => (
            <span
              key={value}
              className="inline-block rounded border border-stone-300 bg-stone-100 px-1.5 py-0.5 font-mono"
              data-testid="scope-chip"
            >
              {value}
            </span>
          ))
        )}
      </div>
    </div>
  );
}
