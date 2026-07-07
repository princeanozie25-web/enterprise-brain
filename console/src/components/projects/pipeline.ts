import type { WorkflowItem } from "@/lib/api";
import { DARK, DEPARTMENT_PASTEL, GRAPH_STAGE } from "@/lib/tokens";

/**
 * SHOWCASE-2 (Track B) — the pipeline model. COLUMNS are the workflow payload's
 * status GROUPS (WorkflowView's set, unchanged); the ORDER is the pipeline flow
 * — planned → active → awaiting a human decision (the gate) → its two outcomes
 * (blocked | done). The order was always a client choice (there is no payload
 * status enum), so the flow order is a deliberate, flagged Showcase-II decision;
 * the group SET stays payload-derived. Stage NAMES are ours — the reference's
 * Capture/Classify/Route/Process names are its data, never imported.
 */
export type StageKey = "next" | "in_progress" | "waiting" | "blocked" | "done";

export type Stage = {
  key: StageKey;
  /** "01".."05" — the reference's STAGE 0N eyebrow ordinal (form, not data). */
  ordinal: string;
  name: string;
  statuses: string[];
  /** The Waiting column IS the human gate: pending items await a decision. */
  humanGate?: boolean;
};

export const STAGES: Stage[] = [
  { key: "next", ordinal: "01", name: "Next", statuses: ["candidate", "planned"] },
  { key: "in_progress", ordinal: "02", name: "In Progress", statuses: ["active"] },
  { key: "waiting", ordinal: "03", name: "Waiting", statuses: ["pending"], humanGate: true },
  {
    key: "blocked",
    ordinal: "04",
    name: "Blocked",
    statuses: ["blocked", "denied", "cancelled", "expired", "dismissed"],
  },
  { key: "done", ordinal: "05", name: "Done", statuses: ["done", "approved"] },
];

const STATUS_TO_STAGE = new Map<string, StageKey>();
for (const stage of STAGES) for (const status of stage.statuses) STATUS_TO_STAGE.set(status, stage.key);

/** Any unmapped status falls to "next" (fail-honest: never invents a stage). */
export function stageForStatus(status: string): StageKey {
  return STATUS_TO_STAGE.get(status.toLowerCase()) ?? "next";
}

export function groupItems(items: WorkflowItem[]): Map<StageKey, WorkflowItem[]> {
  const grouped = new Map<StageKey, WorkflowItem[]>();
  for (const stage of STAGES) grouped.set(stage.key, []);
  for (const item of items) grouped.get(stageForStatus(item.status))!.push(item);
  return grouped;
}

/**
 * The active path (B3). The payload carries NO in-flight marker and NO route
 * history, so "active" = every item whose payload status is literally "active".
 * Each gets the amber OUTLINE; there is no single "the one" and no cross-stage
 * spline (a spline needs history the payload does not have) — flagged.
 */
export function isActive(item: WorkflowItem): boolean {
  return item.status.toLowerCase() === "active";
}

export function isDone(item: WorkflowItem): boolean {
  const status = item.status.toLowerCase();
  return status === "done" || status === "approved";
}

export const KIND_LABEL: Record<WorkflowItem["kind"], string> = {
  access_request: "Access request",
  accepted_agent_box: "Accepted agent box",
  lane_box: "Work item",
};

export function statusLabel(status: string): string {
  switch (status.toLowerCase()) {
    case "active":
      return "In progress";
    case "pending":
      return "Waiting";
    case "blocked":
      return "Blocked";
    case "denied":
      return "Denied";
    case "cancelled":
      return "Cancelled";
    case "expired":
      return "Expired";
    case "dismissed":
      return "Dismissed";
    case "done":
      return "Done";
    case "approved":
      return "Approved";
    case "planned":
      return "Planned";
    case "candidate":
      return "Next";
    default:
      return status;
  }
}

/** B2: the done pastel — soft green from the department family, NEVER the
 * sensitivity register. Reused by the status dot and the completed check. */
export const PASTEL_GREEN = DEPARTMENT_PASTEL[2].hex;

/**
 * Status-dot styling on the fixed-navy stage, reserved-color law intact:
 * - active     → the ONE warm signal (var(--accent-warm)); the active path.
 * - waiting    → luminous periwinkle (pinned DARK.affordance) = awaiting a human.
 * - done       → the sanctioned soft-green pastel + a check badge.
 * - next/blocked → a muted neutral (pinned label-soft) — never red, never a
 *   department-identity pastel. Amber is spent only on the active signal.
 */
export function stageDot(key: StageKey): { fill: string; filled: boolean } {
  switch (key) {
    case "in_progress":
      return { fill: "var(--accent-warm)", filled: true };
    case "waiting":
      return { fill: DARK.affordance, filled: true };
    case "done":
      return { fill: PASTEL_GREEN, filled: true };
    case "next":
    case "blocked":
      return { fill: GRAPH_STAGE.labelSoft, filled: false };
  }
}

/** The item's people, most-relevant first: owner (holds it), approver
 * (decides it), requester (asked), agent (projected). Payload fields only. */
export function itemActors(item: WorkflowItem): Array<{ role: string; id: string }> {
  return [
    item.owner_id ? { role: "owner", id: item.owner_id } : null,
    item.approver_id ? { role: "approver", id: item.approver_id } : null,
    item.requester_id ? { role: "requester", id: item.requester_id } : null,
    item.agent_id ? { role: "agent", id: item.agent_id } : null,
  ].filter((entry): entry is { role: string; id: string } => entry !== null);
}
