"use client";

import { useEffect, useState } from "react";
import * as api from "@/lib/api";
import type { GraphResponse, NodeSummary, OrgStats } from "@/lib/api";
import { COLOR, DERIVED, FONT, TYPE } from "@/lib/tokens";
import { OrgGraph, type SelectedNode } from "./OrgGraph";
import { GraphSidebar } from "./GraphSidebar";
import { GraphInspector } from "./GraphInspector";
import { Skeleton } from "./Skeleton";

/**
 * Click-to-lens is a CROSS-LENS act: the current actor flies into the clicked
 * principal's lens, audited server-side. The route mirrors the room switcher's
 * — `?as` carries the actor, `?subject` names the target — so the iris fires on
 * /lens exactly as a manual cross-lens does.
 */
export function lensHref(actor: string, subject: string): string {
  return `/lens?as=${encodeURIComponent(actor)}&subject=${encodeURIComponent(subject)}`;
}

/** Node kinds whose governance is summarised server-side (have an artifact). */
const SUMMARISED = new Set(["org", "human", "agent"]);

/**
 * THE ORG BRAIN — the console's entry surface and main UI: a dense, scope-
 * honest concentric-ring company graph as a command centre (left rail of real
 * counts + filters + departments; the graph; a right inspector that reads the
 * compiled governance). Dark by default. A no-standing actor gets a quiet,
 * designed empty state — honest dark, nothing padded.
 */
export function GraphRoom({
  actor,
  reducedMotion = false,
}: {
  actor: string | null;
  reducedMotion?: boolean;
}) {
  const [graph, setGraph] = useState<GraphResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [available, setAvailable] = useState(true);
  const [stats, setStats] = useState<OrgStats | null>(null);

  const [selected, setSelected] = useState<SelectedNode | null>(null);
  const [summary, setSummary] = useState<NodeSummary | null>(null);
  const [summaryLoading, setSummaryLoading] = useState(false);
  const [focusDept, setFocusDept] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [hiddenKinds, setHiddenKinds] = useState<string[]>([]);

  useEffect(() => {
    if (actor === null) {
      setGraph(null);
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
    // The sidebar's counts: the org rollup (real cardinalities).
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

  // The inspector's governance: fetched for principals/org; departments and
  // sources are composed on the client from the graph payload (no endpoint).
  useEffect(() => {
    if (actor === null || selected === null || !SUMMARISED.has(selected.kind)) {
      setSummary(null);
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

  if (actor === null) {
    return (
      <GraphEmpty
        testid="graph-room-empty"
        headline="Select a lens to begin."
        sub="Choose a principal from the bar above to see the company through their lens."
      />
    );
  }

  const quiet = (testid: string) => (
    <GraphEmpty
      testid={testid}
      headline="No organizational view in your scope."
      sub="This lens doesn't include a company graph. Nothing is withheld here — there is simply nothing to draw."
    />
  );

  return (
    <div data-testid="graph-room" className="w-full">
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <h1
          className="ap-register-chrome"
          style={{ fontSize: TYPE.scale.lg, lineHeight: TYPE.line.display, fontWeight: 600 }}
        >
          Graph
        </h1>
        <div className="relative ml-auto">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search people, agents, departments…"
            className="w-64 rounded px-2 py-1"
            style={{ fontSize: TYPE.scale.xs }}
            data-testid="graph-search"
          />
          {query.length > 0 && (
            <button
              type="button"
              onClick={() => setQuery("")}
              className="ap-soft absolute right-1 top-1/2 -translate-y-1/2 px-1"
              style={{ fontSize: TYPE.scale.xs }}
              aria-label="Clear search"
              data-testid="graph-search-clear"
            >
              ×
            </button>
          )}
        </div>
        <ThemeToggle />
      </div>

      {loading && (
        <div className="ap-card rounded p-4">
          <Skeleton lines={6} />
        </div>
      )}

      {!loading && !available && quiet("graph-unavailable")}

      {!loading && graph !== null ? (
        graph.people.length === 0 ? (
          quiet("graph-empty")
        ) : (
          <div className="flex flex-wrap items-start gap-3 lg:flex-nowrap">
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
            <div className="ap-card min-w-0 flex-1 rounded p-1" data-testid="graph-stage">
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
            </div>
            {selected !== null && (
              <GraphInspector
                node={selected}
                summary={summary}
                loading={summaryLoading}
                graph={graph}
                onEnterLens={enterLens}
                onClose={() => setSelected(null)}
              />
            )}
          </div>
        )
      ) : null}
    </div>
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
      /* private mode: the choice just won't persist */
    }
  };
  return (
    <button
      type="button"
      onClick={flip}
      className="ap-card ap-washable ap-register-chrome rounded px-2 py-1"
      style={{ fontSize: TYPE.scale.xs, fontWeight: 500 }}
      data-testid="theme-toggle"
      data-theme={theme}
    >
      {theme === "dark" ? "Light mode" : "Dark mode"}
    </button>
  );
}

/**
 * The designed empty state — a quiet card with a resting org glyph, not a bare
 * sentence. HONEST DARK: it states plainly that nothing is withheld.
 */
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
    <div
      className="ap-card flex flex-col items-center gap-3 rounded px-6 py-12 text-center"
      data-testid={testid}
    >
      <svg width={64} height={64} viewBox="0 0 64 64" aria-hidden="true">
        <circle cx={32} cy={32} r={26} fill="none" stroke={DERIVED.hairline} strokeWidth={1} />
        {satellites.map((a, i) => (
          <line
            key={i}
            x1={32}
            y1={32}
            x2={32 + 26 * Math.cos(a)}
            y2={32 + 26 * Math.sin(a)}
            stroke={DERIVED.hairline}
            strokeWidth={1}
            strokeOpacity={0.6}
          />
        ))}
        {satellites.map((a, i) => (
          <circle
            key={`s${i}`}
            cx={32 + 26 * Math.cos(a)}
            cy={32 + 26 * Math.sin(a)}
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
    </div>
  );
}
