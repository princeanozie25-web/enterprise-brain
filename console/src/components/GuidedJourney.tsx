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

const DEMO_WORK_IDENTITY = "p060";

function actorQuery(principal: string | null): string {
  return `?as=${encodeURIComponent(principal ?? DEMO_WORK_IDENTITY)}`;
}

function journeySteps(principal: string | null): JourneyStep[] {
  const query = actorQuery(principal);
  const me = `/me${query}`;
  return [
    {
      detail: "Choose who Enterprise Brain is acting for.",
      href: me,
      key: "me",
      label: "Work Identity",
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
      detail: "Inspect the company relationships this identity can see.",
      href: `/admin/graph${query}`,
      key: "operating-map",
      label: "Operating Map",
      surface: "graph" as const,
    },
    {
      adminOnly: true,
      detail: "Review governed model spend when ledger data is connected.",
      href: "/admin/bursar",
      key: "bursar-ledger-room",
      label: "Bursar Ledger Room",
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
  const selectionCopy =
    principal === null
      ? "Start with a Work Identity. Until one is selected, Enterprise Brain has no permission scope to show work, access, knowledge, or answers."
      : "This path carries the selected Work Identity through work, access, Ask, the Operating Map, and governed spend.";

  return (
    <MotionSection
      className="ap-card rounded border p-3"
      aria-label="Guided product path"
      data-testid={testId}
      style={{
        backdropFilter: "blur(16px)",
        background: "color-mix(in srgb, var(--paper) 84%, transparent)",
      }}
    >
      <div className="mb-3 flex flex-wrap items-baseline justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Guided path
          </p>
          <h2 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            From identity to governed answers
          </h2>
        </div>
        <p className="ap-soft max-w-2xl" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
          {selectionCopy}
        </p>
      </div>
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-4 xl:grid-cols-7">
        {steps.map((step, index) => {
          const active = current === (step.activeSurface ?? step.surface);
          const canOpen = !step.adminOnly || adminLinks || current === "home";
          const className = `${active ? "ap-affordance-button" : "ap-washable"} ap-register-chrome flex min-h-24 flex-col justify-between rounded border px-3 py-2`;
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
