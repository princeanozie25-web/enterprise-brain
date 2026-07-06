// The ledger.v1.1 consumer seam. CONTRACT, NEVER IMPORT: this type is retyped
// by hand from the producer's committed contract; zero code crosses repos.
//
// Read-only by construction: the one function here issues a plain GET to the
// loopback producer with no credentials and no custom headers — the console's
// session token belongs to the console→service seam and never leaves it.
// Fail closed: network error, timeout, non-200, unparseable body, or an
// unknown schema_version all collapse to { state: "unavailable" } — a
// version-mismatched producer is an unavailable producer, and refusing to
// render an unknown schema IS the fail-closed behavior. The room renders the
// honest unavailable state; it never fabricates, caches, or projects numbers.

import { LEDGER_URL } from "./constants";

export const LEDGER_SCHEMA_VERSION = "ledger.v1.1";

export type DenialReason =
  | "over_budget"
  | "model_not_allowed"
  | "effort_exceeded"
  | "tokens_exceeded"
  | "expired"
  | "tampered"
  | "no_envelope"
  | "clock_skew"
  | "stale_snapshot"
  | "envelope_reused"
  | "task_class_faulted"
  | "store_unavailable"
  | "unmapped";

export type LedgerWindowLabel = "24-hour" | "7-day" | "30-day";

export type LedgerByModel = {
  model: string;
  calls: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  /** null = the producer cannot price this honestly. NEVER render as $0. */
  usd: number | null;
};

export type LedgerSkipped = {
  model: string;
  calls: number;
  reason: "unverified_price" | "unknown_model";
};

/** Full mirror of the producer contract (ignore-don't-drop): the narrative
 * bridge renders a subset, but the type carries everything so the future full
 * room re-renders, never re-plumbs. */
export type LedgerSummary = {
  schema_version: typeof LEDGER_SCHEMA_VERSION;
  generated_ordinal: number;
  mode: "observe_only" | "enforce";
  pricing_verified: boolean;
  window: {
    label: LedgerWindowLabel;
    calls: number;
    first_ordinal: number;
    last_ordinal: number;
  };
  baseline: {
    total_usd: number | null;
    by_model: LedgerByModel[];
    skipped_unpriced: LedgerSkipped[];
  };
  governance: {
    envelopes_issued: number;
    calls_authorized: number;
    calls_denied: number;
    denials_by_reason: Array<{ reason: DenialReason; count: number }>;
    effort_ceiling_applied: number;
    drift_flags: Array<{ task_class: string; mean_variance_pct: number; calls: number }>;
    scope: "all_time" | "window";
  };
  delta: {
    available: boolean;
    baseline_usd: number | null;
    governed_usd: number | null;
    savings_pct: number | null;
    /** Producer-defined; rendered verbatim. Never compute savings client-side. */
    note: string;
  };
};

export type LedgerResult =
  | { state: "ok"; data: LedgerSummary }
  | { state: "unavailable"; detail: string };

const FETCH_TIMEOUT_MS = 3000;

/** One read of the spend producer. GET only; no session headers, ever. */
export async function getLedgerSummary(): Promise<LedgerResult> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), FETCH_TIMEOUT_MS);
  let response: Response;
  try {
    response = await fetch(`${LEDGER_URL}/report/summary`, { signal: controller.signal });
  } catch (error) {
    clearTimeout(timer);
    const aborted = error instanceof DOMException && error.name === "AbortError";
    return {
      state: "unavailable",
      detail: aborted
        ? `no response within ${FETCH_TIMEOUT_MS}ms`
        : "the spend producer did not answer on its loopback port",
    };
  }
  clearTimeout(timer);

  if (!response.ok) {
    return { state: "unavailable", detail: `producer answered HTTP ${response.status}` };
  }

  let body: unknown;
  try {
    body = await response.json();
  } catch {
    return { state: "unavailable", detail: "producer response was not parseable JSON" };
  }

  const version = (body as { schema_version?: unknown })?.schema_version;
  if (version !== LEDGER_SCHEMA_VERSION) {
    return {
      state: "unavailable",
      detail: `producer speaks ${typeof version === "string" ? version : "an unknown schema"}, not ${LEDGER_SCHEMA_VERSION}`,
    };
  }

  return { state: "ok", data: body as LedgerSummary };
}
