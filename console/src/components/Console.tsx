"use client";

import { useCallback, useEffect, useState } from "react";
import * as api from "@/lib/api";
import type { AnswerEnvelope, DocCard, ScopeStatement } from "@/lib/api";
import { TYPE } from "@/lib/tokens";
import { AnswerCard } from "./AnswerCard";
import { DocInspector } from "./DocInspector";
import { IdentityRail } from "./IdentityRail";
import { LensBar } from "./LensBar";
import { LensRoom } from "./LensRoom";
import { ResultsList } from "./ResultsList";
import { Skeleton } from "./Skeleton";
import iris from "./LensBar.module.css";

/**
 * The Aperture shell: lens bar on top (the navigation primitive), the Ask
 * view beneath. Switching lenses fires the IRIS — a circular clip-path wipe
 * centered on the lens bar — during which the answer/results state clears
 * (the M3b residue rule, now with a face). prefers-reduced-motion swaps the
 * iris for fade-view.
 */
function prefersReducedMotion(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    // No way to ask: choose the calmer path.
    return true;
  }
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export function Console({ view = "ask" }: { view?: "ask" | "lens" }) {
  const [principal, setPrincipal] = useState<string | null>(null);
  const [scope, setScope] = useState<ScopeStatement | null>(null);
  const [query, setQuery] = useState("");
  const [hybrid, setHybrid] = useState(false);
  const [judge, setJudge] = useState(false);
  const [envelope, setEnvelope] = useState<AnswerEnvelope | null>(null);
  const [asking, setAsking] = useState(false);
  const [inspector, setInspector] = useState<{
    open: boolean;
    loading: boolean;
    card: DocCard | null;
  }>({ open: false, loading: false, card: null });

  const switchPrincipal = useCallback((next: string) => {
    setPrincipal(next);
    // Clear EVERYTHING the previous lens saw, before any fetch: the iris
    // reveals a clean world.
    setScope(null);
    setEnvelope(null);
    setInspector({ open: false, loading: false, card: null });
  }, []);

  useEffect(() => {
    if (principal === null) {
      return;
    }
    let cancelled = false;
    api
      .getScope(principal)
      .then((response) => {
        if (!cancelled) {
          setScope(response.scope_statement);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setScope(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [principal]);

  const submitAsk = useCallback(async () => {
    if (principal === null || query.trim().length === 0 || asking) {
      return;
    }
    setAsking(true);
    setEnvelope(null);
    try {
      const response = await api.ask(principal, query, { hybrid, judge });
      setEnvelope(response);
    } catch {
      // Internal errors render as the quiet no-answer state; the service
      // never explains absence and neither does the console.
      setEnvelope(null);
    } finally {
      setAsking(false);
    }
  }, [principal, query, hybrid, judge, asking]);

  const openDoc = useCallback(
    async (docId: string) => {
      if (principal === null) {
        return;
      }
      setInspector({ open: true, loading: true, card: null });
      try {
        const card = await api.getDoc(principal, docId);
        setInspector({ open: true, loading: false, card });
      } catch {
        setInspector({ open: true, loading: false, card: null });
      }
    },
    [principal],
  );

  const irisClass = prefersReducedMotion() ? iris.fadeIn : iris.irisIn;

  return (
    <div className="min-h-screen">
      <LensBar principal={principal} onSwitch={switchPrincipal} />

      <div
        key={principal ?? "no-lens"}
        className={`mx-auto flex max-w-6xl gap-6 p-4 ${irisClass}`}
        data-testid="iris-stage"
      >
        {view === "lens" ? (
          <main className="min-w-0 flex-1">
            <LensRoom actor={principal} />
          </main>
        ) : (
          <>
            <aside className="w-72 shrink-0">
              <IdentityRail principal={principal} scope={scope} />
            </aside>

            <main className="min-w-0 flex-1">
          <header className="mb-3">
            <h1
              className="ap-register-chrome"
              style={{
                fontSize: TYPE.scale.lg,
                lineHeight: TYPE.line.display,
                fontWeight: 600,
              }}
            >
              Ask
            </h1>
            <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
              Scope, provenance, and honest degradation, at a glance.
            </p>
          </header>

          <div className="ap-card rounded p-3">
            <textarea
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={
                principal === null ? "Select a lens first" : "Ask within your scope…"
              }
              disabled={principal === null}
              rows={2}
              className="w-full resize-none rounded px-2 py-1.5"
              style={{ fontSize: TYPE.scale.sm }}
              data-testid="query-input"
            />
            <div className="mt-2 flex items-center gap-4">
              <label
                className="ap-soft flex items-center gap-1.5"
                style={{ fontSize: TYPE.scale.xs }}
              >
                <input
                  type="checkbox"
                  checked={hybrid}
                  onChange={(e) => setHybrid(e.target.checked)}
                  data-testid="toggle-hybrid"
                />
                hybrid
              </label>
              <label
                className="ap-soft flex items-center gap-1.5"
                style={{ fontSize: TYPE.scale.xs }}
              >
                <input
                  type="checkbox"
                  checked={judge}
                  onChange={(e) => setJudge(e.target.checked)}
                  data-testid="toggle-judge"
                />
                judge
              </label>
              <button
                type="button"
                onClick={submitAsk}
                disabled={principal === null || asking}
                className="ap-affordance-button ml-auto rounded px-3 py-1"
                style={{ fontSize: TYPE.scale.xs, fontWeight: 500 }}
                data-testid="ask-button"
              >
                Ask
              </button>
            </div>
          </div>

          <div className="mt-4 space-y-4">
            {asking && (
              <div className="ap-card rounded p-4">
                <Skeleton lines={3} />
              </div>
            )}
            {envelope && (
              <>
                <AnswerCard envelope={envelope} onOpenDoc={openDoc} />
                <section className="ap-card rounded p-2">
                  <h2
                    className="ap-soft px-2 pb-1 pt-1 uppercase tracking-wide"
                    style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
                  >
                    Results
                  </h2>
                  <ResultsList results={envelope.results} onOpenDoc={openDoc} />
                </section>
              </>
            )}
          </div>
            </main>
          </>
        )}
      </div>

      <DocInspector
        open={inspector.open}
        loading={inspector.loading}
        card={inspector.card}
        onClose={() => setInspector({ open: false, loading: false, card: null })}
        onOpenDoc={openDoc}
      />
    </div>
  );
}
