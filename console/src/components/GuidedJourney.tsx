"use client";

import { TYPE } from "@/lib/tokens";
import { MotionAnchor, MotionSection } from "./MotionPrimitives";

type JourneySurface = "home" | "me" | "project" | "ask" | "graph" | "bursar";
type JourneyStep = {
  activeSurface?: JourneySurface;
  adminOnly?: boolean;
  detail: string;
  href: string;
  key: string;
  label: string;
  surface?: JourneySurface;
};

/** A2: no hardwired identity — with no principal the journey links carry no
 * `?as` at all, and the identity picker (the front door) catches arrivals. */
function actorQuery(principal: string | null): string {
  return principal === null ? "" : `?as=${encodeURIComponent(principal)}`;
}

function journeySteps(principal: string | null): JourneyStep[] {
  const query = actorQuery(principal);
  const me = `/me${query}`;
  return [
    {
      detail: "Your work, your access, at a glance.",
      href: me,
      key: "me",
      label: "Home",
      surface: "me" as const,
    },
    {
      activeSurface: "project" as const,
      detail: "See assigned work and visible workflow items.",
      href: `${me}#dashboard-workflow`,
      key: "review-work",
      label: "Review work",
    },
    {
      detail: "Review requests, approvals, and grant status.",
      href: `${me}#dashboard-requests`,
      key: "access-requests",
      label: "Access requests",
    },
    {
      detail: "Use approved read grants in scoped Ask.",
      href: `${me}#dashboard-granted-knowledge`,
      key: "granted-knowledge",
      label: "Granted Knowledge",
    },
    {
      detail: "Ask with the selected permission scope.",
      href: `/ask${query}`,
      key: "ask",
      label: "Ask",
      surface: "ask" as const,
    },
    {
      // Track A re-homing: the Operating Map is an admin door — the employee
      // journey never points at it.
      adminOnly: true,
      detail: "Inspect the company relationships this identity can see.",
      href: `/admin/graph${query}`,
      key: "operating-map",
      label: "Operating Map",
      surface: "graph" as const,
    },
    {
      adminOnly: true,
      detail: "Admin-side spend view; reads the local ledger producer, read-only.",
      href: "/admin/bursar",
      key: "bursar-ledger-room",
      label: "Spend Ledger",
      surface: "bursar" as const,
    },
  ];
}

export function GuidedJourney({
  adminLinks = false,
  current,
  principal,
  testId = "guided-journey",
}: {
  adminLinks?: boolean;
  current: JourneySurface;
  principal: string | null;
  testId?: string;
}) {
  const steps = journeySteps(principal);
  // Track A: admin steps show only when the caller is admin-class (adminLinks)
  // or when already on an admin surface (bursar) — an employee on Home never
  // sees an admin door in the journey.
  const showAdminSteps = adminLinks || current === "bursar";
  const visibleSteps = steps.filter((step) => !step.adminOnly || showAdminSteps);
  const compact = current !== "home" && current !== "bursar";
  const selectionCopy =
    principal === null
      ? "Pick who you are first. Until then, Enterprise Brain has no permission scope to show work, access, knowledge, or answers."
      : "This path carries the selected Work Identity through work, access, Ask, the Operating Map, and governed spend.";

  if (compact) {
    return (
      <MotionSection
        className="ap-card rounded-lg border p-2"
        aria-label="Guided product path"
        data-testid={testId}
        data-compact="true"
      >
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="min-w-0">
            <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
              Guided path
            </p>
            {/* B2: wayfinding furniture, not a section — a styled label so
                the room's h1 stays the first heading on every route. */}
            <p className="ap-register-chrome mt-0.5" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
              What to do next
            </p>
          </div>
          <nav className="flex max-w-full gap-1 overflow-x-auto pb-1" aria-label="Compact product path">
            {visibleSteps.map((step, index) => {
              const active = current === (step.activeSurface ?? step.surface);
              const canOpen = !step.adminOnly || showAdminSteps;
              const className = `${active ? "ap-affordance-button" : "ap-washable ap-flat"} ap-register-chrome inline-flex min-h-9 shrink-0 items-center gap-1 rounded-lg border px-2.5 py-1.5`;
              const content = (
                <>
                  <span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
                    {index + 1}
                  </span>
                  <span style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>{step.label}</span>
                </>
              );

              if (active || !canOpen) {
                return (
                  <span
                    key={step.key}
                    className={className}
                    data-active={active ? "true" : "false"}
                    data-testid={`guided-journey-step-${step.key}`}
                    aria-current={active ? "step" : undefined}
                    aria-disabled={!canOpen ? "true" : undefined}
                    style={{ borderColor: "var(--hairline)" }}
                  >
                    {content}
                  </span>
                );
              }

              return (
                <MotionAnchor
                  key={step.key}
                  href={step.href}
                  className={className}
                  data-active="false"
                  data-testid={`guided-journey-step-${step.key}`}
                  delayIndex={index}
                  style={{ borderColor: "var(--hairline)" }}
                >
                  {content}
                </MotionAnchor>
              );
            })}
          </nav>
        </div>
      </MotionSection>
    );
  }

  return (
    <MotionSection
      className="ap-card rounded-lg border p-3"
      aria-label="Guided product path"
      data-testid={testId}
    >
      <div className="mb-3 flex flex-wrap items-baseline justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Guided path
          </p>
          {/* B2: wayfinding furniture, not a section (see the compact rail). */}
          <p className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            From identity to governed answers
          </p>
        </div>
        <p className="ap-soft max-w-2xl" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
          {selectionCopy}
        </p>
      </div>
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-4 xl:grid-cols-7">
        {visibleSteps.map((step, index) => {
          const active = current === (step.activeSurface ?? step.surface);
          const canOpen = !step.adminOnly || showAdminSteps;
          const className = `${active ? "ap-affordance-button" : "ap-washable ap-flat"} ap-register-chrome flex min-h-24 flex-col justify-between rounded-lg border px-3 py-2`;
          const content = (
            <>
              <span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
                Step {index + 1}
              </span>
              <span style={{ fontSize: TYPE.scale.sm, fontWeight: 600, lineHeight: TYPE.line.body }}>
                {step.label}
              </span>
              <span className="ap-soft" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
                {step.detail}
              </span>
            </>
          );

          if (active || !canOpen) {
            return (
              <span
                key={step.key}
                className={className}
                data-active={active ? "true" : "false"}
                data-testid={`guided-journey-step-${step.key}`}
                aria-current={active ? "step" : undefined}
                aria-disabled={!canOpen ? "true" : undefined}
                style={{ borderColor: "var(--hairline)" }}
              >
                {content}
              </span>
            );
          }

          return (
            <MotionAnchor
              key={step.key}
              href={step.href}
              className={className}
              data-active="false"
              data-testid={`guided-journey-step-${step.key}`}
              delayIndex={index}
              style={{ borderColor: "var(--hairline)" }}
            >
              {content}
            </MotionAnchor>
          );
        })}
      </div>
    </MotionSection>
  );
}
