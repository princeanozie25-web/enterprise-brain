"use client";

import { useEffect, useState, type ReactNode } from "react";
import * as api from "@/lib/api";
import type { RoleScopeSummary } from "@/lib/api";
import { TYPE } from "@/lib/tokens";
import { MotionPanel } from "./MotionPrimitives";

/**
 * AdminPreviewGate — an HONEST preview interstitial for the admin-domain
 * surfaces (Operating Map, Bursar Ledger Room).
 *
 * THIS IS NOT AN AUTHORIZATION BOUNDARY — the SERVER is. Since AUTH-2, /graph
 * and /node/summary are per-identity scoped server-side (structural + the
 * grant-reachable slice when that lands); this gate cannot widen or narrow
 * that. It exists only to set expectations before the surface opens, on one
 * explicit opt-in. The *_surface_allowed flag from /me/scope is a DERIVED
 * signal, not an enforced boundary for these surfaces.
 *
 * If a real deployment ever returns *_surface_allowed === true (the admin
 * role build), the gate renders the surface directly.
 */
type GatedSurface = "admin" | "bursar";

const SURFACE_COPY: Record<
  GatedSurface,
  { context: "admin" | "bursar"; eyebrow: string; heading: string; lead: string; note: string; cta: string }
> = {
  admin: {
    context: "admin",
    eyebrow: "Scoped to your access",
    heading: "Operating Map — scoped to your access",
    lead:
      "The Operating Map shows the part of the company org structure your Work Identity is permitted to see — not the whole company.",
    note:
      "Enforced now, server-side: this map and its node metadata are scoped per Work Identity. You see your own department, your reporting lines, and the agents you own; an identity with no standing sees an empty map. Structural visibility is enforced today; visibility that flows from one-off access grants is still being added.",
    cta: "Read-only. You see only your slice of the company.",
  },
  bursar: {
    context: "bursar",
    eyebrow: "Preview — not connected",
    heading: "Bursar Ledger Room — preview (not connected)",
    lead: "The Bursar Ledger Room is a placeholder for the future governed-spend room.",
    note:
      "No ledger data and no finance authority are connected in this build; your document access is separately permission-scoped on the server.",
    cta: "Read-only preview. Not connected in this build.",
  },
};

export function AdminPreviewGate({
  actor,
  surface,
  children,
}: {
  actor: string | null;
  surface: GatedSurface;
  children: ReactNode;
}) {
  const [roleScope, setRoleScope] = useState<RoleScopeSummary | null>(null);
  const [revealed, setRevealed] = useState(false);

  // Read role posture for THIS actor. NOTE: *_surface_allowed is a DERIVED
  // signal, not an enforced boundary for these surfaces in this build.
  useEffect(() => {
    setRevealed(false);
    if (actor === null) {
      setRoleScope(null);
      return;
    }
    let cancelled = false;
    api
      .getRoleScope(actor)
      .then((response) => {
        if (!cancelled) setRoleScope(response);
      })
      .catch(() => {
        if (!cancelled) setRoleScope(null);
      });
    return () => {
      cancelled = true;
    };
  }, [actor, surface]);

  const allowed =
    surface === "admin" ? roleScope?.admin_surface_allowed : roleScope?.bursar_surface_allowed;

  // Engine-flagged allowed (the future auth build), OR the viewer opened the preview.
  if (allowed === true || revealed) {
    return <>{children}</>;
  }

  const copy = SURFACE_COPY[surface];

  return (
    <main className="min-w-0 flex-1" data-testid={`admin-preview-gate-${surface}`}>
      <MotionPanel
        className="ap-hero mx-auto max-w-2xl rounded-2xl p-6 md:p-8"
        data-testid="admin-preview-gate"
      >
        <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
          {copy.eyebrow}
        </p>
        <h1
          className="ap-register-chrome mt-2"
          style={{ fontSize: TYPE.scale.xl, fontWeight: 700, lineHeight: TYPE.line.display }}
        >
          {copy.heading}
        </h1>
        <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.md, lineHeight: TYPE.line.body }}>
          {copy.lead}
        </p>

        <p
          className="ap-chip mt-4 rounded-2xl px-4 py-3"
          style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
          data-testid="admin-preview-gate-note"
        >
          {copy.note}
        </p>

        <div className="mt-5 flex flex-wrap items-center gap-3">
          <button
            type="button"
            className="ap-affordance-button ap-register-chrome min-h-11 rounded-full px-5 py-2.5"
            style={{ fontSize: TYPE.scale.sm, fontWeight: 700 }}
            onClick={() => setRevealed(true)}
            data-testid="admin-preview-gate-reveal"
          >
            Open preview
          </button>
          <p className="ap-soft" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            {copy.cta}
          </p>
        </div>
      </MotionPanel>
    </main>
  );
}
