"use client";

import { useEffect, useState, type ReactNode } from "react";
import * as api from "@/lib/api";
import type { RoleScopeSummary } from "@/lib/api";
import { TYPE } from "@/lib/tokens";
import { MotionPanel } from "./MotionPrimitives";
import { DemoIdentityNotice } from "./TrustPosture";

/**
 * AdminPreviewGate — the authorization gate for the admin-domain surfaces
 * (Operating Map, Bursar Ledger Room).
 *
 * The product's grammar is fail-closed: the engine (/me/scope) reports
 * admin_surface_allowed / bursar_surface_allowed, and in demo-identity mode
 * no actor is granted these sensitive surfaces ("no explicit super-admin
 * primitive exists"). So this gate does NOT render the surface by default —
 * it renders an honest "not authorized" panel that shows the engine's own
 * reasons. The surface is reachable only through ONE explicit, clearly
 * labelled opt-in ("View demo preview"), and even then it stays a read-only,
 * scope-filtered preview that grants no authority.
 *
 * If a real deployment ever returns *_surface_allowed === true, the gate
 * renders the surface directly — visibility follows authority, not the other
 * way round. (Closes the P0: "UI renders admin surfaces without a gate.")
 */
type GatedSurface = "admin" | "bursar";

const SURFACE_COPY: Record<
  GatedSurface,
  { context: "admin" | "bursar"; heading: string; lead: string }
> = {
  admin: {
    context: "admin",
    heading: "Operating Map is an admin-domain preview",
    lead:
      "This Work Identity is not granted the admin surface. The route is a demo preview only — production admin authority is not connected in this build.",
  },
  bursar: {
    context: "bursar",
    heading: "Bursar Ledger Room is an admin-domain preview",
    lead:
      "This Work Identity is not granted the Bursar surface. The route is a demo preview only — production finance authority and ledger data are not connected in this build.",
  },
};

const FALLBACK_REASONS = [
  "No explicit admin, Bursar, or governance primitive exists for this Work Identity.",
  "Sensitive surfaces remain disallowed by the current demo-identity contract.",
];

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

  // Read role posture for THIS actor. Fail-closed: any error leaves roleScope
  // null and the surface gated. (A null actor cannot be authorized either.)
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

  // Engine-authorized, OR the viewer explicitly opened the labelled preview.
  if (allowed === true || revealed) {
    return <>{children}</>;
  }

  const copy = SURFACE_COPY[surface];
  const reasons =
    roleScope?.reasons && roleScope.reasons.length > 0 ? roleScope.reasons : FALLBACK_REASONS;

  return (
    <main className="min-w-0 flex-1" data-testid={`admin-preview-gate-${surface}`}>
      <MotionPanel
        className="ap-glass-elevated mx-auto max-w-2xl rounded-2xl p-6 md:p-8"
        data-testid="admin-preview-gate"
      >
        <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
          Authorization gate
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

        <section className="mt-4" aria-label="Why this surface is not granted">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Why this is not granted
          </p>
          <ul className="mt-2 grid grid-cols-1 gap-2">
            {reasons.map((reason) => (
              <li
                key={reason}
                className="ap-chip rounded px-3 py-2"
                style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
                data-testid="admin-preview-gate-reason"
              >
                {reason}
              </li>
            ))}
          </ul>
        </section>

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
            View demo preview
          </button>
          <p className="ap-soft" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            Read-only preview. Shows only what this Work Identity may see — no authority is granted.
          </p>
        </div>
      </MotionPanel>
    </main>
  );
}
