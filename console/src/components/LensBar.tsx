import { useMemo, useState } from "react";
import { COLOR, DERIVED, TYPE } from "@/lib/tokens";
import {
  PRINCIPAL_LIST_HEIGHT,
  PRINCIPAL_ROW_HEIGHT,
  VIRTUAL_OVERSCAN,
} from "@/lib/constants";
import { PRINCIPALS } from "@/lib/principals";

/**
 * THE LENS BAR — the navigation primitive. Permanent, top-center, every
 * view: the current lens (principal id + kind badge), the searchable
 * switcher over the 124 demo principals (virtualized), and the
 * non-dismissible DEMO IDENTITY MODE caption beneath — furniture, not a
 * toast. Switching lenses fires the iris in the parent (Console).
 */
export function LensBar({
  principal,
  onSwitch,
}: {
  principal: string | null;
  onSwitch: (principal: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [scrollTop, setScrollTop] = useState(0);

  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return needle.length === 0
      ? PRINCIPALS
      : PRINCIPALS.filter((p) => p.toLowerCase().includes(needle));
  }, [search]);

  const listVisible = open || search.trim().length > 0;
  const first = Math.max(0, Math.floor(scrollTop / PRINCIPAL_ROW_HEIGHT) - VIRTUAL_OVERSCAN);
  const visibleCount =
    Math.ceil(PRINCIPAL_LIST_HEIGHT / PRINCIPAL_ROW_HEIGHT) + VIRTUAL_OVERSCAN * 2;
  const visible = filtered.slice(first, first + visibleCount);

  const kind = principal?.startsWith("agent_") ? "agent" : "human";

  const choose = (id: string) => {
    setOpen(false);
    setSearch("");
    setScrollTop(0);
    onSwitch(id);
  };

  return (
    <header className="ap-card border-x-0 border-t-0" data-testid="lens-bar">
      <div className="mx-auto flex max-w-6xl items-center justify-center gap-3 px-4 py-2">
        <span className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
          Work Identity
        </span>
        <button
          type="button"
          onClick={() => setOpen((value) => !value)}
          className="ap-washable flex items-center gap-2 rounded px-2 py-1"
          data-testid="lens-current"
        >
          <span
            className="ap-register-evidence"
            style={{ fontSize: TYPE.scale.sm, fontWeight: 500 }}
          >
            {principal ?? "No Work Identity selected"}
          </span>
          {principal && (
            <span
              className="ap-register-chrome rounded border px-1.5 py-0.5"
              style={{
                fontSize: TYPE.scale.xs,
                borderColor: DERIVED.hairline,
                color: COLOR.inkSoft,
              }}
              data-testid="lens-kind"
            >
              {kind}
            </span>
          )}
        </button>
        <input
          id="principal-search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          onFocus={() => setOpen(true)}
          placeholder="Search demo Work Identities"
          className="w-64 rounded px-2 py-1"
          style={{ fontSize: TYPE.scale.xs }}
          data-testid="principal-search"
        />
      </div>

      {listVisible && (
        <div className="ap-fade-view mx-auto max-w-6xl px-4 pb-2">
          <div
            className="ap-card overflow-y-auto rounded"
            style={{ height: PRINCIPAL_LIST_HEIGHT }}
            onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
            data-testid="principal-list"
          >
            <div style={{ height: filtered.length * PRINCIPAL_ROW_HEIGHT, position: "relative" }}>
              {visible.map((id, i) => (
                <button
                  key={id}
                  type="button"
                  onClick={() => choose(id)}
                  className="ap-washable ap-register-evidence absolute left-0 w-full truncate px-3 text-left"
                  style={{
                    top: (first + i) * PRINCIPAL_ROW_HEIGHT,
                    height: PRINCIPAL_ROW_HEIGHT,
                    lineHeight: `${PRINCIPAL_ROW_HEIGHT}px`,
                    fontSize: TYPE.scale.xs,
                    backgroundColor: id === principal ? DERIVED.wash : undefined,
                    fontWeight: id === principal ? 500 : 400,
                  }}
                  data-testid="principal-row"
                >
                  {id}
                </button>
              ))}
            </div>
          </div>
        </div>
      )}

      <div
        className="mx-auto max-w-6xl px-4 pb-1.5 text-center"
        style={{ fontSize: TYPE.scale.xs, color: COLOR.inkSoft }}
        data-testid="demo-banner"
      >
        Demo Identity Mode: Work Identity selection previews permission boundaries in this local pilot workspace. Production identity is not connected.
      </div>
    </header>
  );
}
