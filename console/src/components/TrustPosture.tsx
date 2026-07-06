"use client";

import { TYPE } from "@/lib/tokens";
import { MotionSection } from "./MotionPrimitives";

type DemoIdentityContext = "standard" | "admin" | "bursar" | "employee";

const CONTEXT_COPY: Record<DemoIdentityContext, string> = {
  admin:
    "Admin-side routes are visible here for pilot review. Production admin authority is not connected in this build.",
  bursar:
    "Reads the local spend producer over a loopback contract (ledger.v1.1). Read-only: this room renders spend; it never authorizes it.",
  employee:
    "This Work Identity preview shows scoped work, requests, grants, and Ask context without creating restricted surface access.",
  standard:
    "Actor selection previews permission boundaries in this local pilot workspace. Production identity binding is not connected in this build.",
};

export function DemoIdentityNotice({
  className = "",
  compact = false,
  context = "standard",
  testId = "demo-identity-notice",
}: {
  className?: string;
  compact?: boolean;
  context?: DemoIdentityContext;
  testId?: string;
}) {
  return (
    <MotionSection
      className={`ap-card rounded-lg border ${compact ? "p-2" : "p-3"} ${className}`}
      data-testid={testId}
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Demo Identity Mode
          </p>
          <p
            className="ap-register-chrome mt-1"
            style={{ fontSize: compact ? TYPE.scale.xs : TYPE.scale.sm, fontWeight: 600, lineHeight: TYPE.line.body }}
          >
            Local pilot workspace. Production identity is not connected.
          </p>
        </div>
        <span
          className="ap-chip ap-register-evidence rounded-lg px-2 py-1"
          style={{ fontSize: TYPE.scale.xs }}
        >
          permission preview
        </span>
      </div>
      <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
        {CONTEXT_COPY[context]}
      </p>
      {/* B5 (council D3): the one surprising 200 is pre-explained — calm
          register, one line, never error-styled. */}
      <p
        className="ap-soft mt-2"
        style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
        data-testid="view-as-disclosure"
      >
        In demo mode, any identity may view-as any other — every view-as is audited before render.
      </p>
      {!compact && (
        <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
          The architecture is designed for enterprise IAM, SSO, SCIM, RBAC/ABAC, audit, and policy enforcement. This build does not claim those integrations are live.
        </p>
      )}
    </MotionSection>
  );
}
