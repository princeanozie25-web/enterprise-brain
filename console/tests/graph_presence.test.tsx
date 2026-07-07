/**
 * SHOWCASE-1 (Track B) cross-component laws: the masthead honesty rule, the
 * view-as disclosure, the legend honesty (payload counts, no lying row), the
 * department-pastel determinism, and the fullscreen-container mode. The
 * OrgGraph layout coverage lives in aperture_graph.test.tsx (REF-1..7).
 */
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import type { GraphResponse } from "@/lib/api";
import { departmentPastelMap, DEPARTMENT_PASTEL } from "@/lib/tokens";
import { Console } from "@/components/Console";
import { GraphRoom } from "@/components/GraphRoom";
import { DemoIdentityNotice } from "@/components/TrustPosture";

afterEach(() => {
  vi.unstubAllGlobals();
  cleanup();
});

const GRAPH: GraphResponse = {
  actor_id: "p113",
  center: { id: "org", label: "Bryremead Distribution Ltd" },
  departments: [
    { id: "Finance", label: "Finance", tint_key: "Finance" },
    { id: "IT", label: "IT", tint_key: "IT" },
    { id: "HR", label: "HR", tint_key: "HR" },
  ],
  people: [
    { id: "p060", display_name: "Felix Osei", title: "Head of Finance", department_id: "Finance", avatar_ref: "", is_self: false, ring: "anchor" },
    { id: "p061", display_name: "Ana Flores", title: "Accounts Payable Clerk", department_id: "Finance", avatar_ref: "", is_self: false, ring: "member" },
    { id: "p074", display_name: "Yuki Moreau", title: "Head of IT", department_id: "IT", avatar_ref: "", is_self: false, ring: "anchor" },
    { id: "p088", display_name: "Tomas Iqbal", title: "HR Associate", department_id: "HR", avatar_ref: "", is_self: false, ring: "member" },
  ],
  tools: [{ id: "agent_finance_analyst", label: "Finance analysis assistant", kind: "agent", department_id: "Finance" }],
  sources: [{ id: "docstore", kind: "source", label: "Document store" }],
  projects: [],
  edges: [
    { from: "p060", kind: "member_of", to: "Finance" },
    { from: "p061", kind: "member_of", to: "Finance" },
    { from: "p074", kind: "member_of", to: "IT" },
    { from: "p088", kind: "member_of", to: "HR" },
    { from: "Finance", kind: "member_of", to: "org" },
    { from: "IT", kind: "member_of", to: "org" },
    { from: "HR", kind: "member_of", to: "org" },
    { from: "p061", kind: "reports_to", to: "p060" },
    { from: "docstore", kind: "system_of", to: "org" },
  ],
  snapshot_version: "snap",
};

function stubGraphFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/graph")) return new Response(JSON.stringify(GRAPH), { status: 200 });
      if (url.endsWith("/node/org/summary"))
        return new Response(
          JSON.stringify({ demo_identity_mode: true, id: "org", kind: "org", name: "Bryremead Distribution Ltd" }),
          { status: 200 },
        );
      return new Response('{"demo_identity_mode":true,"error":"not found"}', { status: 404 });
    }),
  );
}

/** Like stubGraphFetch, but the access-request endpoints RESOLVE with an inbox
 * item so `accessAvailable` becomes true and the rail actually renders — the
 * only way to prove the fullscreen mode clears a PRESENT rail. */
function stubGraphFetchWithAccess() {
  const inboxRequest = {
    approver_id: "p113",
    created_ordinal: 0,
    justification: "Need this capability context for assigned project work.",
    request_id: "ar_show4",
    request_key: "rk",
    requester_id: "p061",
    snapshot_version: "snap",
    status: "pending",
    target: { kind: "project", capability_id: "cap31" },
  };
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/graph")) return new Response(JSON.stringify(GRAPH), { status: 200 });
      if (url.endsWith("/node/org/summary"))
        return new Response(
          JSON.stringify({ demo_identity_mode: true, id: "org", kind: "org", name: "Bryremead Distribution Ltd" }),
          { status: 200 },
        );
      if (url.endsWith("/access-requests/inbox"))
        return new Response(
          JSON.stringify({ actor_id: "p113", demo_identity_mode: true, requests: [inboxRequest], snapshot_version: "snap" }),
          { status: 200 },
        );
      if (url.endsWith("/access-requests"))
        return new Response(
          JSON.stringify({ actor_id: "p113", demo_identity_mode: true, requests: [], snapshot_version: "snap" }),
          { status: 200 },
        );
      return new Response('{"demo_identity_mode":true,"error":"not found"}', { status: 404 });
    }),
  );
}

// ---------------------------------------------------------------------------
// SHOW-1: the masthead keeps the FULL payload edge count + focus sub-label
// ---------------------------------------------------------------------------

describe("SHOW-1: the masthead discloses the whole payload edge count", () => {
  it("Connections (N) counts every payload edge; 'shown on focus' names the staging", async () => {
    stubGraphFetch();
    render(<GraphRoom actor="p113" />);
    await waitFor(() => expect(screen.getByTestId("graph-connections-sublabel")).toBeTruthy());
    expect(screen.getByTestId("graph-room").textContent).toContain(`Connections (${GRAPH.edges.length})`);
    expect(screen.getByTestId("graph-connections-sublabel").textContent).toBe("shown on focus");
  });
});

// ---------------------------------------------------------------------------
// SHOW-2: the legend is payload-honest and never lies (B6)
// ---------------------------------------------------------------------------

describe("SHOW-2: the legend counts are payload-derived and no row lies", () => {
  it("names signals-unavailable + focus, real software/agents/people counts, and BANS 'permissions unavailable'", async () => {
    stubGraphFetch();
    render(<GraphRoom actor="p113" />);
    await waitFor(() => expect(screen.getByTestId("graph-legend")).toBeTruthy());
    const legend = screen.getByTestId("graph-legend");
    expect(screen.getByTestId("legend-signals").textContent).toContain("signals unavailable");
    expect(screen.getByTestId("legend-focus").textContent).toContain("connections shown on focus");
    expect(screen.getByTestId("legend-software").textContent).toContain(`software ${GRAPH.sources.length}`);
    expect(screen.getByTestId("legend-agents").textContent).toContain(`agents ${GRAPH.tools.length}`);
    expect(screen.getByTestId("legend-people").textContent).toContain(`people ${GRAPH.people.length}`);
    // The reference's lying row is banned — permissions are the enforced product.
    expect(legend.textContent?.toLowerCase()).not.toContain("permissions unavailable");
  });

  it("the banned legend string appears in no source file", async () => {
    const fs = await import("node:fs");
    const path = await import("node:path");
    const SRC = path.resolve(__dirname, "..", "src");
    const walk = (dir: string): string[] =>
      fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
        const full = path.join(dir, e.name);
        return e.isDirectory() ? walk(full) : /\.tsx?$/.test(e.name) ? [full] : [];
      });
    const offenders = walk(SRC).filter((f) => fs.readFileSync(f, "utf8").includes("permissions unavailable"));
    expect(offenders).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// SHOW-3: department pastel determinism (B3)
// ---------------------------------------------------------------------------

describe("SHOW-3: the department pastel family is deterministic and distinct", () => {
  it("assigns each department a distinct pastel, semantically where a keyword matches", () => {
    const labels = [
      "Quality & Compliance",
      "Warehouse Operations",
      "Pharmacy Services",
      "Finance",
      "IT",
      "HR",
      "Sales & Accounts",
      "Executive",
    ];
    const map = departmentPastelMap(labels);
    // Every department mapped, all distinct.
    expect(new Set(map.values()).size).toBe(labels.length);
    // Semantic anchors from the family.
    expect(map.get("Finance")).toBe("#E7C86E");
    expect(map.get("HR")).toBe("#E8A0BF");
    expect(map.get("Executive")).toBe("#B7A6E3");
    expect(map.get("Quality & Compliance")).toBe("#9DC7A0");
    expect(map.get("Sales & Accounts")).toBe("#7FA8E8");
    expect(map.get("IT")).toBe("#7EC8D8");
    expect(map.get("Warehouse Operations")).toBe("#E8A76E"); // keyword: operations
    expect(map.get("Pharmacy Services")).toBe("#8FBFA8"); // keyword: pharmacy (Logistics pastel)
  });

  it("is stable regardless of input order (sorted internally)", () => {
    const a = departmentPastelMap(["Finance", "HR", "IT"]);
    const b = departmentPastelMap(["IT", "Finance", "HR"]);
    expect([...a.entries()].sort()).toEqual([...b.entries()].sort());
  });

  it("no pastel is an Okabe–Ito sensitivity hue (the reserved-color firewall holds)", () => {
    const sensitivity = ["#0072B2", "#009E73", "#E69F00", "#D55E00", "#CC79A7"];
    for (const { hex } of DEPARTMENT_PASTEL) {
      expect(sensitivity.map((s) => s.toLowerCase())).not.toContain(hex.toLowerCase());
    }
  });
});

// ---------------------------------------------------------------------------
// SHOW-4: the fullscreen-container mode (B1)
// ---------------------------------------------------------------------------

describe("SHOW-4: fullscreen expands the map and Escape exits; the masthead never hides", () => {
  it("clears a PRESENT access rail while keeping the scope masthead, and Escape restores both", async () => {
    stubGraphFetchWithAccess();
    render(<GraphRoom actor="p113" adminPreview />);
    await waitFor(() => expect(screen.getByTestId("graph-fullscreen")).toBeTruthy());
    // The rail is actually present at rest (access endpoints resolved) — so the
    // "fullscreen clears it" mechanic is observed, not vacuous.
    await waitFor(() => expect(screen.getByTestId("access-request-rail")).toBeTruthy());
    expect(screen.getByTestId("graph-audit-panel")).toBeTruthy();

    const btn = screen.getByTestId("graph-fullscreen");
    expect(btn.getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(btn);
    expect(btn.getAttribute("aria-pressed")).toBe("true");
    // Fullscreen clears the rail but the masthead STAYS (the honesty spine).
    expect(screen.queryByTestId("access-request-rail")).toBeNull();
    expect(screen.getByTestId("graph-audit-panel")).toBeTruthy();

    // Escape exits and the rail comes back.
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.getByTestId("graph-fullscreen").getAttribute("aria-pressed")).toBe("false"));
    expect(screen.getByTestId("access-request-rail")).toBeTruthy();
  });
});

// ---------------------------------------------------------------------------
// SHOW-6: THE ROLE-AWARE SHELL (Track A) — the three-door law + admin gate
// ---------------------------------------------------------------------------

function stubRoleScope(derivedLevel: string) {
  const mock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/auth/login"))
        return new Response(
          JSON.stringify({ principal_id: "p113", session_token: "t", expires_at: 0 }),
          { status: 200 },
        );
      if (url.endsWith("/me/scope"))
        return new Response(
          JSON.stringify({
            actor_id: "p113",
            admin_surface_allowed: false,
            approval_scope: { has_approval_scope: false, pending_count: 0 },
            bursar_surface_allowed: false,
            confidence: "high",
            demo_identity_mode: true,
            department_scope: { department_id: "Executive", seniority: "exec" },
            derived_level: derivedLevel,
            enforcement: "derived_only",
            governance_surface_allowed: false,
            project_scope: { capability_ids: [], project_count: 0 },
            reasons: [],
            team_scope: { direct_report_count: 0, has_team_scope: false },
          }),
          { status: 200 },
        );
      if (url.includes("/scope"))
        return new Response(
          JSON.stringify({ demo_identity_mode: true, principal_id: "p113", scope_statement: { band: 5, groups: [], sites: [] } }),
          { status: 200 },
        );
      return new Response('{"demo_identity_mode":true}', { status: 404 });
  });
  vi.stubGlobal("fetch", mock);
  return mock;
}

describe("SHOW-6: the role-aware shell", () => {
  it("an admin-class identity (executive_candidate) additionally sees the operations doors", async () => {
    stubRoleScope("executive_candidate");
    // The URL-borne identity door (?as=p113) must be present BEFORE mount so
    // the entry-door effect resolves the principal → login → role posture.
    window.history.replaceState(null, "", "/me?as=p113");
    render(<Console view="me" />);
    // The base trio always shows.
    expect(screen.getByTestId("view-door-me")).toBeTruthy();
    expect(screen.getByTestId("view-door-ask")).toBeTruthy();
    expect(screen.getByTestId("view-door-project")).toBeTruthy();
    // The admin doors appear once the admin-class posture resolves.
    await waitFor(() => expect(screen.getByTestId("admin-doors")).toBeTruthy());
    expect(screen.getByTestId("view-door-admin-graph")).toBeTruthy();
    expect(screen.getByTestId("view-door-bursar")).toBeTruthy();
    expect(screen.getByTestId("view-door-lane")).toBeTruthy();
    expect(screen.getByTestId("view-door-atlas")).toBeTruthy();
    window.history.replaceState(null, "", "/");
  });

  it("a standard identity (department_head) never gets the admin doors", async () => {
    const mock = stubRoleScope("department_head");
    window.history.replaceState(null, "", "/me?as=p113");
    render(<Console view="me" />);
    // Deterministically drive the entry-door → login → /me/scope chain to
    // completion (no fixed sleep): wait for the posture endpoint to be
    // requested, then flush the setRoleScope microtask + re-render. Once the
    // department_head posture has APPLIED, the admin doors must be absent —
    // a real assertion keyed on resolution, not a timing race.
    await waitFor(() =>
      expect(mock.mock.calls.some((c) => String(c[0]).endsWith("/me/scope"))).toBe(true),
    );
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(screen.queryByTestId("admin-doors")).toBeNull();
    expect(screen.queryByTestId("view-door-admin-graph")).toBeNull();
    window.history.replaceState(null, "", "/");
  });

  it("the settings drawer carries My Access, Appearance, and Identity — with focus trap", () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response('{"demo_identity_mode":true}', { status: 404 })));
    render(<Console view="ask" />);
    fireEvent.click(screen.getByTestId("settings-open"));
    const drawer = screen.getByTestId("settings-drawer");
    expect(drawer.getAttribute("role")).toBe("dialog");
    expect(drawer.getAttribute("aria-modal")).toBe("true");
    expect(within(drawer).getByTestId("settings-my-access")).toBeTruthy();
    expect(within(drawer).getByTestId("settings-appearance")).toBeTruthy();
    expect(within(drawer).getByTestId("settings-identity")).toBeTruthy();
    expect(within(drawer).getByTestId("settings-my-access-link").getAttribute("href")).toBe("/lens");
    // Escape closes.
    fireEvent.keyDown(drawer, { key: "Escape" });
    expect(screen.queryByTestId("settings-drawer")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// SHOW-5: the view-as disclosure (council D3, carried)
// ---------------------------------------------------------------------------

describe("SHOW-5: the view-as disclosure renders once, calm register", () => {
  it("names the demo view-as rule and its audit, without error styling", () => {
    render(<DemoIdentityNotice />);
    const line = screen.getByTestId("view-as-disclosure");
    expect(line.textContent).toBe(
      "In demo mode, any identity may view-as any other — every view-as is audited before render.",
    );
    expect(line.className).toContain("ap-soft");
    expect(line.getAttribute("role")).toBeNull();
  });
});
