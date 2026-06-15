"use client";

import { useEffect, useState } from "react";
import * as api from "@/lib/api";
import type { GraphResponse } from "@/lib/api";
import { COLOR, DERIVED, FONT, TYPE } from "@/lib/tokens";
import { OrgGraph } from "./OrgGraph";
import { Skeleton } from "./Skeleton";

/**
 * Click-to-lens is a CROSS-LENS act: the current actor flies into the clicked
 * principal's lens, audited server-side (the audited-view line shows there).
 * The route is the same the room switcher uses — `?as` carries the actor,
 * `?subject` names the target — so the iris fires on the /lens page exactly as
 * a manual cross-lens does.
 */
export function lensHref(actor: string, subject: string): string {
  return `/lens?as=${encodeURIComponent(actor)}&subject=${encodeURIComponent(subject)}`;
}

/**
 * THE ORG GRAPH ROOM (AR-2) — the entry surface. Fetches the scope-honest
 * /graph for the acting lens and renders it. A no-standing actor (or a world
 * with no humanization layer) gets a quiet "No organizational view in your
 * scope" — not an error, not a teaser. Honest dark: a small permitted world is
 * a small graph; nothing is padded.
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

  useEffect(() => {
    if (actor === null) {
      setGraph(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setGraph(null);
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
        if (!cancelled) {
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [actor]);

  const selectPerson = (id: string) => {
    if (actor === null) {
      return;
    }
    // Navigate into the clicked principal's lens (cross-lens, audited).
    window.location.href = lensHref(actor, id);
  };

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
    <div data-testid="graph-room">
      <header className="mb-3">
        <h1
          className="ap-register-chrome"
          style={{ fontSize: TYPE.scale.lg, lineHeight: TYPE.line.display, fontWeight: 600 }}
        >
          Graph
        </h1>
        <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
          The company through your lens — every node backed by a compiled permission. Click a
          person to fly into their lens.
        </p>
      </header>

      {loading && (
        <div className="ap-card rounded p-4">
          <Skeleton lines={5} />
        </div>
      )}

      {!loading && !available && quiet("graph-unavailable")}

      {!loading && graph !== null
        ? graph.people.length === 0
          ? quiet("graph-empty")
          : (
            <div className="ap-card rounded p-2">
              <OrgGraph graph={graph} onSelectPerson={selectPerson} reducedMotion={reducedMotion} />
            </div>
          )
        : null}
    </div>
  );
}

/**
 * The designed empty state — a quiet card with a resting org glyph, not a bare
 * sentence. HONEST DARK: it states plainly that nothing is withheld; a small
 * (or absent) permitted world simply has little to draw.
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
  // A faint org mark at rest: a ring, a weighted center, three dim satellites.
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
