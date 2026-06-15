"use client";

import { useEffect, useState } from "react";
import * as api from "@/lib/api";
import type { GraphResponse } from "@/lib/api";
import { TYPE } from "@/lib/tokens";
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
      <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="graph-room-empty">
        Select a lens to begin.
      </p>
    );
  }

  const quiet = (testid: string) => (
    <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid={testid}>
      No organizational view in your scope.
    </p>
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
