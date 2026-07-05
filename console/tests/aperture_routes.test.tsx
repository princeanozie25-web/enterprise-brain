import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

import type { GraphResponse, NodeSummary, RoleScopeSummary } from "@/lib/api";
import { Console } from "@/components/Console";
import { ProductHome } from "@/components/ProductHome";

afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
  document.documentElement.setAttribute("data-theme", "dark");
  window.history.pushState({}, "", "/");
});

const GRAPH: GraphResponse = {
  actor_id: "p060",
  center: { id: "org", label: "Bryremead Distribution Ltd" },
  departments: [{ id: "Finance", label: "Finance", tint_key: "Finance" }],
  edges: [
    { from: "p060", kind: "member_of", to: "Finance" },
    { from: "Finance", kind: "member_of", to: "org" },
  ],
  people: [
    {
      avatar_ref: "faces/p060.jpg",
      department_id: "Finance",
      display_name: "Felix Osei",
      id: "p060",
      is_self: true,
      ring: "anchor",
      title: "Head of Finance",
    },
  ],
  projects: [],
  snapshot_version: "snap",
  sources: [],
  tools: [],
};

const ORG_SUMMARY: NodeSummary = {
  demo_identity_mode: true,
  id: "org",
  kind: "org",
  name: "Bryremead Distribution Ltd",
  stats: {
    agents: 4,
    capabilities: 90,
    departments: 8,
    document_total: 600,
    groups: 14,
    initiatives: 18,
    people: 120,
    permission_edges: 16881,
    principals: 124,
    sites: 2,
    sources: 5,
    strategies: 6,
    total_decisions: 74400,
    workflows: 40,
  },
};

const ROLE_SCOPE: RoleScopeSummary = {
  actor_id: "p060",
  admin_surface_allowed: false,
  approval_scope: { has_approval_scope: false, pending_count: 0 },
  bursar_surface_allowed: false,
  confidence: "high",
  demo_identity_mode: true,
  department_scope: {
    band: 5,
    department_id: "Finance",
    seniority: "Leadership",
  },
  derived_level: "department_head",
  enforcement: "derived_only",
  governance_surface_allowed: false,
  project_scope: { capability_ids: [], project_count: 0 },
  reasons: ["sensitive surfaces remain disallowed by this contract"],
  team_scope: { direct_report_count: 2, has_team_scope: true },
};

function stubRouteFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/auth/login")) {
        return new Response(
          JSON.stringify({ principal_id: "demo", session_token: "test-session", expires_at: 9_999_999_999 }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.endsWith("/me/scope")) {
        return new Response(JSON.stringify(ROLE_SCOPE), { status: 200 });
      }
      if (url.endsWith("/scope")) {
        return new Response(
          JSON.stringify({
            demo_identity_mode: true,
            principal_id: "p060",
            scope_statement: { band: 5, groups: ["grp_finance"], sites: ["site_keldonbury"] },
          }),
          { status: 200 },
        );
      }
      if (url.endsWith("/graph")) return new Response(JSON.stringify(GRAPH), { status: 200 });
      if (url.endsWith("/node/org/summary")) {
        return new Response(JSON.stringify(ORG_SUMMARY), { status: 200 });
      }
      if (url.endsWith("/access-requests/inbox")) {
        return new Response(
          JSON.stringify({ actor_id: "p060", demo_identity_mode: true, requests: [], snapshot_version: "snap" }),
          { status: 200 },
        );
      }
      if (url.endsWith("/access-requests")) {
        return new Response(
          JSON.stringify({ actor_id: "p060", demo_identity_mode: true, requests: [], snapshot_version: "snap" }),
          { status: 200 },
        );
      }
      return new Response("{\"demo_identity_mode\":true,\"error\":\"not found\"}", { status: 404 });
    }),
  );
}

describe("route separation", () => {
  it("renders the root as the identity-picker front door (A2)", () => {
    render(<ProductHome />);

    expect(screen.getByTestId("root-home")).toBeTruthy();
    // One product name, one sentence — nothing else competes.
    expect(screen.getByRole("heading", { name: "Enterprise Brain" })).toBeTruthy();
    expect(screen.getByTestId("root-one-sentence").textContent).toContain(
      "Every answer respects what you're allowed to see",
    );
    expect(screen.queryByText("Company Operating System")).toBeNull();
    expect((screen.getByTestId("root-home").textContent ?? "")).not.toContain("Aperture");

    // The picker is the front door: heading, the verbatim demo-mode line,
    // and the three featured REAL fixture identities. No hardwired p060.
    expect(screen.getByRole("heading", { name: "Who are you today?" })).toBeTruthy();
    const demoLine = screen.getByTestId("identity-picker-demo-line").textContent ?? "";
    expect(demoLine).toContain("Demo mode: sign in as anyone — no password.");
    expect(demoLine).toContain("View-as is open to everyone.");
    expect(demoLine).toContain("Nothing here is deployed.");
    expect(screen.getByTestId("identity-option-p060").getAttribute("href")).toBe("/me?as=p060");
    expect(screen.getByTestId("identity-option-p060").textContent).toContain("Felix Osei");
    expect(screen.getByTestId("identity-option-p088").getAttribute("href")).toBe("/me?as=p088");
    expect(screen.getByTestId("identity-option-p088").textContent).toContain("Tomas Reyes");
    expect(screen.getByTestId("identity-option-p_void").getAttribute("href")).toBe("/me?as=p_void");
    expect(screen.getByTestId("identity-option-p_void").textContent).toContain("No access");

    // The old wall of route cards / doctrine tiles / trust grid is gone —
    // the front door asks ONE question.
    expect(screen.queryByTestId("root-link-me")).toBeNull();
    expect(screen.queryByTestId("root-demo-flow")).toBeNull();
    expect(screen.queryByTestId("root-buyer-trust-posture")).toBeNull();
    expect(screen.queryByTestId("root-admin-note")).toBeNull();
    expect(screen.getByTestId("root-home").textContent ?? "").not.toMatch(/supplier|invoice|procurement/i);
    expect(screen.getByTestId("root-home").textContent ?? "").not.toMatch(/derived-only|derived only|SOC2|SOC 2|ISO27001|ISO 27001|certified|certification|live SSO|live IAM/i);
  });

  it("keeps product navigation split across daily, project, ask, and admin-domain surfaces", () => {
    render(<Console view="me" />);

    expect(screen.getByTestId("view-door-me").getAttribute("aria-current")).toBe("page");
    // A1: the locked vocabulary — /me is Home; lens/atlas carry plain names.
    expect(screen.getByTestId("view-door-me").textContent).toBe("Home");
    expect(screen.getByTestId("view-door-lens").textContent).toBe("My Access");
    expect(screen.getByTestId("view-door-atlas").textContent).toBe("Company Map");
    expect(screen.getByTestId("view-door-lane").textContent).toBe("Review Queue");
    expect(screen.getByTestId("view-door-project").getAttribute("href")).toBe("/project");
    expect(screen.getByTestId("view-door-project").textContent).toBe("Projects");
    expect(screen.getByTestId("view-door-ask").getAttribute("href")).toBe("/ask");
    expect(screen.getByTestId("view-door-admin-graph").getAttribute("href")).toBe("/admin/graph");
    expect(screen.getByTestId("view-door-admin-graph").textContent).toBe("Operating Map");
    expect(screen.queryByTestId("view-door-bursar")).toBeNull();
    expect(screen.queryByTestId("admin-preview-badge")).toBeNull();
    // A4: the shell notice is THE single demo-status line — on /me too.
    expect(screen.getByTestId("shell-demo-identity-mode")).toBeTruthy();
    expect(screen.getByTestId("theme-toggle").textContent).toContain("Light mode");
    fireEvent.click(screen.getByTestId("theme-toggle"));
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    expect(localStorage.getItem("ap-theme")).toBe("light");
  });

  it("renders direct Ask and Projects route shells without fabricating access", () => {
    const ask = render(<Console view="ask" />);
    expect(screen.getByRole("heading", { name: "Ask" })).toBeTruthy();
    expect(screen.getByText("Ask a question. Every answer shows its sources.")).toBeTruthy();
    expect(screen.queryByTestId("view-door-bursar")).toBeNull();
    ask.unmount();

    render(<Console view="project" />);
    const empty = screen.getByTestId("project-empty");
    expect(empty.textContent).toContain("Choose a Work Identity to review work.");
    expect(empty.textContent).toContain("real capability-backed workflow");
    // A2: no hardwired identity — identity-less links carry no ?as; the
    // front-door picker catches them.
    expect(screen.getByTestId("project-empty-work-identity-link").getAttribute("href")).toBe("/me");
    expect(screen.getByTestId("project-empty-operating-map-link").getAttribute("href")).toBe("/admin/graph");
    expect(screen.queryByTestId("view-door-bursar")).toBeNull();
    expect(document.querySelector("a[href='/admin/bursar']")).toBeNull();
  });

  it("renders the Spend Ledger as an honest placeholder without fake spend data", () => {
    render(<Console view="adminBursar" />);

    // Fail-closed: the admin-domain surface is gated behind an explicit,
    // labelled preview opt-in and is never rendered by default.
    expect(screen.getByTestId("admin-preview-gate")).toBeTruthy();
    expect(screen.queryByTestId("bursar-surface")).toBeNull();
    fireEvent.click(screen.getByTestId("admin-preview-gate-reveal"));

    const surface = screen.getByTestId("bursar-surface");
    const text = surface.textContent ?? "";
    expect(text).toContain("Spend Ledger");
    expect(text).toContain("What AI assistance costs, and who authorized it.");
    expect(text).toContain("Authorization before spend");
    expect(text).toContain("Fail closed by default");
    expect(text).toContain("Audit before effect");
    expect(text).toContain("Reconcile every call");
    expect(text).toContain("ledger.v1.1 expected");
    expect(text).toContain("producer not connected in this UI surface");
    expect(text).toContain("admin-side preview");
    expect(text).toContain("finance authority pending");
    // A4: the demo-status line lives on the SHELL notice (one per page),
    // not inside the Spend Ledger surface.
    const shellNotice = screen.getByTestId("shell-demo-identity-mode").textContent ?? "";
    expect(shellNotice).toContain("Demo Identity Mode");
    expect(shellNotice).toContain("Production identity is not connected");
    expect(text).toContain("No ledger fixture is connected in this workspace yet.");
    expect(text).toContain("Same console: the answer, and the governed spend it cost");
    expect(text).not.toMatch(/supplier|invoice|procurement|duplicate payment|savings opportunity/i);
    expect(text).not.toMatch(/spend total|total spend|model call total|token total|total tokens/i);
    expect(text).not.toMatch(/budget used|cost chart|ledger row|savings/i);
    expect(text).not.toMatch(/SOC2|SOC 2|ISO27001|ISO 27001|certified|certification|live SSO|live IAM/i);
    expect(text).not.toMatch(/£|\$[0-9]|[0-9][0-9,]*\s*tokens?/i);
    expect(screen.queryByTestId("bursar-ledger-row")).toBeNull();
    expect(screen.queryByTestId("bursar-cost-chart")).toBeNull();
    expect(screen.getByTestId("view-door-bursar").getAttribute("aria-current")).toBe("page");
  });

  it("renders the admin graph as honestly scoped to the viewer's access (AUTH-2 enforced)", async () => {
    stubRouteFetch();
    window.history.pushState({}, "", "/admin/graph?as=p060");

    render(<Console view="adminGraph" />);

    // AUTH-2: the Operating Map IS now per-identity access-enforced (server-side),
    // so the gate copy says so honestly — scoped to the viewer's access — while
    // still avoiding the old over-claims and the now-false "pending" framing.
    const gate = await screen.findByTestId("admin-preview-gate");
    expect(gate.textContent).toMatch(/scoped to your access/i);
    expect(gate.textContent).toMatch(/enforced now/i);
    expect(gate.textContent).not.toContain("Authorization gate");
    expect(gate.textContent).not.toContain("not granted");
    expect(gate.textContent).not.toMatch(/pending the authorization build/i);
    expect(screen.queryByTestId("graph-room")).toBeNull();
    fireEvent.click(screen.getByTestId("admin-preview-gate-reveal"));

    await waitFor(() => expect(screen.getByTestId("graph-room")).toBeTruthy());
    await waitFor(() => {
      expect(screen.getByTestId("admin-graph-preview-banner").textContent).toContain("Demo Identity Mode");
    });
    expect(screen.getByTestId("admin-graph-preview-banner").textContent).toMatch(/scoped to your Work Identity/i);
    expect(screen.getByTestId("admin-graph-preview-banner").textContent).not.toMatch(/not per-identity access-enforced/i);
    await waitFor(() => expect(screen.getByTestId("graph-audited-line").textContent).toContain("This view is audited"));
    expect(screen.getByTestId("graph-acting-context").textContent).toContain("Acting as p060");
    expect(screen.getByTestId("graph-relationship-summary").textContent).toContain("Keyboard-readable rows");
    expect(screen.getByTestId("view-door-admin-graph").getAttribute("aria-current")).toBe("page");
    expect(screen.getByTestId("graph-room").textContent ?? "").not.toMatch(/derived-only|derived only|SOC2|SOC 2|ISO27001|ISO 27001|certified|certification|live SSO|live IAM/i);
    expect(screen.getByTestId("graph-room").textContent ?? "").not.toMatch(/Bursar|supplier|invoice|spend total|token total|ledger row|signals unavailable|permissions unavailable/i);
  });
});
