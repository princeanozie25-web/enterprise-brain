/**
 * Lane-room tests U-24..U-28 (AP-6, v4a DISPLAY ONLY). Fully offline:
 * captured fixtures as typed literals plus one synthetic all-status lane;
 * fetch stubbed; no sockets. U-6 sweeps the new component on its own.
 */
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import type { InboxResponse, LaneResponse } from "@/lib/api";
import { LaneRoom } from "@/components/LaneRoom";
import {
  inboxP061,
  laneAllStatuses,
  laneP060,
  rollupFixture,
} from "./fixtures/lane_typed";

afterEach(() => {
  vi.unstubAllGlobals();
});

/** A stateful stub: accept/dismiss mutate what later fetches return. */
function stubLaneFetch(initial: { lane: LaneResponse; inbox: InboxResponse }) {
  const state = {
    lane: initial.lane,
    inbox: initial.inbox,
  };
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? "GET";
    if (url.endsWith("/lane") && method === "GET") {
      return new Response(JSON.stringify(state.lane), { status: 200 });
    }
    if (url.endsWith("/lane/inbox") && method === "GET") {
      return new Response(JSON.stringify(state.inbox), { status: 200 });
    }
    if (url.endsWith("/lane/rollup")) {
      return new Response(JSON.stringify(rollupFixture), { status: 200 });
    }
    if (url.includes("/lane/inbox/") && url.endsWith("/accept")) {
      const proposalId = url.split("/lane/inbox/")[1].split("/")[0];
      const accepted = state.inbox.proposals.find((p) => p.proposal_id === proposalId)!;
      state.inbox = {
        ...state.inbox,
        proposals: state.inbox.proposals.filter((p) => p.proposal_id !== proposalId),
      };
      state.lane = {
        ...state.lane,
        boxes: [
          ...state.lane.boxes,
          {
            blocked_by: [],
            blocks: [],
            box_id: `accepted_${proposalId}`,
            capability: { id: "cap18", name: "Capability: Accepted 18" },
            derived: false,
            effect_class: "read_only",
            evidence: [],
            honesty: { band: 4, groups: ["grp_finance"], sites: ["site_one"] },
            provenance: {
              initiative: { id: "init_x", name: "Initiative X" },
              strategy: { id: "strat_x", name: "Strategy X" },
              workflow: { id: "wf_x", name: "Workflow X" },
            },
            snapshot_version: state.lane.snapshot_version,
            sop_state: "current",
            status: "candidate",
            why: `Accepted from ${accepted.agent_id}: ${accepted.standing_query}`,
          },
        ],
      };
      return new Response("{\"demo_identity_mode\":true}", { status: 200 });
    }
    if (url.includes("/lane/inbox/") && url.endsWith("/dismiss")) {
      const proposalId = url.split("/lane/inbox/")[1].split("/")[0];
      state.inbox = {
        ...state.inbox,
        proposals: state.inbox.proposals.filter((p) => p.proposal_id !== proposalId),
      };
      return new Response("{\"demo_identity_mode\":true}", { status: 200 });
    }
    if (url.includes("/lane/box/") && url.endsWith("/status")) {
      return new Response("{\"demo_identity_mode\":true}", { status: 200 });
    }
    if (url.includes("/doc/")) {
      return new Response(
        JSON.stringify({
          document_id: "d0134",
          sensitivity: "internal",
          snippet: "snippet",
          title: "Customer Account",
        }),
        { status: 200 },
      );
    }
    return new Response("{\"demo_identity_mode\":true,\"error\":\"not found\"}", {
      status: 404,
    });
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

const emptyInbox: InboxResponse = {
  actor_id: "p060",
  proposals: [],
  snapshot_version: "synthetic",
};

// ---------------------------------------------------------------------------
// U-24 THE LANE
// ---------------------------------------------------------------------------

describe("U-24: the lane renders in status order with its furniture", () => {
  it("orders active, candidate, blocked, done; hides dismissed; carries headers and honesty", async () => {
    stubLaneFetch({ lane: laneAllStatuses, inbox: emptyInbox });
    render(<LaneRoom actor="p060" />);
    await waitFor(() => expect(screen.getAllByTestId("lane-box").length).toBe(4));

    expect(screen.getByTestId("derived-demo-header").textContent).toBe(
      "Derived assignments (demo)",
    );
    const statuses = screen
      .getAllByTestId("lane-box")
      .map((node) => node.getAttribute("data-status"));
    expect(statuses).toEqual(["active", "candidate", "blocked", "done"]);

    // Every box: breadcrumb, why, honesty footer.
    for (const box of screen.getAllByTestId("lane-box")) {
      expect(within(box).getByTestId("box-breadcrumb").textContent).toContain("›");
      expect(within(box).getByTestId("box-honesty").textContent).toContain("Scope: groups");
      expect(within(box).getByTestId("box-why").textContent!.length).toBeGreaterThan(0);
    }

    // The blocked box: the deviation line and nothing else of the kind.
    const blocked = screen
      .getAllByTestId("lane-box")
      .find((node) => node.getAttribute("data-status") === "blocked")!;
    expect(within(blocked).getByTestId("box-deviation").textContent).toBe(
      "Bound procedure superseded — awaiting effective version",
    );
    expect(within(blocked).queryByTestId("box-successor-link")).toBeNull();
    expect(within(blocked).queryByTestId("box-action-active")).toBeNull();

    // Dismissed appears only behind the quiet toggle.
    fireEvent.click(screen.getByTestId("toggle-dismissed"));
    await waitFor(() => expect(screen.getAllByTestId("lane-box").length).toBe(5));
    expect(
      screen
        .getAllByTestId("lane-box")
        .map((node) => node.getAttribute("data-status")),
    ).toEqual(["active", "candidate", "blocked", "done", "dismissed"]);

    // The "+N more" expander is the actor's own produce.
    const active = screen
      .getAllByTestId("lane-box")
      .find((node) => node.getAttribute("data-status") === "active")!;
    expect(within(active).getAllByTestId("box-evidence-row").length).toBe(3);
    expect(within(active).getByTestId("box-more").textContent).toBe("+2 more");
    fireEvent.click(within(active).getByTestId("box-more"));
    expect(within(active).getAllByTestId("box-evidence-row").length).toBe(5);
  });

  it("renders the captured p060 lane (8 derived candidates)", async () => {
    stubLaneFetch({ lane: laneP060, inbox: emptyInbox });
    render(<LaneRoom actor="p060" />);
    await waitFor(() =>
      expect(screen.getAllByTestId("lane-box").length).toBe(laneP060.boxes.length),
    );
    for (const box of laneP060.boxes) {
      expect(box.derived).toBe(true);
      expect(box.effect_class).toBe("read_only");
    }
  });
});

// ---------------------------------------------------------------------------
// U-25 EXPLAIN THIS BOX
// ---------------------------------------------------------------------------

describe("U-25: explain-this-box lights the provenance path", () => {
  it("capability deep-links into Atlas; other segments anchor in the lane", async () => {
    stubLaneFetch({ lane: laneAllStatuses, inbox: emptyInbox });
    render(<LaneRoom actor="p060" />);
    await waitFor(() => expect(screen.getAllByTestId("lane-box").length).toBe(4));

    const first = screen.getAllByTestId("lane-box")[0];
    // Unlit: plain text, no links.
    expect(within(first).queryByTestId("crumb-atlas")).toBeNull();

    fireEvent.click(within(first).getByTestId("box-explain"));
    const atlasLink = within(first).getByTestId("crumb-atlas");
    const capabilityId = laneAllStatuses.boxes.find((b) => b.status === "active")!
      .capability.id;
    expect(atlasLink.getAttribute("href")).toBe(
      `/atlas?cap=${capabilityId}&as=p060`,
    );
    const anchors = within(first).getAllByTestId("crumb-anchor");
    expect(anchors.length).toBe(3);
    for (const anchor of anchors) {
      expect(anchor.getAttribute("href")!.startsWith("#box-")).toBe(true);
    }
  });
});

// ---------------------------------------------------------------------------
// U-26 INBOX FLOW
// ---------------------------------------------------------------------------

describe("U-26: the inbox accept flow on fixtures", () => {
  it("accept materializes a candidate box; dismiss removes; endpoints exact", async () => {
    const fetchMock = stubLaneFetch({
      lane: { ...laneP060, actor_id: "p061" },
      inbox: inboxP061,
    });
    render(<LaneRoom actor="p061" />);
    await waitFor(() => expect(screen.getByTestId("inbox-strip")).toBeTruthy());
    const proposals = screen.getAllByTestId("inbox-proposal");
    expect(proposals.length).toBe(inboxP061.proposals.length);

    const firstId = inboxP061.proposals[0].proposal_id;
    fireEvent.click(within(proposals[0]).getByTestId("inbox-accept"));
    await waitFor(() =>
      expect(
        screen
          .getAllByTestId("lane-box")
          .some((node) => node.id === `box-accepted_${firstId}`),
      ).toBe(true),
    );
    const acceptCalls = fetchMock.mock.calls.filter((call) =>
      String(call[0]).endsWith("/accept"),
    );
    expect(acceptCalls.length).toBe(1);
    expect(String(acceptCalls[0][0])).toContain(`/lane/inbox/${firstId}/accept`);
    expect((acceptCalls[0][1] as RequestInit).method).toBe("POST");

    // Dismiss the next proposal: it leaves the strip; no box appears.
    const before = screen.getAllByTestId("lane-box").length;
    const secondId = inboxP061.proposals[1].proposal_id;
    fireEvent.click(
      within(screen.getAllByTestId("inbox-proposal")[0]).getByTestId("inbox-dismiss"),
    );
    await waitFor(() =>
      expect(screen.getAllByTestId("inbox-proposal").length).toBe(
        inboxP061.proposals.length - 2,
      ),
    );
    expect(screen.getAllByTestId("lane-box").length).toBe(before);
    const dismissCalls = fetchMock.mock.calls.filter((call) =>
      String(call[0]).endsWith("/dismiss"),
    );
    expect(String(dismissCalls[0][0])).toContain(`/lane/inbox/${secondId}/dismiss`);

    // A status change carries the strict body.
    const candidate = screen
      .getAllByTestId("lane-box")
      .find((node) => node.getAttribute("data-status") === "candidate")!;
    fireEvent.click(within(candidate).getByTestId("box-action-active"));
    await waitFor(() =>
      expect(
        fetchMock.mock.calls.some((call) => String(call[0]).endsWith("/status")),
      ).toBe(true),
    );
    const statusCall = fetchMock.mock.calls.find((call) =>
      String(call[0]).endsWith("/status"),
    )!;
    expect(JSON.parse(String((statusCall[1] as RequestInit).body))).toEqual({
      to: "active",
    });
  });
});

// ---------------------------------------------------------------------------
// U-27 ROLLUP
// ---------------------------------------------------------------------------

describe("U-27: the rollup holds the floor and the honesty statement", () => {
  it("sub-floor capabilities are absent; the statement is byte-exact; no names", async () => {
    stubLaneFetch({ lane: laneP060, inbox: emptyInbox });
    render(<LaneRoom actor="p060" />);
    await waitFor(() => expect(screen.getAllByTestId("lane-box").length).toBe(8));

    fireEvent.click(screen.getByTestId("rollup-toggle"));
    await waitFor(() => expect(screen.getByTestId("rollup-panel")).toBeTruthy());
    const rows = screen.getAllByTestId("rollup-row");
    expect(rows.length).toBe(rollupFixture.capabilities.length);

    // The live capture itself proves the floor: cap62 has 2 assignees,
    // cap54 has 3, cap21 has 4 — all ABSENT.
    const panel = screen.getByTestId("rollup-panel");
    for (const absent of ["cap62", "cap54", "cap21"]) {
      expect(
        rollupFixture.capabilities.some((row) => row.capability_id === absent),
      ).toBe(false);
      expect(within(panel).queryByText(absent)).toBeNull();
    }

    expect(screen.getByTestId("rollup-honesty").textContent).toBe(
      "This view shows assignment status by capability. It cannot see activity, time, load, or any individual.",
    );
    // No name renders anywhere: every cell is a capability id or a count
    // (cell-level check — concatenated textContent forges false tokens).
    for (const cell of panel.querySelectorAll("td, th")) {
      expect(cell.textContent ?? "").not.toMatch(/^p\d{3}$|^p_void$|^agent_/);
    }
  });
});

// ---------------------------------------------------------------------------
// U-28 NO AMBER
// ---------------------------------------------------------------------------

describe("U-28: the v4b door stays visibly shut", () => {
  it("no amber class, vocabulary, or styling exists anywhere in the lane", async () => {
    stubLaneFetch({ lane: laneAllStatuses, inbox: emptyInbox });
    const { container } = render(<LaneRoom actor="p060" />);
    await waitFor(() => expect(screen.getAllByTestId("lane-box").length).toBe(4));
    fireEvent.click(screen.getByTestId("toggle-dismissed"));
    await waitFor(() => expect(screen.getAllByTestId("lane-box").length).toBe(5));

    const html = container.innerHTML.toLowerCase();
    expect(html).not.toContain("amber");
    expect(html).not.toContain("side_effecting");
    expect(html).not.toContain("side-effecting");
    for (const box of [...laneAllStatuses.boxes, ...laneP060.boxes]) {
      expect(box.effect_class).toBe("read_only");
    }
  });
});
