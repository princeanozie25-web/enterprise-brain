/**
 * Evidence-export tests U-22..U-23 (AP-5). Fully offline: fetch stubbed,
 * object URLs stubbed; the assertions are about the affordance's four
 * homes, the PARAMS-ONLY bodies it sends, and the filename law. U-6 sweeps
 * the new component for color literals on its own.
 */
import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import * as api from "@/lib/api";
import { AtlasRoom } from "@/components/AtlasRoom";
import { Console } from "@/components/Console";
import { DiffView } from "@/components/DiffView";
import { LensRoom } from "@/components/LensRoom";
import { atlasP060 } from "./fixtures/atlas_typed";
import { diffP016P087 } from "./fixtures/diff_typed";
import { lensP060Self } from "./fixtures/lens_typed";
import { richEnvelope } from "./fixtures/typed";

beforeEach(() => {
  URL.createObjectURL = vi.fn(() => "blob:test");
  URL.revokeObjectURL = vi.fn();
});

afterEach(() => {
  vi.unstubAllGlobals();
  window.history.replaceState(null, "", "/");
});

function stubFetch() {
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.endsWith("/export")) {
      return new Response(new Uint8Array([0x25, 0x50, 0x44, 0x46]), {
        status: 200,
        headers: { "content-type": "application/pdf" },
      });
    }
    if (url.endsWith("/lens/p060")) {
      return new Response(JSON.stringify(lensP060Self), { status: 200 });
    }
    if (url.includes("/lens/diff?")) {
      return new Response(JSON.stringify(diffP016P087), { status: 200 });
    }
    if (url.endsWith("/atlas")) {
      return new Response(JSON.stringify(atlasP060), { status: 200 });
    }
    if (url.endsWith("/access-grants/ag_123")) {
      return new Response(
        JSON.stringify({
          demo_identity_mode: true,
          grant: {
            approver_id: "p001",
            created_ordinal: 0,
            grant_id: "ag_123",
            grantee_id: "p060",
            permission: "read",
            reason: "manager_approved",
            request_id: "ar_approved",
            snapshot_version: "snap",
            status: "active",
            target: { kind: "project", capability_id: "cap31" },
          },
          snapshot_version: "snap",
        }),
        { status: 200 },
      );
    }
    if (url.endsWith("/ask")) {
      return new Response(JSON.stringify(richEnvelope), { status: 200 });
    }
    if (url.includes("/scope")) {
      return new Response(
        JSON.stringify({
          demo_identity_mode: true,
          principal_id: "p060",
          scope_statement: { band: null, groups: [], sites: [] },
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

function exportBodies(fetchMock: ReturnType<typeof vi.fn>): unknown[] {
  return fetchMock.mock.calls
    .filter((call) => String(call[0]).endsWith("/export"))
    .map((call) => JSON.parse(String((call[1] as RequestInit).body)));
}

function askBodies(fetchMock: ReturnType<typeof vi.fn>): unknown[] {
  return fetchMock.mock.calls
    .filter((call) => String(call[0]).endsWith("/ask") && (call[1] as RequestInit | undefined)?.method === "POST")
    .map((call) => JSON.parse(String((call[1] as RequestInit).body)));
}

// ---------------------------------------------------------------------------
// U-22 THE FOUR HOMES
// ---------------------------------------------------------------------------

describe("U-22: the affordance lives in four homes and sends params only", () => {
  it("Lens room header: disabled while loading, then params-only body", async () => {
    const fetchMock = stubFetch();
    render(<LensRoom actor="p060" />);
    const button = screen.getByTestId("export-evidence");
    expect((button as HTMLButtonElement).disabled).toBe(true);

    await waitFor(() => expect(screen.getByTestId("masthead")).toBeTruthy());
    expect((screen.getByTestId("export-evidence") as HTMLButtonElement).disabled).toBe(false);

    fireEvent.click(screen.getByTestId("export-evidence"));
    await waitFor(() => expect(exportBodies(fetchMock).length).toBe(1));
    expect(exportBodies(fetchMock)[0]).toEqual({
      view: "lens",
      lens: { subject_id: "p060" },
    });
    expect(URL.createObjectURL).toHaveBeenCalledTimes(1);
  });

  it("Diff header: disabled while loading, then params-only body", async () => {
    const fetchMock = stubFetch();
    render(
      <DiffView actor="p016" left="p016" right="p087" onClose={() => {}} onOpenDoc={() => {}} />,
    );
    expect((screen.getByTestId("export-evidence") as HTMLButtonElement).disabled).toBe(true);

    await waitFor(() => expect(screen.getByTestId("diff-audited-line")).toBeTruthy());
    expect((screen.getByTestId("export-evidence") as HTMLButtonElement).disabled).toBe(false);

    fireEvent.click(screen.getByTestId("export-evidence"));
    await waitFor(() => expect(exportBodies(fetchMock).length).toBe(1));
    expect(exportBodies(fetchMock)[0]).toEqual({
      view: "diff",
      diff: { left: "p016", right: "p087" },
    });
  });

  it("Atlas capability sheet: enabled with the sheet, params-only body", async () => {
    const fetchMock = stubFetch();
    render(<AtlasRoom actor="p060" />);
    await waitFor(() => expect(screen.getAllByTestId("strategy-band").length).toBe(6));

    const capabilities = atlasP060.strategies.flatMap((s) =>
      s.initiatives.flatMap((i) => i.workflows.flatMap((w) => w.capabilities)),
    );
    const targetIndex = capabilities.findIndex((c) => c.docs.length > 0);
    const cards = screen.getAllByTestId("capability-card");
    fireEvent.click(within(cards[targetIndex]).getByTestId("capability-open"));
    const sheet = await screen.findByTestId("capability-sheet");

    const button = within(sheet).getByTestId("export-evidence") as HTMLButtonElement;
    expect(button.disabled).toBe(false);
    fireEvent.click(button);
    await waitFor(() => expect(exportBodies(fetchMock).length).toBe(1));
    expect(exportBodies(fetchMock)[0]).toEqual({
      view: "atlas_capability",
      atlas_capability: { capability_id: capabilities[targetIndex].id },
    });
  });

  it("Ask answer card: present only with an envelope, names the SUBMITTED params", async () => {
    const fetchMock = stubFetch();
    render(<Console view="ask" />);
    fireEvent.change(screen.getByTestId("principal-search"), { target: { value: "p060" } });
    fireEvent.click(
      screen.getAllByTestId("principal-row").find((b) => b.textContent === "p060")!,
    );
    // No envelope yet: the affordance does not exist in this home.
    expect(screen.queryByTestId("export-evidence")).toBeNull();

    fireEvent.change(screen.getByTestId("query-input"), {
      target: { value: "payroll aggregate" },
    });
    fireEvent.click(screen.getByTestId("toggle-hybrid"));
    fireEvent.click(screen.getByTestId("ask-button"));
    await waitFor(() => expect(screen.getByTestId("export-evidence")).toBeTruthy());
    expect((screen.getByTestId("export-evidence") as HTMLButtonElement).disabled).toBe(false);

    fireEvent.click(screen.getByTestId("export-evidence"));
    await waitFor(() => expect(exportBodies(fetchMock).length).toBe(1));
    expect(exportBodies(fetchMock)[0]).toEqual({
      view: "ask",
      ask: { query: "payroll aggregate", hybrid: true, judge: false },
    });
  });

  it("Ask entry door carries validated grant context to the existing ask endpoint", async () => {
    const fetchMock = stubFetch();
    window.history.pushState({}, "", "/ask?as=p060&grant=ag_123&cap=cap31");
    render(<Console view="ask" />);

    await waitFor(() => expect(screen.getByTestId("ask-granted-context").textContent).toContain("active"));
    expect(screen.getByTestId("ask-granted-context").textContent).toContain("grant ag_123");
    expect(screen.getByTestId("ask-granted-context").textContent).toContain("capability cap31");

    fireEvent.change(screen.getByTestId("query-input"), {
      target: { value: "summarise this granted capability" },
    });
    fireEvent.click(screen.getByTestId("ask-button"));

    await waitFor(() => expect(askBodies(fetchMock).length).toBe(1));
    expect(askBodies(fetchMock)[0]).toEqual({
      query: "summarise this granted capability",
      hybrid: false,
      judge: false,
      grant_id: "ag_123",
      capability_id: "cap31",
    });
  });
});

// ---------------------------------------------------------------------------
// U-23 FILENAMES
// ---------------------------------------------------------------------------

describe("U-23: filename construction matches the fixtures", () => {
  it("subject, pair, capability, and queryhash8 slugs with snapshot8", () => {
    const lens8 = lensP060Self.snapshot_version.slice(0, 8);
    expect(api.exportFilename("lens", "p060", lensP060Self.snapshot_version)).toBe(
      `aperture-lens-p060-${lens8}.pdf`,
    );

    const diff8 = diffP016P087.snapshot_version.slice(0, 8);
    expect(api.exportFilename("diff", "p016-p087", diffP016P087.snapshot_version)).toBe(
      `aperture-diff-p016-p087-${diff8}.pdf`,
    );

    const atlas8 = atlasP060.snapshot_version.slice(0, 8);
    expect(
      api.exportFilename("atlas_capability", "cap01", atlasP060.snapshot_version),
    ).toBe(`aperture-atlas_capability-cap01-${atlas8}.pdf`);

    const hash8 = richEnvelope.query_hash.slice(0, 8);
    const ask8 = richEnvelope.snapshot_version.slice(0, 8);
    expect(api.exportFilename("ask", hash8, richEnvelope.snapshot_version)).toBe(
      `aperture-ask-${hash8}-${ask8}.pdf`,
    );
    expect(hash8).toHaveLength(8);
    expect(ask8).toHaveLength(8);
  });
});
