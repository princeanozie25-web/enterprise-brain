/**
 * SHOWCASE-III (Showreel Track B) — the grounded-proposal console surfaces.
 * Fully offline: ProposalsPanel + PipelineBoard rendered against typed
 * fixtures with a stubbed fetch. Pins: the create flow's honest states
 * (drafted / honest-empty / 429), the "Proposed" watermark + refusal
 * disclosure, the S4 withheld-anchor render (marker, never content), the
 * approver-only live gate (deciding requires being THE approver), and the
 * materialized payload in the drawer.
 */
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import type { ProjectWorkflowResponse, WorkflowItem, WorkflowProposal } from "@/lib/api";
import { PipelineBoard } from "@/components/projects/PipelineBoard";
import { ProposalsPanel } from "@/components/projects/ProposalsPanel";

afterEach(() => {
  vi.unstubAllGlobals();
});

const PROV = {
  capability: { id: "cap31", name: "Access Review 31" },
  initiative: { id: "init03", name: "Strengthen Workforce Capability" },
  strategy: { id: "strat01", name: "Workforce Capability" },
  workflow: { id: "wf11", name: "Goods-In Verification 31" },
};

const PROPOSAL: WorkflowProposal = {
  proposal_id: "wfp_0001",
  proposer_id: "p060",
  capability_id: "cap31",
  approver_id: "p113",
  title: "Onboarding new hires",
  goal: "confidential financial statements",
  drafted_from: "Drafted by a model from documents p060 is authorized to see.",
  boxes: [
    {
      box_index: 0,
      stage: "Next",
      title: "Collect the signed statements",
      description: "Gather the confidential financial statements for the quarter.",
      anchors: [
        {
          visible: true,
          doc_id: "doc_fin_042",
          title: "Q3 Financial Statements",
          quote: "the confidential financial statements are filed quarterly",
          locator: "doc_fin_042@118",
        },
      ],
      sources_total: 1,
      sources_outside_view: 0,
    },
    {
      box_index: 1,
      stage: "Next",
      title: "Verify the retention schedule",
      description: "Check the retention schedule before distribution.",
      anchors: [{ visible: false }],
      sources_total: 1,
      sources_outside_view: 1,
    },
  ],
  grounding: { admitted: 2, refused: 1 },
  status: "pending",
  created_ordinal: 7,
  materialized: false,
  snapshot_version: "snap",
};

type FetchStub = (url: string, init?: RequestInit) => Response | null;

function stubFetch(handler: FetchStub) {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      calls.push({ url, init });
      const handled = handler(url, init);
      if (handled) return handled;
      return new Response('{"demo_identity_mode":true,"error":"not found"}', { status: 404 });
    }),
  );
  return calls;
}

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status });
}

function listResponse(role: string, proposals: WorkflowProposal[]): Response {
  return json({
    actor_id: "p060",
    demo_identity_mode: true,
    role,
    proposals,
    snapshot_version: "snap",
  });
}

function stubLists(mine: WorkflowProposal[], inbox: WorkflowProposal[]): FetchStub {
  return (url, init) => {
    const method = (init?.method ?? "GET").toUpperCase();
    if (method === "GET" && url.includes("/workflow/proposals?role=proposer"))
      return listResponse("proposer", mine);
    if (method === "GET" && url.includes("/workflow/proposals?role=approver"))
      return listResponse("approver", inbox);
    return null;
  };
}

// ===========================================================================
describe("WFP-1: the create flow — drafted, honest-empty, and rate-limited", () => {
  it("posts {capability_id, title, goal} and reports the drafted count + approver", async () => {
    const lists = stubLists([], []);
    const calls = stubFetch((url, init) => {
      const method = (init?.method ?? "GET").toUpperCase();
      if (method === "POST" && url.endsWith("/workflow/proposals"))
        return json({ demo_identity_mode: true, proposal: PROPOSAL, snapshot_version: "snap" });
      return lists(url, init);
    });
    render(<ProposalsPanel actor="p060" capabilityId="cap31" onMaterialized={() => {}} />);
    await waitFor(() => expect(screen.getByTestId("proposals-empty")).toBeTruthy());

    fireEvent.change(screen.getByTestId("proposal-create-title"), {
      target: { value: "Onboarding new hires" },
    });
    fireEvent.change(screen.getByTestId("proposal-create-goal"), {
      target: { value: "confidential financial statements" },
    });
    fireEvent.click(screen.getByTestId("proposal-create-submit"));

    await waitFor(() =>
      expect(screen.getByTestId("proposal-create-status").textContent).toContain(
        "Drafted 2 grounded steps — awaiting p113.",
      ),
    );
    const post = calls.find((c) => (c.init?.method ?? "GET") === "POST");
    expect(post).toBeTruthy();
    expect(JSON.parse(String(post!.init!.body))).toEqual({
      capability_id: "cap31",
      title: "Onboarding new hires",
      goal: "confidential financial statements",
    });
  });

  it("renders the honest empty verbatim: reason + refusal count + nothing written", async () => {
    const lists = stubLists([], []);
    stubFetch((url, init) => {
      const method = (init?.method ?? "GET").toUpperCase();
      if (method === "POST" && url.endsWith("/workflow/proposals"))
        return json({
          demo_identity_mode: true,
          generated: false,
          reason: "no plan could be grounded in your sources",
          grounding: { admitted: 0, refused: 3 },
          snapshot_version: "snap",
        });
      return lists(url, init);
    });
    render(<ProposalsPanel actor="p060" capabilityId="cap31" onMaterialized={() => {}} />);
    await waitFor(() => expect(screen.getByTestId("proposals-empty")).toBeTruthy());
    fireEvent.change(screen.getByTestId("proposal-create-title"), { target: { value: "Plan" } });
    fireEvent.change(screen.getByTestId("proposal-create-goal"), { target: { value: "goal" } });
    fireEvent.click(screen.getByTestId("proposal-create-submit"));
    await waitFor(() => {
      const note = screen.getByTestId("proposal-create-status").textContent ?? "";
      expect(note).toContain("no plan could be grounded in your sources");
      expect(note).toContain("3 draft steps could not be grounded");
      expect(note).toContain("Nothing was written.");
    });
  });

  it("surfaces the per-principal 429 as a calm limit line", async () => {
    const lists = stubLists([], []);
    stubFetch((url, init) => {
      const method = (init?.method ?? "GET").toUpperCase();
      if (method === "POST" && url.endsWith("/workflow/proposals"))
        return new Response('{"demo_identity_mode":true,"error":"rate limited"}', { status: 429 });
      return lists(url, init);
    });
    render(<ProposalsPanel actor="p060" capabilityId="cap31" onMaterialized={() => {}} />);
    await waitFor(() => expect(screen.getByTestId("proposals-empty")).toBeTruthy());
    fireEvent.change(screen.getByTestId("proposal-create-title"), { target: { value: "Plan" } });
    fireEvent.change(screen.getByTestId("proposal-create-goal"), { target: { value: "goal" } });
    fireEvent.click(screen.getByTestId("proposal-create-submit"));
    await waitFor(() =>
      expect(screen.getByTestId("proposal-create-status").textContent).toContain(
        "Generation limit reached",
      ),
    );
  });
});

// ===========================================================================
describe("WFP-2: the proposal render — watermark, disclosure, S4 anchors", () => {
  it("shows Proposed watermark, grounding + refusal disclosure, and anchors", async () => {
    stubFetch(stubLists([PROPOSAL], []));
    render(<ProposalsPanel actor="p060" capabilityId="cap31" onMaterialized={() => {}} />);
    await waitFor(() => expect(screen.getByTestId("proposal-card")).toBeTruthy());

    expect(screen.getByTestId("proposal-status-chip").textContent).toBe("Proposed");
    const grounding = screen.getByTestId("proposal-grounding-line").textContent ?? "";
    expect(grounding).toContain("2 steps grounded verbatim");
    expect(screen.getByTestId("proposal-refused-line").textContent).toContain(
      "1 draft step could not be grounded in your sources and were dropped",
    );

    // The visible anchor: mono doc chip + verbatim quote.
    expect(screen.getByTestId("proposal-anchor-chip").textContent).toBe("doc_fin_042");
    expect(screen.getByTestId("proposal-anchor-quote").textContent).toContain(
      "the confidential financial statements are filed quarterly",
    );
  });

  it("S4: a withheld anchor is a marker — no doc id, no quote, no locator", async () => {
    stubFetch(stubLists([PROPOSAL], []));
    render(<ProposalsPanel actor="p060" capabilityId="cap31" onMaterialized={() => {}} />);
    await waitFor(() => expect(screen.getByTestId("proposal-card")).toBeTruthy());
    const boxes = screen.getAllByTestId("proposal-box");
    const withheldBox = boxes[1];
    expect(within(withheldBox).getByTestId("proposal-anchor-withheld").textContent).toBe(
      "source outside your view",
    );
    expect(within(withheldBox).queryByTestId("proposal-anchor-chip")).toBeNull();
    expect(within(withheldBox).queryByTestId("proposal-anchor-quote")).toBeNull();
    expect(withheldBox.textContent).toContain("1 of 1 source is outside your view");
  });

  it("a pending proposal seen by the PROPOSER carries no decision buttons", async () => {
    stubFetch(stubLists([PROPOSAL], []));
    render(<ProposalsPanel actor="p060" capabilityId="cap31" onMaterialized={() => {}} />);
    await waitFor(() => expect(screen.getByTestId("proposal-card")).toBeTruthy());
    expect(screen.getByTestId("proposal-card").getAttribute("data-can-decide")).toBe("false");
    expect(screen.queryByTestId("proposal-gate-approve")).toBeNull();
    expect(screen.getByTestId("proposal-gate-note").textContent).toContain("Awaiting p113");
  });
});

// ===========================================================================
describe("WFP-3: the human gate — approver-only live decision, reload on 2xx", () => {
  it("the approver approves; the board reload fires only after the 2xx", async () => {
    const decided: WorkflowProposal = {
      ...PROPOSAL,
      status: "approved",
      decided_by: "p113",
      materialized: true,
    };
    let approved = false;
    const onMaterialized = vi.fn();
    stubFetch((url, init) => {
      const method = (init?.method ?? "GET").toUpperCase();
      if (method === "POST" && url.endsWith("/workflow/proposals/wfp_0001/approve")) {
        approved = true;
        return json({ demo_identity_mode: true, proposal: decided, snapshot_version: "snap" });
      }
      if (method === "GET" && url.includes("/workflow/proposals?role=proposer"))
        return listResponse("proposer", []);
      if (method === "GET" && url.includes("/workflow/proposals?role=approver"))
        return listResponse("approver", [approved ? decided : PROPOSAL]);
      return null;
    });

    render(<ProposalsPanel actor="p113" capabilityId="cap31" onMaterialized={onMaterialized} />);
    await waitFor(() => expect(screen.getByTestId("proposal-card")).toBeTruthy());
    expect(screen.getByTestId("proposal-card").getAttribute("data-can-decide")).toBe("true");

    fireEvent.click(screen.getByTestId("proposal-gate-approve"));
    await waitFor(() =>
      expect(screen.getByTestId("proposal-gate-feedback").textContent).toContain(
        "Approved. The steps are now real pipeline work.",
      ),
    );
    expect(onMaterialized).toHaveBeenCalledTimes(1);
    // The card re-renders server-derived: approved + materialized.
    expect(screen.getByTestId("proposal-status-chip").textContent).toBe("Approved · materialized");
  });

  it("deny records the decision and materializes nothing", async () => {
    const denied: WorkflowProposal = { ...PROPOSAL, status: "denied", decided_by: "p113" };
    let done = false;
    const onMaterialized = vi.fn();
    stubFetch((url, init) => {
      const method = (init?.method ?? "GET").toUpperCase();
      if (method === "POST" && url.endsWith("/workflow/proposals/wfp_0001/deny")) {
        done = true;
        return json({ demo_identity_mode: true, proposal: denied, snapshot_version: "snap" });
      }
      if (method === "GET" && url.includes("/workflow/proposals?role=proposer"))
        return listResponse("proposer", []);
      if (method === "GET" && url.includes("/workflow/proposals?role=approver"))
        return listResponse("approver", [done ? denied : PROPOSAL]);
      return null;
    });
    render(<ProposalsPanel actor="p113" capabilityId="cap31" onMaterialized={onMaterialized} />);
    await waitFor(() => expect(screen.getByTestId("proposal-card")).toBeTruthy());
    fireEvent.click(screen.getByTestId("proposal-gate-deny"));
    await waitFor(() =>
      expect(screen.getByTestId("proposal-gate-feedback").textContent).toContain(
        "Denied. Nothing was materialized.",
      ),
    );
    expect(onMaterialized).not.toHaveBeenCalled();
    expect(screen.getByTestId("proposal-status-chip").textContent).toBe("Denied");
  });
});

// ===========================================================================
describe("WFP-4: the payoff — materialized boxes carry their payload into the drawer", () => {
  it("a materialized Next item opens with description, anchors, and the S4 marker", async () => {
    const materializedItem: WorkflowItem = {
      capability_id: "cap31",
      dependencies: [],
      item_id: "wfp_0001#0",
      kind: "lane_box",
      owner_id: "p060",
      provenance: PROV,
      snapshot_version: "snap",
      status: "planned",
      title: "Collect the signed statements",
      description: "Gather the confidential financial statements for the quarter.",
      anchors: [
        {
          visible: true,
          doc_id: "doc_fin_042",
          quote: "the confidential financial statements are filed quarterly",
          locator: "doc_fin_042@118",
        },
        { visible: false },
      ],
      sources_outside_view: 1,
      proposal_id: "wfp_0001",
    };
    const workflow: ProjectWorkflowResponse = {
      actor_id: "p060",
      capability_id: "cap31",
      demo_identity_mode: true,
      items: [materializedItem],
      provenance: PROV,
      snapshot_version: "snap",
    };
    render(<PipelineBoard workflow={workflow} actor="p060" onReload={() => {}} />);
    // The materialized box lands in Next as an ordinary payload card.
    const card = screen.getByTestId("pipeline-card");
    fireEvent.click(card);
    const drawer = await screen.findByTestId("pipeline-drawer");
    expect(within(drawer).getByTestId("pipeline-drawer-description").textContent).toContain(
      "Gather the confidential financial statements",
    );
    const docs = within(drawer).getByTestId("pipeline-drawer-anchors");
    expect(docs.textContent).toContain("doc_fin_042 · doc_fin_042@118");
    expect(docs.textContent).toContain("the confidential financial statements are filed quarterly");
    expect(within(drawer).getByTestId("pipeline-drawer-anchor-withheld").textContent).toContain(
      "withheld",
    );
    expect(within(drawer).getByText(/From proposal/)).toBeTruthy();
  });
});
