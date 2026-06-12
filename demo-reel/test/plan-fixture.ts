// Shared by T-R4 and the one-off golden generator: synthetic clip metadata
// and the plan serializer. No vitest imports — plain module.
import type { Plan, PlanBeat } from "../src/assemble.ts";

export const SYNTHETIC_BEATS: PlanBeat[] = [
  {
    id: "B0",
    kind: "card",
    media: "cards/B0.png",
    footageS: 8,
    vo: null,
    voS: null,
    targetS: 8,
    isLast: false,
  },
  {
    id: "B1",
    kind: "capture",
    media: "takes/B1.webm",
    footageS: 33.4,
    vo: "vo/vo-B1.wav",
    voS: 36.2,
    targetS: 35,
    isLast: false,
  },
  {
    id: "B2",
    kind: "capture",
    media: "takes/B2.webm",
    footageS: 52,
    vo: null,
    voS: null,
    targetS: 50,
    isLast: true,
  },
];

export const SYNTHETIC_DATE = "2026-01-05";

export function serializePlan(plan: Plan): string {
  const lines: string[] = [];
  for (const step of plan.steps) {
    lines.push(`# ${step.comment}`);
    lines.push(`ffmpeg ${step.args.join(" ")}`);
  }
  lines.push("== segments.txt ==");
  lines.push(plan.segmentList.trimEnd());
  lines.push("== master ==");
  lines.push(plan.master);
  lines.push("");
  return lines.join("\n");
}
