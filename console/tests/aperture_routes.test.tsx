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
    expect(screen.getByRole("heading", { name: "Company Operating System" })).toBeTruthy();
    expect(screen.getByText("Governed knowledge")).toBeTruthy();
    expect(screen.getByText("Governed workflows")).toBeTruthy();
    expect(screen.getByText("Governed spend")).toBeTruthy();
    expect(screen.getByTestId("root-demo-flow").textContent).toContain("Work Identity");
    expect(screen.getByTestId("root-demo-flow").textContent).toContain("Granted Knowledge");
    expect(screen.getByTestId("root-demo-flow").textContent).toContain("Bursar Ledger Room");
    expect(screen.getByTestId("root-link-me").getAttribute("href")).toBe("/me?as=p060");
    expect(screen.getByTestId("root-link-me").textContent).toContain("Work Identity");
    expect(screen.getByTestId("root-link-project").getAttribute("href")).toBe("/project");
    expect(screen.getByTestId("root-link-project").textContent).toContain("Workflow Command");
    expect(screen.getByTestId("root-link-ask").getAttribute("href")).toBe("/ask?as=p060");
    expect(screen.getByTestId("root-link-ask").textContent).toContain("Permission-aware Ask");
    expect(screen.getByTestId("root-link-admin-graph").getAttribute("href")).toBe("/admin/graph?as=p060");
    expect(screen.getByTestId("root-link-admin-graph").textContent).toContain("Operating Map");
    expect(screen.getByTestId("root-link-admin-bursar").getAttribute("href")).toBe("/admin/bursar");
    expect(screen.getByTestId("root-link-admin-bursar").textContent).toContain("Bursar Ledger Room");
    expect(screen.getByTestId("root-demo-identity-mode").textContent).toContain("Demo Identity Mode");
    expect(screen.getByTestId("root-demo-identity-mode").textContent).toContain("Production identity is not connected");
    expect(screen.getByTestId("root-buyer-trust-posture").textContent).toContain("Enterprise identity boundary");
    expect(screen.getByTestId("root-buyer-trust-posture").textContent).toContain("Admin and Bursar routes are separated");
    expect(screen.getByTestId("root-admin-note").textContent).toContain("Production");
    expect(screen.getByTestId("root-admin-note").textContent).toContain("ledger producers");
    expect(screen.getByTestId("root-home").textContent ?? "").not.toMatch(/supplier|invoice|procurement/i);
    expect(screen.getByTestId("root-home").textContent ?? "").not.toMatch(/derived-only|derived only|SOC2|SOC 2|ISO27001|ISO 27001|certified|certification|live SSO|live IAM/i);
  });

  it("keeps product navigation split across daily, project, ask, and admin-domain surfaces", () => {
    render(<Console view="me" />);

    expect(screen.getByTestId("view-door-me").getAttribute("aria-current")).toBe("page");
    expect(screen.getByTestId("view-door-me").textContent).toBe("Work Identity");
    expect(screen.getByTestId("view-door-project").getAttribute("href")).toBe("/project");
    expect(screen.getByTestId("view-door-project").textContent).toBe("Workflow Command");
    expect(screen.getByTestId("view-door-ask").getAttribute("href")).toBe("/ask");
    expect(screen.getByTestId("view-door-admin-graph").getAttribute("href")).toBe("/admin/graph");
    expect(screen.getByTestId("view-door-admin-graph").textContent).toBe("Operating Map");
    expect(screen.queryByTestId("view-door-bursar")).toBeNull();
    expect(screen.getByTestId("admin-preview-badge").textContent).toContain("Demo Identity Mode");
    expect(screen.getByTestId("admin-preview-badge").textContent).toContain("production authority binding is not connected");
    expect(screen.getByTestId("shell-demo-identity-mode").textContent).toContain("Production identity is not connected");
  });

  it("renders direct Ask and Workflow Command route shells without fabricating access", () => {
    const ask = render(<Console view="ask" />);
    expect(screen.getByRole("heading", { name: "Ask" })).toBeTruthy();
    expect(screen.getByText("Permission-aware Ask with Work Identity scope, provenance, and fail-closed grant checks.")).toBeTruthy();
    expect(screen.queryByTestId("view-door-bursar")).toBeNull();
    ask.unmount();

    render(<Console view="project" />);
    const empty = screen.getByTestId("project-empty");
    expect(empty.textContent).toContain("Choose a Work Identity to review work.");
    expect(empty.textContent).toContain("real capability-backed workflow");
    expect(screen.getByTestId("project-empty-work-identity-link").getAttribute("href")).toBe("/me?as=p060");
    expect(screen.getByTestId("project-empty-operating-map-link").getAttribute("href")).toBe("/admin/graph?as=p060");
    expect(screen.queryByTestId("view-door-bursar")).toBeNull();
    expect(document.querySelector("a[href='/admin/bursar']")).toBeNull();
  });

  it("renders Bursar as an honest Ledger-room placeholder without fake spend data", () => {
    render(<Console view="adminBursar" />);

    const surface = screen.getByTestId("bursar-surface");
    const text = surface.textContent ?? "";
    expect(text).toContain("Bursar Ledger Room");
    expect(text).toContain("Governed spend for Enterprise Brain model actions.");
    expect(text).toContain("Authorization before spend");
    expect(text).toContain("Fail closed by default");
    expect(text).toContain("Audit before effect");
    expect(text).toContain("Reconcile every call");
    expect(text).toContain("ledger.v1.1 expected");
    expect(text).toContain("producer not connected in this UI surface");
    expect(text).toContain("admin-side preview");
    expect(text).toContain("finance authority pending");
    expect(text).toContain("Demo Identity Mode");
    expect(text).toContain("Production identity is not connected");
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

  it("renders the admin graph shell as a Demo Identity Mode preview, not a security claim", async () => {
    stubRouteFetch();
    window.history.pushState({}, "", "/admin/graph?as=p060");

    render(<Console view="adminGraph" />);

    await waitFor(() => expect(screen.getByTestId("graph-room")).toBeTruthy());
    await waitFor(() => {
      expect(screen.getByTestId("admin-graph-preview-banner").textContent).toContain("Demo Identity Mode");
    });
    expect(screen.getByTestId("admin-graph-preview-banner").textContent).toContain("production admin authority not connected");
    expect(screen.getByTestId("admin-graph-preview-banner").textContent).toContain("admin not granted");
    await waitFor(() => expect(screen.getByTestId("graph-audited-line").textContent).toContain("This view is audited"));
    expect(screen.getByTestId("graph-acting-context").textContent).toContain("Acting as p060");
    expect(screen.getByTestId("graph-relationship-summary").textContent).toContain("Keyboard-readable rows");
    expect(screen.getByTestId("view-door-admin-graph").getAttribute("aria-current")).toBe("page");
    expect(screen.getByTestId("graph-room").textContent ?? "").not.toMatch(/derived-only|derived only|SOC2|SOC 2|ISO27001|ISO 27001|certified|certification|live SSO|live IAM/i);
    expect(screen.getByTestId("graph-room").textContent ?? "").not.toMatch(/Bursar|supplier|invoice|spend total|token total|ledger row|signals unavailable|permissions unavailable/i);
  });
});
