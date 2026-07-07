"use client";
import { MotionAnchor } from "../MotionPrimitives";
import { EmptyLine, activeKnowledgeGrants, dashboardPanelStyle } from "./shared";
import type { CockpitItem } from "./shared";

import { useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import type { AccessGrantRecord } from "@/lib/api";
import { TYPE } from "@/lib/tokens";
import { MotionArticle, MotionSection } from "../MotionPrimitives";
import {
  Chip,
  askGrantHref,
  plural,
  type CommandPodModel,
  type TodayCockpitModel,
} from "./shared";

export function CommandPods({ pods }: { pods: CommandPodModel[] }) {
  return (
    <section className="mb-4 grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-4" data-testid="dashboard-command-pods">
      {pods.map((pod, index) => (
        <CommandPod key={`${pod.kind}:${pod.title}`} delayIndex={index} pod={pod} />
      ))}
    </section>
  );
}


export function CommandPod({ delayIndex, pod }: { delayIndex: number; pod: CommandPodModel }) {
  const isAsk = pod.kind === "ask";
  const content = (
    <>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            {pod.metric}
          </p>
          <h2
            className="ap-register-chrome mt-1"
            style={{ fontSize: isAsk ? TYPE.scale.lg : TYPE.scale.sm, fontWeight: 600 }}
          >
            {pod.title}
          </h2>
        </div>
        <span
          aria-hidden="true"
          className="ap-hairline grid shrink-0 place-items-center rounded-full border"
          style={{ height: 30, width: 30 }}
        >
          <span className="ap-register-evidence" style={{ fontSize: TYPE.scale.xs }}>
            {pod.kind.slice(0, 1).toUpperCase()}
          </span>
        </span>
      </div>
      <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
        {pod.detail}
      </p>
      {pod.callToAction && (
        <span
          className="ap-affordance-button ap-register-chrome mt-4 inline-flex rounded-lg px-3 py-2"
          style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
        >
          {pod.callToAction}
        </span>
      )}
    </>
  );

  return (
    <MotionAnchor
      href={pod.href}
      className="ap-card ap-washable block rounded-lg p-3"
      delayIndex={delayIndex}
      data-pod-kind={pod.kind}
      data-testid={isAsk ? "dashboard-ask-pod" : `dashboard-pod-${pod.kind}`}
      style={{
        ...dashboardPanelStyle(),
        minHeight: isAsk ? 178 : 136,
      }}
    >
      {content}
    </MotionAnchor>
  );
}


export function AskAgentCard({ actor, grants }: { actor: string; grants: AccessGrantRecord[] }) {
  const activeGrants = activeKnowledgeGrants(grants, actor);
  const grant = activeGrants[0];
  const href = grant
    ? `/ask?as=${encodeURIComponent(actor)}&grant=${encodeURIComponent(grant.grant_id)}&cap=${encodeURIComponent(grant.target.capability_id)}`
    : `/ask?as=${encodeURIComponent(actor)}`;

  return (
    <MotionAnchor
      href={href}
      className="ap-card ap-washable block h-full rounded-2xl p-4"
      data-testid="dashboard-ask-agent-card"
    >
      <div className="flex h-full flex-col justify-between gap-4">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Ask AI agent
          </p>
          <h2 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
            Ask with your current access
          </h2>
          <p className="ap-soft mt-1 max-w-2xl" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            Get an answer scoped to what this identity may see.
          </p>
        </div>
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div className="flex flex-wrap gap-1.5">
            <Chip>{activeGrants.length > 0 ? `${activeGrants.length} read grants` : "identity scope"}</Chip>
            <Chip mono>{actor}</Chip>
          </div>
          <span
            className="ap-affordance-button ap-register-chrome rounded-full px-4 py-2"
            style={{ fontSize: TYPE.scale.sm, fontWeight: 700 }}
          >
            Ask
          </span>
        </div>
      </div>
    </MotionAnchor>
  );
}


export function TodayCockpit({ model }: { model: TodayCockpitModel }) {
  const attentionCount = model.needsAttention.length;
  return (
    <MotionSection
      className="ap-card rounded-2xl p-3"
      data-testid="dashboard-today-cockpit"
    >
      <div className="mb-2 flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Today
          </p>
          <h2 className="ap-register-chrome" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
            What needs attention?
          </h2>
        </div>
        <Chip>{plural(attentionCount, "attention row")}</Chip>
      </div>

      <div className="grid grid-cols-1 gap-2 md:grid-cols-3">
        <CockpitSection
          emptyLabel="Nothing waiting."
          items={model.needsAttention}
          testId="dashboard-today-needs-attention"
          title="Needs Attention"
        />
        <CockpitSection
          emptyLabel="No active workflow rows."
          items={model.continueWork}
          testId="dashboard-today-continue-work"
          title="Continue Work"
        />
        <CockpitSection
          emptyLabel="No requests waiting."
          items={model.waitingOn}
          testId="dashboard-today-waiting-on"
          title="Waiting On"
        />
      </div>

      {model.managerRows.length > 0 && (
        <section className="mt-2" data-testid="dashboard-today-manager-context">
          <div className="mb-2 flex items-center justify-between gap-3">
            <h3 className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
              Manager Context
            </h3>
            <span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
              real scope only
            </span>
          </div>
          <div className="grid grid-cols-1 gap-2 lg:grid-cols-3">
            {model.managerRows.map((item, index) => (
              <CockpitRow item={item} key={`${item.title}:${item.metric}`} delayIndex={index} />
            ))}
          </div>
        </section>
      )}
    </MotionSection>
  );
}


export function CockpitSection({
  emptyLabel,
  items,
  testId,
  title,
}: {
  emptyLabel: string;
  items: CockpitItem[];
  testId: string;
  title: string;
}) {
  const visibleItems = items.slice(0, 2);
  return (
    <section className="ap-card rounded-2xl border p-2" data-testid={testId}>
      <div className="flex items-center justify-between gap-3">
        <h3 className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
          {title}
        </h3>
        <span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
          {items.length}
        </span>
      </div>
      {items.length === 0 ? (
        <EmptyLine>{emptyLabel}</EmptyLine>
      ) : (
        <div className="mt-2 space-y-2">
          {visibleItems.map((item, index) => (
            <CockpitRow compact item={item} key={`${item.title}:${item.metric}`} delayIndex={index} />
          ))}
          {items.length > visibleItems.length && (
            <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
              {items.length - visibleItems.length} more in Workspace
            </p>
          )}
        </div>
      )}
    </section>
  );
}


export function CockpitRow({
  compact = false,
  delayIndex,
  item,
}: {
  compact?: boolean;
  delayIndex: number;
  item: CockpitItem;
}) {
  return (
    <MotionAnchor
      href={item.href}
      className="ap-card ap-washable block rounded-2xl border p-2"
      delayIndex={delayIndex}
      data-cockpit-action={item.action.toLowerCase()}
      data-cockpit-tone={item.tone}
      data-testid="dashboard-today-row"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            {item.metric}
          </p>
          <h4 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            {item.title}
          </h4>
        </div>
        <span
          className="ap-affordance-button ap-register-chrome rounded-lg px-2 py-1"
          style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
        >
          {item.action}
        </span>
      </div>
      {!compact && (
        <>
          <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            {item.detail}
          </p>
          <p className="ap-register-evidence ap-soft mt-2" style={{ fontSize: TYPE.scale.xs }}>
            {item.source}
          </p>
        </>
      )}
    </MotionAnchor>
  );
}

