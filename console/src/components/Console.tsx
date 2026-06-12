"use client";

import { useCallback, useEffect, useState } from "react";
import * as api from "@/lib/api";
import type { AnswerEnvelope, DocCard, ScopeStatement } from "@/lib/api";
import { AnswerCard } from "./AnswerCard";
import { DocInspector } from "./DocInspector";
import { IdentityRail } from "./IdentityRail";
import { ResultsList } from "./ResultsList";
import { Skeleton } from "./Skeleton";

/**
 * The console: one screen, three regions. Identity rail (left), ask column
 * (center), doc inspector (side sheet). Switching principals clears the
 * answer view entirely — no cross-principal residue on screen (U-4).
 */
export function Console() {
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
    // Clear EVERYTHING the previous principal saw, before any fetch.
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

  return (
    <div className="mx-auto flex min-h-screen max-w-6xl gap-4 p-4">
      <aside className="w-72 shrink-0">
        <IdentityRail principal={principal} scope={scope} onSwitch={switchPrincipal} />
      </aside>

      <main className="min-w-0 flex-1">
        <header className="mb-3">
          <h1 className="text-base font-semibold text-stone-800">Ask Brain</h1>
          <p className="text-xs text-stone-500">
            Governed retrieval console — scope, provenance, and honest degradation, at a glance.
          </p>
        </header>

        <div className="rounded-lg border border-stone-200 bg-white p-3">
          <textarea
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={
              principal === null ? "Select a principal first" : "Ask within your scope…"
            }
            disabled={principal === null}
            rows={2}
            className="w-full resize-none rounded border border-stone-200 px-2 py-1.5 text-sm"
            data-testid="query-input"
          />
          <div className="mt-2 flex items-center gap-4">
            <label className="flex items-center gap-1.5 text-xs text-stone-600">
              <input
                type="checkbox"
                checked={hybrid}
                onChange={(e) => setHybrid(e.target.checked)}
                data-testid="toggle-hybrid"
              />
              hybrid
            </label>
            <label className="flex items-center gap-1.5 text-xs text-stone-600">
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
              className="ml-auto rounded bg-stone-800 px-3 py-1 text-xs font-medium text-white hover:bg-stone-700 disabled:opacity-40"
              data-testid="ask-button"
            >
              Ask
            </button>
          </div>
        </div>

        <div className="mt-3 space-y-3">
          {asking && (
            <div className="rounded-lg border border-stone-200 bg-white p-4">
              <Skeleton lines={3} />
            </div>
          )}
          {envelope && (
            <>
              <AnswerCard envelope={envelope} onOpenDoc={openDoc} />
              <section className="rounded-lg border border-stone-200 bg-white p-2">
                <h2 className="px-2 pb-1 pt-1 text-xs font-semibold uppercase tracking-wide text-stone-500">
                  Results
                </h2>
                <ResultsList results={envelope.results} onOpenDoc={openDoc} />
              </section>
            </>
          )}
        </div>
      </main>

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
