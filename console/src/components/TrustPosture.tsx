"use client";

import { TYPE } from "@/lib/tokens";
import { MotionArticle, MotionSection } from "./MotionPrimitives";

type DemoIdentityContext = "standard" | "admin" | "bursar" | "employee";

const CONTEXT_COPY: Record<DemoIdentityContext, string> = {
  admin:
    "Admin-side routes are visible here for pilot review. Production admin authority is not connected in this build.",
  bursar:
    "Bursar is an admin, finance, and executive-domain preview. Production authority and ledger.v1.1 data are not connected in this build.",
  employee:
    "This Work Identity preview shows scoped work, requests, grants, and Ask context without creating restricted surface access.",
  standard:
    "Actor selection previews permission boundaries in this local pilot workspace. Production identity binding is not connected in this build.",
};

const TRUST_ITEMS = [
  {
    detail: "No Work Identity means no permission scope for work, knowledge, or Ask.",
    label: "Deny by default",
  },
  {
    detail: "Ask runs inside the selected Work Identity and validates granted context server-side.",
    label: "Permission-aware Ask",
  },
  {
    detail: "Read grants are scoped to specific capabilities and remain visible as audit rows.",
    label: "Scoped grants",
  },
  {
    detail: "Admin and Bursar routes are separated from employee surfaces in this UI.",
    label: "Separated authority",
  },
  {
    detail: "A production deployment would bind Enterprise Brain to IAM, SSO, SCIM, RBAC/ABAC, audit, and policy enforcement.",
    label: "Enterprise identity boundary",
  },
];

function posturePanelStyle(): React.CSSProperties {
  return {
    backdropFilter: "blur(var(--material-blur))",
    background: "var(--surface-glass)",
    boxShadow: "var(--shadow-1), inset 0 1px 0 var(--edge-highlight)",
  };
}

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
      className={`ap-card ap-glass rounded border ${compact ? "p-2" : "p-3"} ${className}`}
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
          className="ap-chip ap-register-evidence rounded px-2 py-1"
          style={{ fontSize: TYPE.scale.xs }}
        >
          permission preview
        </span>
      </div>
      <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
        {CONTEXT_COPY[context]}
      </p>
      {!compact && (
        <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
          The architecture is designed for enterprise IAM, SSO, SCIM, RBAC/ABAC, audit, and policy enforcement. This build does not claim those integrations are live.
        </p>
      )}
    </MotionSection>
  );
}

export function BuyerTrustPosture({
  className = "",
  testId = "buyer-trust-posture",
}: {
  className?: string;
  testId?: string;
}) {
  return (
    <MotionSection className={`grid grid-cols-1 gap-3 lg:grid-cols-[1fr_1fr] ${className}`} data-testid={testId}>
      {TRUST_ITEMS.map((item, index) => (
        <MotionArticle
          key={item.label}
          className="ap-card rounded border p-4"
          delayIndex={index}
          style={posturePanelStyle()}
        >
          <h2 className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            {item.label}
          </h2>
          <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            {item.detail}
          </p>
        </MotionArticle>
      ))}
    </MotionSection>
  );
}
