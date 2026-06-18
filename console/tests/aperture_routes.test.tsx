import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";

import type { GraphResponse, NodeSummary, RoleScopeSummary } from "@/lib/api";
import { Console } from "@/components/Console";
import { ProductHome } from "@/components/ProductHome";

afterEach(() => {
  vi.unstubAllGlobals();
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
  it("renders the root as an intentional product entry route", () => {
    render(<ProductHome />);

    expect(screen.getByTestId("root-home")).toBeTruthy();
    expect(screen.getByTestId("root-link-me").getAttribute("href")).toBe("/me");
    expect(screen.getByTestId("root-link-project").getAttribute("href")).toBe("/project");
    expect(screen.getByTestId("root-link-ask").getAttribute("href")).toBe("/ask");
    expect(screen.getByTestId("root-link-admin-graph").getAttribute("href")).toBe("/admin/graph");
    expect(screen.getByTestId("root-link-admin-bursar").getAttribute("href")).toBe("/admin/bursar");
    expect(screen.getByTestId("root-admin-note").textContent).toContain("derived-only");
  });

  it("keeps product navigation split across daily, project, ask, and admin-domain surfaces", () => {
    render(<Console view="me" />);

    expect(screen.getByTestId("view-door-me").getAttribute("aria-current")).toBe("page");
    expect(screen.getByTestId("view-door-project").getAttribute("href")).toBe("/project");
    expect(screen.getByTestId("view-door-ask").getAttribute("href")).toBe("/ask");
    expect(screen.getByTestId("view-door-admin-graph").getAttribute("href")).toBe("/admin/graph");
    expect(screen.queryByTestId("view-door-bursar")).toBeNull();
    expect(screen.getByTestId("admin-preview-badge").textContent).toContain("not full auth enforced yet");
  });

  it("renders Bursar as an honest finance placeholder without fake spend data", () => {
    render(<Console view="adminBursar" />);

    const surface = screen.getByTestId("bursar-surface");
    expect(surface.textContent).toContain("Finance intelligence surface");
    expect(surface.textContent).toContain("not server-enforced yet");
    expect(surface.textContent).toContain("no spend data connected");
    expect(surface.textContent).toContain("Bursar data model is not connected");
    expect(surface.textContent).toContain("supplier master");
    expect(surface.textContent).toContain("invoice ledger");
    expect(surface.textContent).toContain("Read grants for project context do not unlock");
    expect(surface.textContent).not.toMatch(/£|\$|savings found|duplicate payments found/i);
    expect(screen.getByTestId("view-door-bursar").getAttribute("aria-current")).toBe("page");
  });

  it("renders the admin graph shell as a derived preview, not a security claim", async () => {
    stubRouteFetch();
    window.history.pushState({}, "", "/admin/graph?as=p060");

    render(<Console view="adminGraph" />);

    await waitFor(() => expect(screen.getByTestId("graph-room")).toBeTruthy());
    await waitFor(() => {
      expect(screen.getByTestId("admin-graph-preview-banner").textContent).toContain("derived_only");
    });
    expect(screen.getByTestId("admin-graph-preview-banner").textContent).toContain("admin not granted");
    expect(screen.getByTestId("view-door-admin-graph").getAttribute("aria-current")).toBe("page");
  });
});
