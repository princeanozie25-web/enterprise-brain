/**
 * ATTESTATION (Track B1) — a live axe-core sweep over the five council
 * surfaces, in BOTH themes, at wcag2a + wcag2aa. Zero violations is the bar;
 * any rule disable must be documented inline with its reason.
 *
 * DOCUMENTED SCOPE (not a disable): axe's color-contrast check needs a real
 * layout engine and canvas; under jsdom it self-reports as "incomplete",
 * never as a violation — contrast is proven separately by the computed-ratio
 * badge test (T-B1) and the focus-ring token-identity test (T-B4), both in
 * comprehension_cinematic.test.tsx, so nothing here is silently waved
 * through. Note heading-order/page-has-heading-one are best-practice tags
 * axe excludes under wcag2a/aa — attestation_headings.test.tsx is the
 * heading gate, not this sweep.
 */
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { axe } from "vitest-axe";

import { Console } from "@/components/Console";
import { EntryScreen } from "@/components/EntryScreen";
import { ProductHome } from "@/components/ProductHome";
import { GraphRoom } from "@/components/GraphRoom";
import { BursarSurface } from "@/components/BursarSurface";
import { ProjectSurface } from "@/components/ProjectSurface";
import type { AnswerEnvelope, GraphResponse, ProjectWorkflowResponse } from "@/lib/api";
import { richEnvelope } from "./fixtures/typed";

const PIPE_PROV = {
  capability: { id: "cap31", name: "Access Review 31" },
  initiative: { id: "init03", name: "Strengthen Workforce Capability" },
  strategy: { id: "strat01", name: "Workforce Capability" },
  workflow: { id: "wf11", name: "Goods-In Verification 31" },
};
const PIPE_WORKFLOW: ProjectWorkflowResponse = {
  actor_id: "p060",
  capability_id: "cap31",
  demo_identity_mode: true,
  provenance: PIPE_PROV,
  snapshot_version: "snap",
  items: [
    { capability_id: "cap31", dependencies: ["box_seed"], item_id: "box_active", kind: "lane_box", owner_id: "p060", provenance: PIPE_PROV, snapshot_version: "snap", status: "active", title: "Verify goods-in batch 31" },
    { capability_id: "cap31", dependencies: [], item_id: "ar_mine", kind: "access_request", approver_id: "p060", requester_id: "p074", provenance: PIPE_PROV, snapshot_version: "snap", status: "pending", title: "Access request for Access Review 31" },
    { capability_id: "cap31", dependencies: ["box_active"], item_id: "box_done", kind: "accepted_agent_box", owner_id: "p060", provenance: PIPE_PROV, snapshot_version: "snap", status: "done", title: "Review accepted agent proposal" },
  ],
};
// SHOWCASE-III: one pending proposal so the axe sweep covers the proposal
// card, anchor chips (visible + S4-withheld), and the approver's live gate.
const PIPE_PROPOSAL = {
  proposal_id: "wfp_0001",
  proposer_id: "p074",
  capability_id: "cap31",
  approver_id: "p060",
  title: "Onboarding new hires",
  goal: "confidential financial statements",
  drafted_from: "Drafted by a model from documents p074 is authorized to see.",
  boxes: [
    {
      box_index: 0,
      stage: "Next",
      title: "Collect the signed statements",
      description: "Gather the confidential financial statements for the quarter.",
      anchors: [
        { visible: true, doc_id: "doc_fin_042", quote: "filed quarterly", locator: "doc_fin_042@118" },
        { visible: false },
      ],
      sources_total: 2,
      sources_outside_view: 1,
    },
  ],
  grounding: { admitted: 1, refused: 1 },
  status: "pending",
  created_ordinal: 1,
  materialized: false,
  snapshot_version: "snap",
};
function stubPipelineFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/workflow/project/cap31")) return new Response(JSON.stringify(PIPE_WORKFLOW), { status: 200 });
      if (url.includes("/workflow/proposals?role=proposer"))
        return new Response(
          JSON.stringify({ actor_id: "p060", demo_identity_mode: true, role: "proposer", proposals: [], snapshot_version: "snap" }),
          { status: 200 },
        );
      if (url.includes("/workflow/proposals?role=approver"))
        return new Response(
          JSON.stringify({ actor_id: "p060", demo_identity_mode: true, role: "approver", proposals: [PIPE_PROPOSAL], snapshot_version: "snap" }),
          { status: 200 },
        );
      return new Response('{"demo_identity_mode":true,"error":"not found"}', { status: 404 });
    }),
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
  cleanup();
});

const AXE_OPTIONS = {
  runOnly: { type: "tag" as const, values: ["wcag2a", "wcag2aa"] },
};

async function expectNoViolations(container: Element, surface: string) {
  const results = await axe(container, AXE_OPTIONS);
  const summary = results.violations.map(
    (violation) => `${violation.id}: ${violation.nodes.length} node(s) — ${violation.help}`,
  );
  expect(summary, `${surface}: ${summary.join(" | ")}`).toEqual([]);
}

const THEMES = ["dark", "light"] as const;
function setTheme(theme: (typeof THEMES)[number]) {
  document.documentElement.setAttribute("data-theme", theme);
}

/** The K1 grounded envelope: answer + claims + disclosure counts. */
const GROUNDED: AnswerEnvelope = {
  ...(richEnvelope as AnswerEnvelope),
  answer: {
    citations: ["d0200"],
    claims: [
      { doc_id: "d0200", locator: "d0200@58", text: "Aggregate band-4 payroll commitment is £153,000 per annum." },
    ],
    text: "Aggregate band-4 payroll commitment is £153,000 per annum. [d0200]",
  },
  generation_applied: true,
  grounding: { admitted: 1, refused: 2 },
  grounding_applied: true,
};

const GRAPH: GraphResponse = {
  actor_id: "p060",
  center: { id: "org", label: "Bryremead Distribution Ltd" },
  departments: [
    { id: "Finance", label: "Finance", tint_key: "Finance" },
    { id: "IT", label: "IT", tint_key: "IT" },
  ],
  people: [
    { id: "p060", display_name: "Felix Osei", title: "Head of Finance", department_id: "Finance", avatar_ref: "", is_self: true, ring: "anchor" },
    { id: "p061", display_name: "Ana Flores", title: "Accounts Payable Clerk", department_id: "Finance", avatar_ref: "", is_self: false, ring: "member" },
    { id: "p074", display_name: "Yuki Moreau", title: "Head of IT", department_id: "IT", avatar_ref: "", is_self: false, ring: "anchor" },
  ],
  tools: [],
  sources: [{ id: "docstore", kind: "source", label: "Document store" }],
  projects: [],
  edges: [
    { from: "p060", kind: "member_of", to: "Finance" },
    { from: "p061", kind: "member_of", to: "Finance" },
    { from: "p074", kind: "member_of", to: "IT" },
    { from: "docstore", kind: "system_of", to: "org" },
  ],
  snapshot_version: "snap",
};

function stubAskFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.endsWith("/auth/login"))
        return new Response(
          JSON.stringify({ demo_identity_mode: true, principal_id: "p060", session_token: "t", expires_at: 0 }),
          { status: 200 },
        );
      if (url.endsWith("/ask")) return new Response(JSON.stringify(GROUNDED), { status: 200 });
      if (url.includes("/scope"))
        return new Response(
          JSON.stringify({
            demo_identity_mode: true,
            principal_id: "p060",
            scope_statement: { band: 5, groups: ["grp_finance"], sites: ["site_keldonbury"] },
          }),
          { status: 200 },
        );
      return new Response('{"demo_identity_mode":true,"error":"not found"}', { status: 404 });
    }),
  );
}

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

function stubUnavailableLedger() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response("producer down", { status: 503 })),
  );
}

for (const theme of THEMES) {
  describe(`B1 axe sweep — ${theme} theme`, () => {
    it(`Home (/) has zero wcag2a/aa violations [${theme}]`, async () => {
      setTheme(theme);
      const { container } = render(<ProductHome />);
      await expectNoViolations(container, `home/${theme}`);
    });

    it(`the cinematic entry screen (Showreel Track A) has zero violations [${theme}]`, async () => {
      setTheme(theme);
      const { container } = render(<EntryScreen onEnter={() => {}} />);
      await expectNoViolations(container, `entry/${theme}`);
    });

    it(`Ask with a GROUNDED answer has zero violations [${theme}]`, async () => {
      setTheme(theme);
      stubAskFetch();
      const { container } = render(<Console view="ask" />);
      // Pick the identity, ask, and wait for the grounded answer.
      fireEvent.change(screen.getByTestId("principal-search"), { target: { value: "p060" } });
      fireEvent.click(screen.getAllByTestId("principal-row").find((b) => b.textContent === "p060")!);
      fireEvent.change(screen.getByTestId("query-input"), {
        target: { value: "confidential financial statements" },
      });
      fireEvent.click(screen.getByTestId("ask-button"));
      await waitFor(() => expect(screen.getByTestId("answer-card")).toBeTruthy());
      expect(screen.getByTestId("badge-grounding")).toBeTruthy();
      expect(screen.getByTestId("grounding-removed-line")).toBeTruthy();
      await expectNoViolations(container, `ask-grounded/${theme}`);
    });

    it(`Operating Map (p060 fixture) has zero violations [${theme}]`, async () => {
      setTheme(theme);
      stubGraphFetch();
      const { container } = render(<GraphRoom actor="p060" />);
      await waitFor(() => expect(screen.getByTestId("graph-audit-panel")).toBeTruthy());
      await expectNoViolations(container, `map/${theme}`);
    });

    it(`My Access (identity-less prerender state) has zero violations [${theme}]`, async () => {
      setTheme(theme);
      stubGraphFetch();
      const { container } = render(<Console view="lens" />);
      await expectNoViolations(container, `my-access/${theme}`);
    });

    it(`the Pipeline projects room + OPEN detail drawer has zero violations [${theme}]`, async () => {
      setTheme(theme);
      stubPipelineFetch();
      const { container } = render(<ProjectSurface actor="p060" capabilityId="cap31" />);
      await waitFor(() => expect(screen.getByTestId("pipeline-board")).toBeTruthy());
      fireEvent.click(screen.getAllByTestId("pipeline-card")[0]);
      expect(screen.getByTestId("pipeline-drawer")).toBeTruthy();
      await expectNoViolations(container, `pipeline/${theme}`);
    });

    it(`the OPEN Settings drawer (Track A's shell modal) has zero violations [${theme}]`, async () => {
      setTheme(theme);
      vi.stubGlobal("fetch", vi.fn(async () => new Response('{"demo_identity_mode":true}', { status: 404 })));
      const { container } = render(<Console view="ask" />);
      fireEvent.click(screen.getByTestId("settings-open"));
      expect(screen.getByTestId("settings-drawer")).toBeTruthy();
      await expectNoViolations(container, `settings-drawer/${theme}`);
    });

    it(`Spend Ledger (honest STATE 3) has zero violations [${theme}]`, async () => {
      setTheme(theme);
      stubUnavailableLedger();
      const { container } = render(<BursarSurface />);
      await waitFor(() => expect(screen.getByTestId("bursar-unavailable")).toBeTruthy());
      await expectNoViolations(container, `ledger/${theme}`);
    });
  });
}
