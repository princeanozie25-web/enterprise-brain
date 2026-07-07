/**
 * K3 ROBUSTNESS — the typed-fetch seam, session expiry, error boundaries,
 * and the decomposition file-length + freeze laws. Fully offline.
 */
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

import {
  serviceRequest,
  parseJson,
  isRequestError,
  ServiceRequestError,
  ASK_TIMEOUT_MS,
  DEFAULT_TIMEOUT_MS,
} from "@/lib/request";
import * as session from "@/lib/session";
import * as api from "@/lib/api";
import { RoomBoundary } from "@/components/RoomBoundary";
import { ProductHome } from "@/components/ProductHome";

const SRC = path.resolve(__dirname, "..", "src");
const read = (p: string) => fs.readFileSync(p, "utf8");

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
  cleanup();
  try {
    sessionStorage.clear();
  } catch {
    /* jsdom */
  }
});

// ---------------------------------------------------------------------------
// K3-1 THE REQUEST CORE — timeout budgets, taxonomy branches
// ---------------------------------------------------------------------------

describe("K3-1: the request core aborts at the named budgets and produces each taxonomy branch", () => {
  it("POST /ask uses the 20s budget; everything else uses 8s", async () => {
    const budgets: number[] = [];
    const realSetTimeout = globalThis.setTimeout;
    vi.stubGlobal("setTimeout", ((fn: TimerHandler, ms?: number, ...rest: unknown[]) => {
      budgets.push(ms ?? 0);
      return realSetTimeout(fn as () => void, ms, ...(rest as []));
    }) as unknown as typeof setTimeout);
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("{}", { status: 200 })),
    );

    await serviceRequest("http://svc", "/ask", { method: "POST" });
    await serviceRequest("http://svc", "/graph", {});
    await serviceRequest("http://svc", "/ask", {}); // GET /ask is NOT the generation path

    expect(budgets[0]).toBe(ASK_TIMEOUT_MS);
    expect(ASK_TIMEOUT_MS).toBe(20_000);
    expect(budgets[1]).toBe(DEFAULT_TIMEOUT_MS);
    expect(DEFAULT_TIMEOUT_MS).toBe(8_000);
    expect(budgets[2]).toBe(DEFAULT_TIMEOUT_MS);
  });

  it("an abort surfaces as { kind: 'timeout' }", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((_url: string, init: RequestInit) => {
        return new Promise((_resolve, reject) => {
          init.signal?.addEventListener("abort", () =>
            reject(new DOMException("aborted", "AbortError")),
          );
        });
      }),
    );
    vi.useFakeTimers();
    const call = serviceRequest("http://svc", "/graph", {});
    // Attach the rejection expectation BEFORE advancing timers so the
    // rejection is never transiently unhandled (no vitest unhandled-error
    // warning). `expect(...).rejects` installs the handler eagerly.
    const settled = expect(call).rejects.toMatchObject({ error: { kind: "timeout" } });
    await vi.advanceTimersByTimeAsync(DEFAULT_TIMEOUT_MS + 1);
    await settled;
  });

  it("a fetch rejection is { kind: 'network' }", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new TypeError("Failed to fetch");
      }),
    );
    await expect(serviceRequest("http://svc", "/graph", {})).rejects.toMatchObject({
      error: { kind: "network" },
    });
  });

  it("a 401 is { kind: 'unauthorized' } regardless of route", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("nope", { status: 401 })),
    );
    await expect(serviceRequest("http://svc", "/graph", {})).rejects.toMatchObject({
      error: { kind: "unauthorized" },
    });
  });

  it("a non-2xx the caller did not pre-handle is { kind: 'service', status, body }", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("boom", { status: 500 })),
    );
    const response = await serviceRequest("http://svc", "/graph", {});
    await expect(parseJson(response)).rejects.toMatchObject({
      error: { kind: "service", status: 500, body: "boom" },
    });
  });

  it("an unparseable 2xx body is { kind: 'invalid_body' }", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("not json{", { status: 200 })),
    );
    const response = await serviceRequest("http://svc", "/graph", {});
    await expect(parseJson(response)).rejects.toMatchObject({ error: { kind: "invalid_body" } });
  });

  it("no retries: fetch is called exactly once per request", async () => {
    const fetchMock = vi.fn(async () => new Response("boom", { status: 500 }));
    vi.stubGlobal("fetch", fetchMock);
    await serviceRequest("http://svc", "/graph", {});
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});

// ---------------------------------------------------------------------------
// K3-2 COMPLETENESS — no bare fetch() to the service outside request.ts
// ---------------------------------------------------------------------------

describe("K3-2: every service call routes through request.ts (no bare fetch to SERVICE_URL)", () => {
  function tsFiles(dir: string): string[] {
    const out: string[] = [];
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) out.push(...tsFiles(full));
      else if (/\.tsx?$/.test(entry.name)) out.push(full);
    }
    return out;
  }

  it("only request.ts calls fetch() with the service base; ledger.ts uses its own", () => {
    const offenders: string[] = [];
    for (const file of tsFiles(SRC)) {
      const rel = file.replace(SRC, "src").replace(/\\/g, "/");
      const source = read(file);
      // Any fetch() that references the service (SERVICE_URL or the loopback
      // literal) outside the seam is a leak.
      const usesFetch = /\bfetch\s*\(/.test(source);
      if (!usesFetch) continue;
      // Catch the literal constant, the loopback IP, AND the localhost
      // equivalent — a leak that hardcodes any form is flagged.
      const touchesService = /SERVICE_URL|127\.0\.0\.1:8787|localhost:8787/.test(source);
      if (touchesService && !rel.endsWith("lib/request.ts")) {
        offenders.push(rel);
      }
    }
    expect(offenders).toEqual([]);
  });

  it("api.ts holds no bare fetch( at all — every call goes through the seam wrapper", () => {
    expect(read(path.join(SRC, "lib", "api.ts"))).not.toMatch(/\bfetch\s*\(/);
  });
});

// ---------------------------------------------------------------------------
// K3-7 LEDGER SEAM UNTOUCHED — hash-pinned, and never on the request seam
// ---------------------------------------------------------------------------

describe("K3-7: ledger.ts is a different trust boundary and stays byte-identical", () => {
  // The ledger seam is deliberately NOT unified; this hash is the freeze. If a
  // legitimate ledger change lands later, update this literal in that commit.
  const EXPECTED_SHA256 = "53d025884a117994bfab945d629340cbe55076ea529090e49cee413224e25142";

  it("ledger.ts still uses its own fetch, never SERVICE_URL, never a session header", () => {
    const source = read(path.join(SRC, "lib", "ledger.ts"));
    expect(source).toMatch(/LEDGER_URL/);
    expect(source).not.toMatch(/SERVICE_URL/);
    expect(source).not.toMatch(/authHeader|Authorization|Bearer/i);
    // Its 3s single-shot AbortController discipline is intact.
    expect(source).toMatch(/AbortController/);
    expect(source).not.toMatch(/from "\.\/request"/);
  });

  it("request.ts states, in a top comment, that ledger is excluded", () => {
    const source = read(path.join(SRC, "lib", "request.ts"));
    expect(source).toMatch(/LEDGER|ledger\.ts/);
    expect(source).toMatch(/trust boundary/i);
  });

  it("ledger.ts content is frozen (hash pin)", () => {
    const source = read(path.join(SRC, "lib", "ledger.ts"));
    const sha = crypto.createHash("sha256").update(source, "utf8").digest("hex");
    expect(sha).toBe(EXPECTED_SHA256);
  });
});

// ---------------------------------------------------------------------------
// K3-3 SESSION EXPIRY — 401 on any call → picker + calm line + restore
// ---------------------------------------------------------------------------

describe("K3-3: a 401 fires the expiry notifier once; the picker restores the return intent", () => {
  it("api.request notifies expiry exactly once, even across concurrent 401s (the idempotency guard)", async () => {
    // Seed an active session so notifyExpiry has something to clear/fire on.
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url.endsWith("/auth/login"))
          return new Response(
            JSON.stringify({ principal_id: "p060", session_token: "t", expires_at: 0 }),
            { status: 200 },
          );
        return new Response("nope", { status: 401 });
      }),
    );
    await session.loginAs("p060");
    let fired = 0;
    const unsub = session.onSessionExpired(() => (fired += 1));
    // TWO concurrent 401s from the same (now-expired) session. The first
    // clears the session; the second must NOT re-fire (wasActive === false).
    await Promise.allSettled([api.getGraph("p060"), api.getScope("p060")]);
    expect(fired).toBe(1);
    expect(session.sessionPrincipal()).toBeNull();
    unsub();
  });

  it("a stale 401 from a superseded token does NOT destroy a freshly-minted session", () => {
    // A newer valid session is active; a late 401 attributed to the OLD token
    // must be ignored (no spurious expiry, no wipe of the fresh session).
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        if (String(input).endsWith("/auth/login"))
          return new Response(
            JSON.stringify({ principal_id: "p088", session_token: "fresh", expires_at: 0 }),
            { status: 200 },
          );
        return new Response("nope", { status: 401 });
      }),
    );
    return (async () => {
      await session.loginAs("p088"); // active token = "fresh"
      let fired = 0;
      const unsub = session.onSessionExpired(() => (fired += 1));
      session.notifyExpiry("stale-old-token"); // a 401 from a superseded token
      expect(fired).toBe(0);
      expect(session.sessionPrincipal()).toBe("p088"); // fresh session survived
      // The genuine case still fires: a 401 for the current token expires it.
      session.notifyExpiry("fresh");
      expect(fired).toBe(1);
      expect(session.sessionPrincipal()).toBeNull();
      unsub();
    })();
  });

  it("the picker shows the calm line + aria-live and restores room + staged query for the SAME identity only", () => {
    session.captureReturnIntent({
      path: "/ask",
      query: "confidential financial statements",
      principal: "p060",
    });
    window.history.replaceState(null, "", "/?expired=1");
    render(<ProductHome />);
    const line = screen.getByTestId("session-expired-line");
    expect(line.textContent).toBe("Your session ended. Pick who you are to continue.");
    expect(line.getAttribute("aria-live")).toBe("polite");
    // Re-picking the SAME identity restores the room + staged (unsubmitted)
    // query — staged via the ?q= door, never auto-submitted.
    expect(screen.getByTestId("identity-option-p060").getAttribute("href")).toBe(
      "/ask?as=p060&q=confidential%20financial%20statements",
    );
    // A DIFFERENT identity does NOT inherit p060's staged query — it starts
    // fresh at Home (the query is p060's content, not p088's / p_void's).
    expect(screen.getByTestId("identity-option-p088").getAttribute("href")).toBe("/me?as=p088");
    expect(screen.getByTestId("identity-option-p_void").getAttribute("href")).toBe("/me?as=p_void");
    window.history.replaceState(null, "", "/");
  });

  it("with no expiry the picker is unchanged: plain /me hrefs, empty status line", () => {
    render(<ProductHome />);
    expect(screen.getByTestId("session-expired-line").textContent).toBe("");
    expect(screen.getByTestId("identity-option-p060").getAttribute("href")).toBe("/me?as=p060");
  });
});

// ---------------------------------------------------------------------------
// K3-4 ERROR BOUNDARIES — a room crash is a calm card; shell + siblings alive
// ---------------------------------------------------------------------------

function Bomb({ armed }: { armed: boolean }): React.ReactElement {
  if (armed) throw new Error("kaboom");
  return <div data-testid="bomb-ok">room content</div>;
}

describe("K3-4: a thrown render error becomes a calm boundary card that reloads the room", () => {
  it("catches the throw, shows the neutral card (no red), and remounts on reload", () => {
    let armed = true;
    function Harness() {
      return (
        <div>
          <nav data-testid="shell-nav">nav</nav>
          <RoomBoundary>
            <Bomb armed={armed} />
          </RoomBoundary>
        </div>
      );
    }
    const { rerender } = render(<Harness />);
    // The shell survives; the boundary card is shown.
    expect(screen.getByTestId("shell-nav")).toBeTruthy();
    const card = screen.getByTestId("room-boundary-card");
    expect(card.textContent).toContain("This room hit an error. The rest of the console is unaffected.");
    // Color law: no red anywhere in the card OR ANY descendant — inline
    // style and class tokens both (jsdom can't resolve var()-driven class
    // colors, but RoomBoundary uses only neutral ap-* classes + --hairline).
    for (const el of [card, ...Array.from(card.querySelectorAll("*"))]) {
      expect(el.getAttribute("style") ?? "").not.toMatch(/red|#f00|#ff0000|crimson|#dc2626/i);
      expect(el.className.toString()).not.toMatch(/\b(red|danger|destructive|error-)\b/);
    }
    // Fix the underlying cause, then reload the room → it remounts clean.
    armed = false;
    rerender(<Harness />);
    fireEvent.click(screen.getByTestId("room-boundary-reload"));
    expect(screen.getByTestId("bomb-ok")).toBeTruthy();
    expect(screen.queryByTestId("room-boundary-card")).toBeNull();
  });

  it("the boundary reports to no network endpoint (no fetch on catch)", () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    render(
      <RoomBoundary>
        <Bomb armed />
      </RoomBoundary>,
    );
    expect(screen.getByTestId("room-boundary-card")).toBeTruthy();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("Console places RoomBoundary AROUND the room body, AFTER the shell (the real placement, not a harness)", () => {
    // Source-structure pin: the shell (LensBar) renders before <RoomBoundary>,
    // which wraps only the #main room ternary — so a room crash keeps the
    // shell. A future refactor that hoisted the boundary above the shell would
    // trip this even though the synthetic Bomb test above would stay green.
    const console_ = read(path.join(SRC, "components", "Console.tsx"));
    const lensBarAt = console_.indexOf("<LensBar");
    const mainOpenAt = console_.indexOf('id="main"');
    const boundaryOpenAt = console_.indexOf("<RoomBoundary>");
    const boundaryCloseAt = console_.indexOf("</RoomBoundary>");
    expect(lensBarAt).toBeGreaterThan(-1);
    expect(boundaryOpenAt).toBeGreaterThan(-1);
    // Shell first, then the room boundary.
    expect(lensBarAt).toBeLessThan(boundaryOpenAt);
    // The boundary opens inside #main and wraps the room ternary (the first
    // arm opens right after <RoomBoundary>, the last arm closes before it).
    expect(mainOpenAt).toBeLessThan(boundaryOpenAt);
    expect(boundaryOpenAt).toBeLessThan(console_.indexOf('view === "adminGraph" ? ('));
    expect(boundaryCloseAt).toBeGreaterThan(console_.indexOf('view === "project" ? ('));
    // Console imports the shared boundary (not an inline reimplementation).
    expect(console_).toMatch(/import\s*\{\s*RoomBoundary\s*\}\s*from\s*"\.\/RoomBoundary"/);
  });
});

// ---------------------------------------------------------------------------
// K3-5 FILE-LENGTH LAW — dashboard shell ≤600, every section ≤450
// ---------------------------------------------------------------------------

describe("K3-5: the dashboard shell and every extracted section obey the length law", () => {
  const lineCount = (p: string) => read(p).split("\n").length;

  it("EmployeeDashboard.tsx is a shell ≤600 lines", () => {
    const lines = lineCount(path.join(SRC, "components", "EmployeeDashboard.tsx"));
    expect(lines, `EmployeeDashboard.tsx is ${lines} lines`).toBeLessThanOrEqual(600);
  });

  it("every console/src/components/dashboard/*.tsx section is ≤450 lines", () => {
    const dir = path.join(SRC, "components", "dashboard");
    expect(fs.existsSync(dir), "dashboard/ directory exists").toBe(true);
    const offenders: string[] = [];
    for (const name of fs.readdirSync(dir)) {
      if (!name.endsWith(".tsx") && !name.endsWith(".ts")) continue;
      const lines = lineCount(path.join(dir, name));
      if (lines > 450) offenders.push(`${name}: ${lines}`);
    }
    expect(offenders).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// K3-6 DASHBOARD HEADING/LANDMARK ORDER — pinned pre/post (freeze)
// ---------------------------------------------------------------------------

describe("K3-6: the dashboard's loaded heading order is exactly one h1 (the user name)", () => {
  const GRAPH = { center: { id: "org", label: "Org" }, departments: [], people: [], tools: [], sources: [], projects: [], edges: [], actor_id: "p060", snapshot_version: "s" };
  const HUMAN = {
    avatar_ref: "",
    bio: "",
    department_label: "Finance",
    display_name: "Felix Osei",
    id: "p060",
    location: "site_keldonbury",
    manages: [] as string[],
    personality_tag: "",
    projects: [] as never[],
    reports_to: null,
    seniority: "head",
    title: "Head of Finance",
    work_style: "",
  };
  const LENS = {
    actor: "p060",
    subject: { id: "p060", name: "Felix Osei", department: "Finance", kind: "human" },
    subject_human: HUMAN,
    holdings: [],
    demo_identity_mode: true,
    snapshot_version: "s",
  };

  it("loaded /me renders exactly one h1 and no skipped levels", async () => {
    const { EmployeeDashboard } = await import("@/components/EmployeeDashboard");
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url.endsWith("/graph")) return new Response(JSON.stringify(GRAPH), { status: 200 });
        if (url.includes("/lens/")) return new Response(JSON.stringify(LENS), { status: 200 });
        if (url.includes("/node/")) return new Response(JSON.stringify({ demo_identity_mode: true, id: "p060", kind: "human", name: "Felix Osei" }), { status: 200 });
        if (url.endsWith("/me/scope")) return new Response("{}", { status: 404 });
        return new Response(JSON.stringify({ requests: [], grants: [] }), { status: 200 });
      }),
    );
    render(<EmployeeDashboard actor="p060" />);
    await waitFor(() => expect(screen.getByTestId("dashboard-user-name")).toBeTruthy());
    const levels = Array.from(document.querySelectorAll("h1,h2,h3,h4,h5,h6")).map((el) => Number(el.tagName.slice(1)));
    expect(levels.filter((l) => l === 1).length).toBe(1);
    let deepest = 0;
    for (const l of levels) {
      expect(l).toBeLessThanOrEqual(deepest + 1);
      deepest = l;
    }
    // The one h1 is the identity name (behaviour-frozen anchor).
    expect(document.querySelector("h1")?.textContent).toBe("Felix Osei");
  });
});
