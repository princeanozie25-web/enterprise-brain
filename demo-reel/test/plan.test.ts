// T-R4: synthetic clip metadata -> the exact ffmpeg command list (golden).
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { buildPlan } from "../src/assemble.ts";
import { serializePlan, SYNTHETIC_BEATS, SYNTHETIC_DATE } from "./plan-fixture.ts";

const here = dirname(fileURLToPath(import.meta.url));

describe("T-R4: ffmpeg plan generation", () => {
  it("synthetic metadata produces the golden command list exactly", () => {
    const plan = buildPlan(SYNTHETIC_BEATS, SYNTHETIC_DATE);
    const golden = readFileSync(join(here, "golden", "plan.txt"), "utf8");
    expect(serializePlan(plan)).toBe(golden);
  });

  it("the plan applies the VO contract per beat", () => {
    const plan = buildPlan(SYNTHETIC_BEATS, SYNTHETIC_DATE);
    // B0 card silent: target 8 -> 8.5s beat + 0.4 gap.
    expect(plan.rows[0]).toMatchObject({ id: "B0", vo: "silent", duration_s: 8.5 });
    // B1 vo 36.2 > footage 33.4: freeze to 36.7.
    expect(plan.rows[1]).toMatchObject({
      id: "B1",
      vo: "present",
      duration_s: 36.7,
      freeze_s: 3.3,
    });
    // B2 silent, footage 52 > target+0.5: footage wins, nothing lost.
    expect(plan.rows[2]).toMatchObject({ id: "B2", vo: "silent", duration_s: 52, freeze_s: 0 });
    // Last beat gets no inter-beat gap: its -t equals its duration.
    const lastStep = plan.steps[2];
    expect(lastStep.args[lastStep.args.indexOf("-t") + 1]).toBe("52");
  });

  it("silent beats carry the VO PENDING watermark; voiced beats do not", () => {
    const plan = buildPlan(SYNTHETIC_BEATS, SYNTHETIC_DATE);
    const filterOf = (i: number) =>
      plan.steps[i].args[plan.steps[i].args.indexOf("-filter_complex") + 1];
    expect(filterOf(0)).toContain("VO PENDING");
    expect(filterOf(1)).not.toContain("VO PENDING");
    expect(filterOf(2)).toContain("VO PENDING");
    // The watermark is ink-soft; the loudness law rides the voiced beat.
    expect(filterOf(0)).toContain("0x5C5C54");
    expect(filterOf(1)).toContain("loudnorm=I=-16");
  });
});
