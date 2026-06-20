"use client";

import { useEffect, useState, type ReactNode } from "react";
import * as api from "@/lib/api";
import type { RoleScopeSummary } from "@/lib/api";
import { TYPE } from "@/lib/tokens";
import { MotionPanel } from "./MotionPrimitives";
import { DemoIdentityNotice } from "./TrustPosture";

/**
 * AdminPreviewGate — an HONEST preview interstitial for the admin-domain
 * surfaces (Operating Map, Bursar Ledger Room).
 *
 * THIS IS NOT AN AUTHORIZATION BOUNDARY. The engine does NOT gate /graph or
 * /node/summary on admin_surface_allowed — org structure and node metadata are
 * currently visible to ANY signed-in Work Identity, and per-identity
 * enforcement of these surfaces is pending the authorization build. So this
 * component does not "grant" or "deny" anything; it states plainly what is and
 * isn't enforced today and reveals the surface on one explicit opt-in. The
 * *_surface_allowed flag from /me/scope is a DERIVED signal, not an enforced
 * boundary for these surfaces.
 *
 * What IS enforced, separately and server-side: the governed document corpus
 * (Ask, documents, granted knowledge) is permission-scoped per principal.
 *
 * If a real deployment ever returns *_surface_allowed === true (the auth
 * build), the gate renders the surface directly.
 */
type GatedSurface = "admin" | "bursar";

const SURFACE_COPY: Record<
  GatedSurface,
  { context: "admin" | "bursar"; heading: string; lead: string; note: string }
> = {
  admin: {
    context: "admin",
    heading: "Operating Map — preview (not access-enforced)",
    lead:
      "The Operating Map shows the company org structure. It is a preview, not an access-enforced surface in this build.",
    note:
      "Enforced today: your document access (Ask, documents, granted knowledge) is permission-scoped on the server. Not enforced yet: this org-structure map and its node metadata are visible to any signed-in Work Identity. Per-identity enforcement of this surface is pending the authorization build.",
  },
  bursar: {
    context: "bursar",
    heading: "Bursar Ledger Room — preview (not connected)",
    lead: "The Bursar Ledger Room is a placeholder for the future governed-spend room.",
    note:
      "No ledger data and no finance authority are connected in this build. Per-identity enforcement of this surface is pending the authorization build; your document access is separately permission-scoped on the server.",
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
        className="ap-glass-elevated mx-auto max-w-2xl rounded-2xl p-6 md:p-8"
        data-testid="admin-preview-gate"
      >
        <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
          Preview — not access-enforced
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

        <DemoIdentityNotice
          className="mt-4"
          context={copy.context}
          testId={`admin-preview-gate-notice-${surface}`}
        />

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
            Read-only preview. Not per-identity access-enforced in this build.
          </p>
        </div>
      </MotionPanel>
    </main>
  );
}
