/**
 * SPEND LEDGER GOES LIVE — T-L1..T-L6 (T-L7 lives in the standing
 * comprehension guards). The producer is never run here: every test mocks
 * fetch. The laws under test are the contract seam's honesty laws — typed
 * mirror completeness, the schema gate, the money law, the delta law, the
 * three honest states, and the read-only no-credentials seam.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { getLedgerSummary, LEDGER_SCHEMA_VERSION } from "@/lib/ledger";
import type { LedgerSummary } from "@/lib/ledger";
import { LEDGER_URL } from "@/lib/constants";
import { BursarSurface } from "@/components/BursarSurface";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

/** T-L1 fixture: EVERY contract field exercised — all 13 denial reasons,
 * drift flags, numeric AND null usd, delta block populated. */
const FULL_FIXTURE: LedgerSummary = {
  schema_version: "ledger.v1.1",
  generated_ordinal: 41,
  mode: "observe_only",
  pricing_verified: true,
  window: { label: "7-day", calls: 21, first_ordinal: 3, last_ordinal: 41 },
  baseline: {
    total_usd: 12.34,
    by_model: [
      {
        model: "model-priced",
        calls: 12,
        input_tokens: 1000,
        output_tokens: 500,
        cache_read_tokens: 64,
        cache_write_tokens: 32,
        usd: 12.34,
      },
      {
        model: "model-unpriced",
        calls: 9,
        input_tokens: 900,
        output_tokens: 450,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        usd: null,
      },
    ],
    skipped_unpriced: [
      { model: "model-unpriced", calls: 9, reason: "unknown_model" },
      { model: "model-stale", calls: 1, reason: "unverified_price" },
    ],
  },
  governance: {
    envelopes_issued: 30,
    calls_authorized: 21,
    calls_denied: 13,
    denials_by_reason: [
      { reason: "over_budget", count: 1 },
      { reason: "model_not_allowed", count: 1 },
      { reason: "effort_exceeded", count: 1 },
      { reason: "tokens_exceeded", count: 1 },
      { reason: "expired", count: 1 },
      { reason: "tampered", count: 1 },
      { reason: "no_envelope", count: 1 },
      { reason: "clock_skew", count: 1 },
      { reason: "stale_snapshot", count: 1 },
      { reason: "envelope_reused", count: 1 },
      { reason: "task_class_faulted", count: 1 },
      { reason: "store_unavailable", count: 1 },
      { reason: "unmapped", count: 1 },
    ],
    effort_ceiling_applied: 4,
    drift_flags: [{ task_class: "code_review", mean_variance_pct: 18.5, calls: 6 }],
    scope: "window",
  },
  delta: {
    available: true,
    baseline_usd: 12.34,
    governed_usd: 9.87,
    savings_pct: 20.0,
    note: "delta computed: measured enforce-mode window vs verified-priced baseline",
  },
};

function stubLedgerFetch(response: () => Promise<Response>) {
  const spy = vi.fn((_input: RequestInfo | URL, _init?: RequestInit) => response());
  vi.stubGlobal("fetch", spy);
  return spy;
}

const okResponse = (body: unknown) => async () =>
  new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } });

describe("T-L1: typed mirror completeness (ignore-don't-drop)", () => {
  it("a payload exercising every field parses with no field dropped", async () => {
    stubLedgerFetch(okResponse(FULL_FIXTURE));
    const result = await getLedgerSummary();
    expect(result.state).toBe("ok");
    if (result.state !== "ok") throw new Error("unreachable");
    // Deep equality against the full fixture: nothing dropped, nothing coerced.
    expect(result.data).toEqual(FULL_FIXTURE);
    expect(result.data.governance.denials_by_reason).toHaveLength(13);
    expect(result.data.governance.drift_flags).toHaveLength(1);
    expect(result.data.governance.scope).toBe("window");
  });
});

describe("T-L2: schema gate", () => {
  it("a version-mismatched producer is an unavailable producer", async () => {
    stubLedgerFetch(okResponse({ ...FULL_FIXTURE, schema_version: "ledger.v2" }));
    const result = await getLedgerSummary();
    expect(result.state).toBe("unavailable");
    if (result.state !== "unavailable") throw new Error("unreachable");
    expect(result.detail).toContain("ledger.v2");
    expect(result.detail).toContain(LEDGER_SCHEMA_VERSION);
  });
});

describe("T-L3: money law", () => {
  it("pricing_verified=false renders zero $ glyphs and the withheld line", async () => {
    stubLedgerFetch(
      okResponse({
        ...FULL_FIXTURE,
        pricing_verified: false,
        delta: { available: false, baseline_usd: null, governed_usd: null, savings_pct: null, note: "delta withheld: pricing is not owner-verified" },
      }),
    );
    const { container } = render(<BursarSurface />);
    await screen.findByTestId("bursar-live-report");
    expect(screen.getByTestId("bursar-pricing-withheld").textContent).toBe(
      "Pricing unverified — figures withheld.",
    );
    expect(container.textContent ?? "").not.toContain("$");
  });

  it("usd=null with verified pricing renders an em dash, never $0.00", async () => {
    stubLedgerFetch(
      okResponse({
        ...FULL_FIXTURE,
        baseline: { ...FULL_FIXTURE.baseline, total_usd: null },
        delta: { available: false, baseline_usd: null, governed_usd: null, savings_pct: null, note: "delta withheld: no verified-priced baseline spend in the window" },
      }),
    );
    const { container } = render(<BursarSurface />);
    await screen.findByTestId("bursar-live-report");
    expect(screen.getByTestId("bursar-total-usd").textContent).toBe("—");
    expect(container.textContent ?? "").not.toContain("$0.00");
    // The null is discoverable: the not-priced section lists the models.
    expect(screen.getByTestId("bursar-skipped").textContent).toContain(
      "Not priced (listed, never guessed)",
    );
  });
});

describe("T-L4: delta law", () => {
  it("available=false renders the producer's note verbatim and no savings number", async () => {
    const note = "delta withheld: no enforce-mode window has been measured against the baseline";
    stubLedgerFetch(
      okResponse({
        ...FULL_FIXTURE,
        delta: { available: false, baseline_usd: null, governed_usd: null, savings_pct: null, note },
      }),
    );
    render(<BursarSurface />);
    await screen.findByTestId("bursar-live-report");
    expect(screen.queryByTestId("bursar-delta-block")).toBeNull();
    expect(screen.getByTestId("bursar-delta-note").textContent).toBe(note);
    expect(screen.getByTestId("bursar-delta").textContent).not.toMatch(/savings/i);
  });

  it("available=true renders the delta block", async () => {
    stubLedgerFetch(okResponse(FULL_FIXTURE));
    render(<BursarSurface />);
    await screen.findByTestId("bursar-live-report");
    const block = screen.getByTestId("bursar-delta-block");
    expect(block.textContent).toContain("$12.34");
    expect(block.textContent).toContain("$9.87");
    expect(block.textContent).toContain("20%");
  });
});

describe("T-L5: three honest states", () => {
  it("STATE 1 — live: mode, scope, window, governance render from the payload", async () => {
    stubLedgerFetch(okResponse(FULL_FIXTURE));
    render(<BursarSurface />);
    await screen.findByTestId("bursar-live-report");
    expect(screen.getByTestId("bursar-mode-chip").textContent).toBe("observe_only");
    expect(screen.getByTestId("bursar-scope-chip").textContent).toBe("window counters");
    expect(screen.getByTestId("bursar-window-line").textContent).toContain(
      "7-day window · 21 calls · ordinals 3 → 41",
    );
    expect(screen.getAllByTestId("bursar-denial-row")).toHaveLength(13);
    expect(screen.getByTestId("bursar-governance").textContent).toContain("Envelopes issued");
  });

  it("STATE 2 — live-empty: zeros render as zeros, not as the unavailable state", async () => {
    stubLedgerFetch(
      okResponse({
        ...FULL_FIXTURE,
        window: { label: "7-day", calls: 0, first_ordinal: 0, last_ordinal: 0 },
        baseline: { total_usd: 0, by_model: [], skipped_unpriced: [] },
        governance: { ...FULL_FIXTURE.governance, envelopes_issued: 0, calls_authorized: 0, calls_denied: 0, denials_by_reason: [], effort_ceiling_applied: 0, drift_flags: [] },
        delta: { available: false, baseline_usd: null, governed_usd: null, savings_pct: null, note: "delta withheld: no verified-priced baseline spend in the window" },
      }),
    );
    render(<BursarSurface />);
    await screen.findByTestId("bursar-live-report");
    expect(screen.queryByTestId("bursar-unavailable")).toBeNull();
    expect(screen.getByTestId("bursar-window-line").textContent).toContain("0 calls");
    expect(screen.getByTestId("bursar-live-report").textContent).toContain(
      "Honest zeros are data, not an error.",
    );
  });

  it("STATE 3 — network reject, timeout, and 503 all land unavailable with detail", async () => {
    // Network reject.
    stubLedgerFetch(async () => {
      throw new TypeError("fetch failed");
    });
    expect(await getLedgerSummary()).toEqual({
      state: "unavailable",
      detail: "the spend producer did not answer on its loopback port",
    });

    // Timeout (abort).
    stubLedgerFetch(async () => {
      throw new DOMException("aborted", "AbortError");
    });
    const timedOut = await getLedgerSummary();
    expect(timedOut.state).toBe("unavailable");
    if (timedOut.state !== "unavailable") throw new Error("unreachable");
    expect(timedOut.detail).toContain("no response within");

    // Producer's own fail-closed 503.
    stubLedgerFetch(async () =>
      new Response(JSON.stringify({ error: "ledger_unavailable" }), { status: 503 }),
    );
    expect(await getLedgerSummary()).toEqual({
      state: "unavailable",
      detail: "producer answered HTTP 503",
    });
  });

  it("STATE 3 renders the calm card with detail and a single-shot Retry", async () => {
    const spy = stubLedgerFetch(async () =>
      new Response(JSON.stringify({ error: "ledger_unavailable" }), { status: 503 }),
    );
    render(<BursarSurface />);
    await screen.findByTestId("bursar-unavailable");
    expect(screen.getByTestId("bursar-unavailable-detail").textContent).toBe(
      "producer answered HTTP 503",
    );
    expect(spy).toHaveBeenCalledTimes(1); // no auto-retry, no polling
    fireEvent.click(screen.getByTestId("bursar-retry"));
    await screen.findByTestId("bursar-unavailable");
    expect(spy).toHaveBeenCalledTimes(2); // exactly one re-fetch per click
  });
});

describe("T-L6: read-only, credential-free seam", () => {
  it("issues a single GET to LEDGER_URL with no session/Authorization headers", async () => {
    const spy = stubLedgerFetch(okResponse(FULL_FIXTURE));
    await getLedgerSummary();
    expect(spy).toHaveBeenCalledTimes(1);
    const [input, init] = spy.mock.calls[0] as [RequestInfo | URL, RequestInit | undefined];
    expect(String(input)).toBe(`${LEDGER_URL}/report/summary`);
    // No method override (plain GET), no headers object at all — the EB
    // session token never leaves the console -> :8787 seam.
    expect(init?.method).toBeUndefined();
    expect(init?.headers).toBeUndefined();
    expect(init?.credentials).toBeUndefined();
    expect(Object.keys(init ?? {})).toEqual(["signal"]);
  });
});
