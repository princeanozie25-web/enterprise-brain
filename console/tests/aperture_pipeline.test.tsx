/**
 * SHOWCASE-2 — THE PIPELINE PROJECTS ROOM (PIPE-1..PIPE-9). Fully offline: the
 * PipelineBoard is rendered directly against typed workflow fixtures; the
 * ProjectSurface integration + the live human-gate decision use a stubbed fetch.
 * These SUPERSEDE the old WorkflowView suite (aperture_workflow.test.tsx,
 * removed — see closeout); the honest-empty / no-evidence / provenance laws
 * carry over, re-pinned to the pipeline form.
 */
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import type { ProjectWorkflowResponse, WorkflowItem } from "@/lib/api";
import { PipelineBoard } from "@/components/projects/PipelineBoard";
import { ProjectSurface } from "@/components/ProjectSurface";
import { STAGES } from "@/components/projects/pipeline";

afterEach(() => {
  vi.unstubAllGlobals();
});

const PROV = {
  capability: { id: "cap31", name: "Access Review 31" },
  initiative: { id: "init03", name: "Strengthen Workforce Capability" },
  strategy: { id: "strat01", name: "Workforce Capability" },
  workflow: { id: "wf11", name: "Goods-In Verification 31" },
};

function item(overrides: Partial<WorkflowItem> & Pick<WorkflowItem, "item_id" | "kind" | "status" | "title">): WorkflowItem {
  return {
    capability_id: "cap31",
    dependencies: [],
    provenance: PROV,
    snapshot_version: "snap",
    ...overrides,
  };
}

// A world with every column populated: active (outline), next, two pending
// access-requests (one the viewer approves, one it does not), a done item, and
// a blocked item with NO owner/deps (the missing-fields case).
const WORKFLOW: ProjectWorkflowResponse = {
  actor_id: "p060",
  capability_id: "cap31",
  demo_identity_mode: true,
  items: [
    item({ item_id: "box_active", kind: "lane_box", status: "active", title: "Verify goods-in batch 31", owner_id: "p060", dependencies: ["box_seed"] }),
    item({ item_id: "box_next", kind: "lane_box", status: "planned", title: "Prepare batch 32", owner_id: "p060" }),
    item({ item_id: "ar_mine", kind: "access_request", status: "pending", title: "Access request for Access Review 31", approver_id: "p060", requester_id: "p074" }),
    item({ item_id: "ar_other", kind: "access_request", status: "pending", title: "Access request for cap31 (other approver)", approver_id: "p001", requester_id: "p060" }),
    item({ item_id: "box_done", kind: "accepted_agent_box", status: "done", title: "Review accepted agent proposal", owner_id: "p060", agent_id: "agent_finance_analyst", dependencies: ["box_active"] }),
    item({ item_id: "box_blocked", kind: "lane_box", status: "blocked", title: "Blocked reconciliation" }),
  ],
  provenance: PROV,
  snapshot_version: "snap",
};

function renderBoard(overrides: Partial<React.ComponentProps<typeof PipelineBoard>> = {}) {
  return render(
    <PipelineBoard workflow={WORKFLOW} actor="p060" onReload={() => {}} {...overrides} />,
  );
}
function cardById(id: string): HTMLElement {
  return screen.getAllByTestId("pipeline-card").find((el) => el.getAttribute("data-item-id") === id)!;
}
function columnByStage(stage: string): HTMLElement {
  return screen.getAllByTestId("pipeline-column").find((el) => el.getAttribute("data-stage") === stage)!;
}

// ===========================================================================
describe("PIPE-1: the stage-column layout law (320 / gap 48, STAGE 0N headers)", () => {
  it("renders one column per stage at the reference geometry, in flow order", () => {
    renderBoard();
    const columns = screen.getAllByTestId("pipeline-column");
    expect(columns.length).toBe(STAGES.length);
    for (const column of columns) {
      expect(column.style.width).toBe("320px");
    }
    expect(screen.getByTestId("pipeline-columns").style.gap).toBe("48px");
    // Flow order: Next -> In Progress -> Waiting(gate) -> Blocked -> Done.
    expect(columns.map((c) => c.getAttribute("data-stage"))).toEqual([
      "next",
      "in_progress",
      "waiting",
      "blocked",
      "done",
    ]);
    expect(screen.getAllByTestId("pipeline-stage-eyebrow").map((e) => e.textContent)).toEqual([
      "STAGE 01",
      "STAGE 02",
      "STAGE 03",
      "STAGE 04",
      "STAGE 05",
    ]);
    expect(screen.getAllByTestId("pipeline-stage-name").map((n) => n.textContent)).toEqual([
      "Next",
      "In Progress",
      "Waiting",
      "Blocked",
      "Done",
    ]);
  });
});

describe("PIPE-2: columns are the payload status set; items land in the right column", () => {
  it("maps each status to its stage and honours empty stages with an honest line", () => {
    renderBoard();
    expect(within(columnByStage("in_progress")).getAllByTestId("pipeline-card").length).toBe(1);
    expect(within(columnByStage("in_progress")).getByTestId("pipeline-card").getAttribute("data-item-id")).toBe("box_active");
    expect(within(columnByStage("next")).getByTestId("pipeline-card").getAttribute("data-item-id")).toBe("box_next");
    expect(within(columnByStage("done")).getByTestId("pipeline-card").getAttribute("data-item-id")).toBe("box_done");
    expect(within(columnByStage("blocked")).getByTestId("pipeline-card").getAttribute("data-item-id")).toBe("box_blocked");
    // Waiting holds the two access-requests as human-gate panels (not cards).
    expect(within(columnByStage("waiting")).getAllByTestId("pipeline-human-gate").length).toBe(2);
  });

  it("an empty stage renders the honest line, never a placeholder card", () => {
    const empty: ProjectWorkflowResponse = { ...WORKFLOW, items: [WORKFLOW.items[0]] };
    render(<PipelineBoard workflow={empty} actor="p060" onReload={() => {}} />);
    // Only In Progress has an item; the other four show "Nothing in this stage."
    expect(screen.getAllByTestId("pipeline-column-empty").length).toBe(4);
    expect(screen.getAllByTestId("pipeline-column-empty")[0].textContent).toContain("Nothing in this stage.");
  });

  it("a truly empty payload renders one honest line, no columns of placeholders", () => {
    const none: ProjectWorkflowResponse = { ...WORKFLOW, items: [] };
    render(<PipelineBoard workflow={none} actor="p060" onReload={() => {}} />);
    expect(screen.getByTestId("pipeline-empty").textContent).toContain("No work items");
    expect(screen.queryAllByTestId("pipeline-card").length).toBe(0);
  });
});

describe("PIPE-3: card fields are payload fields — absent fields render nothing", () => {
  it("a full card shows title, mono item_id, kind, and the owner avatar chip", () => {
    renderBoard();
    const card = cardById("box_active");
    expect(within(card).getByTestId("pipeline-card-title").textContent).toBe("Verify goods-in batch 31");
    expect(within(card).getByTestId("pipeline-card-id").textContent).toBe("box_active");
    expect(within(card).getByTestId("pipeline-card-actor").textContent).toContain("owner p060");
    expect(within(card).getByTestId("pipeline-card-deps").textContent).toContain("1 dep");
  });

  it("a card missing owner/deps omits those elements (no invented description/timestamp)", () => {
    renderBoard();
    const card = cardById("box_blocked");
    expect(within(card).queryByTestId("pipeline-card-actor")).toBeNull();
    expect(within(card).queryByTestId("pipeline-card-deps")).toBeNull();
  });

  it("never leaks non-payload data (evidence rows, document ids, timestamps, fake metrics)", () => {
    const { container } = renderBoard();
    const text = container.textContent ?? "";
    expect(text).not.toContain("document_id");
    expect(text).not.toMatch(/evidence|unread|notification|created_at|updated_at|\bmetric\b|fake/i);
  });
});

describe("PIPE-4: the active-path law — amber outline, and NO spline (no history)", () => {
  it("only 'active' items carry the amber outline; nothing else does", () => {
    renderBoard();
    const active = cardById("box_active");
    expect(active.getAttribute("data-active")).toBe("true");
    expect(active.getAttribute("style") ?? "").toContain("var(--accent-warm)");
    const next = cardById("box_next");
    expect(next.getAttribute("data-active")).toBe("false");
    expect(next.getAttribute("style") ?? "").not.toContain("var(--accent-warm)");
  });

  it("draws NO cross-stage spline — the payload has no route history", () => {
    renderBoard();
    // Real guard (not a nonexistent-testid check): the active path is outline-
    // only, so inside the columns region the ONLY vector shapes are the done-
    // check badges. Any connector spline between cards would be a path/line
    // here and fail. The constellation lives OUTSIDE the columns (a board-level
    // sibling), so it is not counted; PersonAvatar emits no path/line.
    const columns = screen.getByTestId("pipeline-columns");
    const vectors = Array.from(columns.querySelectorAll("path, line"));
    expect(vectors.length).toBeGreaterThan(0); // the done badge(s) exist
    for (const vector of vectors) {
      expect(vector.closest('[data-testid="pipeline-card-done"]')).not.toBeNull();
    }
  });
});

describe("PIPE-5: the human gate — live buttons for the approver, chips for everyone else", () => {
  it("the viewer's own pending access-request gets LIVE approve/reject; a 2xx refetches", async () => {
    const onReload = vi.fn();
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.includes("/access-requests/ar_mine/approve") && init?.method === "POST") {
        return new Response(JSON.stringify({ demo_identity_mode: true, request: {}, snapshot_version: "snap" }), { status: 200 });
      }
      return new Response("{}", { status: 404 });
    });
    vi.stubGlobal("fetch", fetchMock);

    renderBoard({ onReload });
    const gate = screen.getAllByTestId("pipeline-human-gate").find((g) => g.getAttribute("data-item-id") === "ar_mine")!;
    expect(gate.getAttribute("data-can-decide")).toBe("true");
    const approve = within(gate).getByTestId("pipeline-gate-approve");
    expect(within(gate).getByTestId("pipeline-gate-reject")).toBeTruthy();
    fireEvent.click(approve);
    await waitFor(() =>
      expect(fetchMock.mock.calls.some((c) => String(c[0]).includes("/access-requests/ar_mine/approve"))).toBe(true),
    );
    await waitFor(() => expect(onReload).toHaveBeenCalled());
    await waitFor(() =>
      expect(within(gate).getByTestId("pipeline-gate-feedback").textContent).toContain("Approved"),
    );
  });

  it("Reject denies via POST .../deny (never /approve), refetches, and shows the 'Rejected' line", async () => {
    const onReload = vi.fn();
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.includes("/access-requests/ar_mine/deny") && init?.method === "POST") {
        return new Response(JSON.stringify({ demo_identity_mode: true, request: {}, snapshot_version: "snap" }), { status: 200 });
      }
      return new Response("{}", { status: 404 });
    });
    vi.stubGlobal("fetch", fetchMock);

    renderBoard({ onReload });
    const gate = screen.getAllByTestId("pipeline-human-gate").find((g) => g.getAttribute("data-item-id") === "ar_mine")!;
    fireEvent.click(within(gate).getByTestId("pipeline-gate-reject"));
    await waitFor(() =>
      expect(fetchMock.mock.calls.some((c) => String(c[0]).includes("/access-requests/ar_mine/deny"))).toBe(true),
    );
    expect(fetchMock.mock.calls.some((c) => String(c[0]).includes("/approve"))).toBe(false);
    await waitFor(() => expect(onReload).toHaveBeenCalled());
    await waitFor(() =>
      expect(within(gate).getByTestId("pipeline-gate-feedback").textContent).toContain("Rejected"),
    );
  });

  it("a non-2xx decision does NOT refetch and shows the honest error line (state changes only on 2xx)", async () => {
    const onReload = vi.fn();
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/access-requests/ar_mine/approve")) {
        return new Response(JSON.stringify({ error: "denied by server" }), { status: 500 });
      }
      return new Response("{}", { status: 404 });
    });
    vi.stubGlobal("fetch", fetchMock);

    renderBoard({ onReload });
    const gate = screen.getAllByTestId("pipeline-human-gate").find((g) => g.getAttribute("data-item-id") === "ar_mine")!;
    fireEvent.click(within(gate).getByTestId("pipeline-gate-approve"));
    await waitFor(() =>
      expect(within(gate).getByTestId("pipeline-gate-feedback").textContent).toContain("was not recorded"),
    );
    // The money-law: no 2xx -> no refetch, no implied success.
    expect(onReload).not.toHaveBeenCalled();
  });

  it("a pending access-request the viewer does NOT approve shows a chip + the quiet line, no buttons", () => {
    renderBoard();
    const gate = screen.getAllByTestId("pipeline-human-gate").find((g) => g.getAttribute("data-item-id") === "ar_other")!;
    expect(gate.getAttribute("data-can-decide")).toBe("false");
    expect(within(gate).queryByTestId("pipeline-gate-approve")).toBeNull();
    expect(within(gate).queryByTestId("pipeline-gate-reject")).toBeNull();
    expect(within(gate).getByTestId("pipeline-gate-chip")).toBeTruthy();
    expect(within(gate).getByTestId("pipeline-gate-note").textContent).toContain("recorded in the Review Queue");
  });

  it("no dead controls: every gate button has a decision handler (approve/reject only when decidable)", () => {
    renderBoard();
    // ar_other (not decidable) exposes ONLY a Details button — no approve/reject.
    const other = screen.getAllByTestId("pipeline-human-gate").find((g) => g.getAttribute("data-item-id") === "ar_other")!;
    const buttons = within(other).getAllByRole("button");
    expect(buttons.map((b) => b.getAttribute("data-testid"))).toEqual(["pipeline-gate-details"]);
  });
});

describe("PIPE-6: the detail drawer renders every payload field and admits its bounds", () => {
  it("opens on a card, shows facts/provenance/people/deps, and the honest no-documents line", () => {
    renderBoard();
    fireEvent.click(cardById("box_done"));
    const drawer = screen.getByTestId("pipeline-drawer");
    expect(drawer.getAttribute("role")).toBe("dialog");
    expect(within(drawer).getByTestId("pipeline-drawer-facts").textContent).toContain("box_done");
    expect(within(drawer).getByTestId("pipeline-drawer-provenance").textContent).toContain("Goods-In Verification 31");
    expect(within(drawer).getByTestId("pipeline-drawer-people").textContent).toContain("agent · agent_finance_analyst");
    expect(within(drawer).getByTestId("pipeline-drawer-deps").textContent).toContain("box_active");
    // No doc-ref chips exist because the payload carries none — said plainly.
    expect(within(drawer).getByTestId("pipeline-drawer-documents").textContent).toContain("no document references");
    // Escape closes and returns.
    fireEvent.keyDown(drawer, { key: "Escape" });
    expect(screen.queryByTestId("pipeline-drawer")).toBeNull();
  });

  it("empty sections are stated, never faked (a bare item admits no people, no deps)", () => {
    renderBoard();
    fireEvent.click(cardById("box_blocked"));
    const drawer = screen.getByTestId("pipeline-drawer");
    expect(within(drawer).getByTestId("pipeline-drawer-people").textContent).toContain("No people");
    expect(within(drawer).getByTestId("pipeline-drawer-deps").textContent).toContain("No dependencies");
  });
});

describe("PIPE-7: the constellation is decoration, not data", () => {
  it("is aria-hidden, non-interactive, and never node-like (no circle above 3px)", () => {
    const { container } = renderBoard();
    const decoration = screen.getByTestId("pipeline-constellation");
    expect(decoration.getAttribute("aria-hidden")).toBe("true");
    expect(decoration.getAttribute("class") ?? "").toContain("pointer-events-none");
    expect(decoration.querySelector("[data-id]")).toBeNull();
    for (const circle of Array.from(container.querySelectorAll('[data-testid="pipeline-constellation"] circle'))) {
      expect(Number(circle.getAttribute("r"))).toBeLessThanOrEqual(3);
    }
  });
});

describe("PIPE-8: keyboard — arrows move column focus, Tab reaches the cards", () => {
  it("ArrowRight/Left walk the columns; a card is a real button", () => {
    renderBoard();
    const next = columnByStage("next");
    next.focus();
    expect(document.activeElement).toBe(next);
    fireEvent.keyDown(next, { key: "ArrowRight" });
    expect(document.activeElement).toBe(columnByStage("in_progress"));
    fireEvent.keyDown(columnByStage("in_progress"), { key: "ArrowLeft" });
    expect(document.activeElement).toBe(next);
    // Cards are buttons (Tab-reachable, Enter/click activatable).
    expect(cardById("box_active").tagName).toBe("BUTTON");
  });

  it("arrowing a FOCUSED card does not steal focus to a sibling column", () => {
    renderBoard();
    const card = cardById("box_active");
    card.focus();
    expect(document.activeElement).toBe(card);
    // The keydown bubbles to the column, but the target-guard leaves it alone.
    fireEvent.keyDown(card, { key: "ArrowRight" });
    expect(document.activeElement).toBe(card);
  });
});

// ===========================================================================
// ProjectSurface integration + the honest entry states (carried from the old
// WorkflowView suite, re-pinned to the pipeline).
// ===========================================================================
function stubProjectFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/workflow/project/cap31")) return new Response(JSON.stringify(WORKFLOW), { status: 200 });
      if (url.endsWith("/graph"))
        return new Response(
          JSON.stringify({ actor_id: "p060", center: { id: "org", label: "Org" }, departments: [], edges: [], people: [], projects: [{ departments: ["Finance"], id: "cap31", initiative_name: "Strengthen Workforce Capability", label: "Capability: Access Review 31", people: 2, primary_department_id: "Finance", status_counts: { Active: 1 }, strategy_name: "Workforce Capability", workflow_name: "Goods-In Verification 31" }], snapshot_version: "snap", sources: [], tools: [] }),
          { status: 200 },
        );
      return new Response('{"demo_identity_mode":true,"error":"not found"}', { status: 404 });
    }),
  );
}

describe("PIPE-9: ProjectSurface hosts the pipeline and keeps its honest entry states", () => {
  it("renders the pipeline in the Projects tab", async () => {
    stubProjectFetch();
    render(<ProjectSurface actor="p060" capabilityId="cap31" />);
    await waitFor(() => expect(screen.getByTestId("project-title").textContent).toBe("Access Review 31"));
    expect(screen.getByTestId("pipeline-board")).toBeTruthy();
    expect(screen.getAllByTestId("pipeline-column").length).toBe(5);
    // The Operating Map Trace tab still exists and swaps in.
    fireEvent.click(screen.getAllByTestId("project-tab")[0]);
    expect(screen.getByTestId("project-graph-view")).toBeTruthy();
    expect(screen.queryByTestId("pipeline-board")).toBeNull();
  });

  it("no actor / no capability: honest entry states, never fabricated pipeline data", () => {
    const { container: c1 } = render(<ProjectSurface actor={null} capabilityId="cap31" />);
    expect(screen.getByTestId("project-empty")).toBeTruthy();
    expect(c1.querySelector("[data-testid='pipeline-card']")).toBeNull();

    const { container: c2 } = render(<ProjectSurface actor="p060" capabilityId={null} />);
    expect(screen.getByTestId("project-missing-capability").textContent).toContain("does not fabricate project state");
    expect(c2.querySelector("[data-testid='pipeline-board']")).toBeNull();
  });
});
