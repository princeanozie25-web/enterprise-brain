/**
 * AR-1 console tests U-29..U-32 — the humanization layer's UI. Fully offline:
 * fetch is stubbed, fixtures are small typed literals carrying the new human
 * fields. The existing U-1..U-28 are unmodified.
 */
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import * as api from "@/lib/api";
import type { AtlasResponse, LensResponse, PersonCard } from "@/lib/api";
import { initialsOf, PersonAvatar, RoomActor } from "@/components/PersonAvatar";
import { AtlasRoom } from "@/components/AtlasRoom";
import { LensRoom } from "@/components/LensRoom";

afterEach(() => {
  vi.unstubAllGlobals();
});

const p060Card: PersonCard = {
  avatar_ref: "faces/p060.jpg",
  department_label: "Finance",
  display_name: "Felix Osei",
  id: "p060",
  title: "Head of Finance",
};

// A faithful slice of the generated p060 record (the frozen name was "Kerensa
// Pellbrook"; AR-1 regenerated it to "Felix Osei").
const lensHuman = {
  actor: p060Card,
  actor_id: "p060",
  agents: [],
  cross_lens: false,
  holdings: [
    {
      docs: [
        {
          also_via: [],
          document_id: "d0196",
          sensitivity: "confidential",
          title: "Notice: aggregate financial position",
        },
      ],
      reason: "REBAC:grp_finance",
      sentence: "You see this because you are in grp_finance.",
    },
  ],
  snapshot_version: "snap",
  subject: {
    band: 5,
    department: "Finance",
    groups: ["grp_finance"],
    id: "p060",
    kind: "human",
    name: "Felix Osei",
    sites: ["site_keldonbury"],
  },
  subject_human: {
    avatar_ref: "faces/p060.jpg",
    bio: "Head of Finance at Bryremead Distribution Ltd. Currently working across Pick Accuracy. Collaborative across teams; protective of focus time.",
    department_label: "Finance",
    display_name: "Felix Osei",
    id: "p060",
    location: "Keldonbury, UK",
    manages: ["Ahmed Abebe", "Amara Nguyen"],
    personality_tag: "ENFJ",
    projects: [
      {
        capability_id: "cap03",
        capability_name: "Capability: Pick Accuracy 03",
        initiative_name: "Standardise Resilient Cold Chain",
        role: "Lead",
        status: "Active",
        strategy_name: "Strategy: Resilient Cold Chain",
        workflow_name: "Workflow: Batch Release Review 03",
      },
    ],
    reports_to: "Ingrid Cohen",
    seniority: "Leadership",
    title: "Head of Finance",
    work_style: "Hybrid",
  },
} satisfies LensResponse;

const atlasHuman = {
  actor: p060Card,
  actor_id: "p060",
  snapshot_version: "snap",
  strategies: [],
} satisfies AtlasResponse;

function stubJson(routes: (url: string) => unknown | null) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const body = routes(String(input));
      if (body === null) {
        return new Response('{"demo_identity_mode":true,"error":"not found"}', { status: 404 });
      }
      return new Response(JSON.stringify(body), { status: 200 });
    }),
  );
}

// ---------------------------------------------------------------------------
// U-29 PERSON AVATAR — swap-ready img with a designed monogram fallback
// ---------------------------------------------------------------------------

describe("U-29: PersonAvatar resolves a face or falls back to a department monogram", () => {
  it("renders the face <img> by default, pointing at the swap-in path", () => {
    render(<PersonAvatar principalId="p001" displayName="Amara Chen" department="Finance" />);
    const img = screen.getByTestId("person-avatar-img") as HTMLImageElement;
    expect(img.getAttribute("src")).toBe("/faces/p001.jpg");
    expect(screen.queryByTestId("person-avatar-monogram")).toBeNull();
  });

  it("falls back to the monogram when the face errors (the existence check)", () => {
    render(<PersonAvatar principalId="p001" displayName="Amara Chen" department="Finance" />);
    fireEvent.error(screen.getByTestId("person-avatar-img"));
    const mono = screen.getByTestId("person-avatar-monogram");
    expect(mono.textContent).toBe("AC");
    // Tinted by department (drives the disc color, grouped + on-brand).
    expect(mono.getAttribute("data-department")).toBe("Finance");
    expect(screen.queryByTestId("person-avatar-img")).toBeNull();
  });

  it("monograms gracefully with no department and odd names", () => {
    render(<PersonAvatar principalId="p_void" displayName="Solène" department={null} />);
    fireEvent.error(screen.getByTestId("person-avatar-img"));
    const mono = screen.getByTestId("person-avatar-monogram");
    expect(mono.getAttribute("data-department")).toBe("");
    expect(mono.textContent).toBe("SO");
    expect(initialsOf("Felix Osei")).toBe("FO");
    expect(initialsOf("Madonna")).toBe("MA");
  });
});

// ---------------------------------------------------------------------------
// U-30 MASTHEAD HUMANIZATION — name, title, avatar, bio, projects; id evidence
// ---------------------------------------------------------------------------

describe("U-30: the Lens masthead shows the human, with the id still evidence", () => {
  it("renders display name + title + avatar + bio + projects, id in the mono register", async () => {
    stubJson((url) => {
      if (url.endsWith("/lens/p060")) return lensHuman;
      if (url.endsWith("/atlas")) return atlasHuman;
      return null;
    });
    render(<LensRoom actor="p060" />);
    await waitFor(() => expect(screen.getByTestId("masthead")).toBeTruthy());

    expect(screen.getByTestId("masthead-name").textContent).toBe("Felix Osei");
    expect(screen.getByTestId("masthead-title").textContent).toBe("Head of Finance");
    // The id is still evidence — shown small, in the mono register (U-13 law).
    const id = screen.getByTestId("masthead-id");
    expect(id.textContent).toBe("p060");
    expect(id.className).toContain("ap-register-evidence");
    // The masthead carries an avatar (face img in jsdom, since onError never fires).
    expect(within(screen.getByTestId("masthead")).getAllByTestId("person-avatar-img").length).toBe(1);

    expect(screen.getByTestId("masthead-bio").textContent).toContain("Head of Finance");
    expect(screen.getByTestId("masthead-reports-to").textContent).toBe("Reports to Ingrid Cohen");
    const project = screen.getByTestId("masthead-project");
    expect(project.textContent).toContain("Capability: Pick Accuracy 03");
    expect(project.textContent).toContain("Lead");
    expect(project.textContent).toContain("cap03");
  });
});

// ---------------------------------------------------------------------------
// U-31 ROOM ACTOR — the "viewing as" identity header (Atlas/Lane)
// ---------------------------------------------------------------------------

describe("U-31: rooms head with the actor's human identity", () => {
  it("renders the actor card in the Atlas room from the response's own card", async () => {
    stubJson((url) => (url.endsWith("/atlas") ? atlasHuman : null));
    render(<AtlasRoom actor="p060" />);
    await waitFor(() => expect(screen.getByTestId("room-actor")).toBeTruthy());
    expect(screen.getByTestId("room-actor-name").textContent).toBe("Felix Osei");
    expect(screen.getByTestId("room-actor-title").textContent).toBe("Head of Finance");
    expect(screen.getByTestId("room-actor-id").textContent).toBe("p060");
  });

  it("renders nothing when no humanization card is present", () => {
    render(<RoomActor card={null} />);
    expect(screen.queryByTestId("room-actor")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// U-32 THE ROSTER — /people returns org-structural cards, no holdings
// ---------------------------------------------------------------------------

describe("U-32: getPeople returns the org directory", () => {
  it("parses the roster cards and carries no document field", async () => {
    const roster: PersonCard[] = [
      p060Card,
      { avatar_ref: "faces/p001.jpg", department_label: "Quality & Compliance", display_name: "Samir Nakamura", id: "p001", title: "Head of Quality & Compliance" },
    ];
    stubJson((url) => (url.endsWith("/people") ? { demo_identity_mode: true, people: roster } : null));
    const people = await api.getPeople("p060");
    expect(people.length).toBe(2);
    expect(people[0].display_name).toBe("Felix Osei");
    expect(people[1].id).toBe("p001");
    // A card is org-structural: the keys are exactly the directory fields.
    expect(Object.keys(people[0]).sort()).toEqual(
      ["avatar_ref", "department_label", "display_name", "id", "title"].sort(),
    );
  });
});
