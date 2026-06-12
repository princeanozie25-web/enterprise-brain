// T-R3: beats.json is the single source — schema + the 3:30–4:30 budget.
import { describe, expect, it } from "vitest";
import beatsJson from "../beats.json" with { type: "json" };

describe("T-R3: beats.json schema and budget", () => {
  it("every beat carries id, kind, and a positive target duration", () => {
    expect(beatsJson.beats.length).toBeGreaterThan(0);
    for (const beat of beatsJson.beats) {
      expect(typeof beat.id).toBe("string");
      expect(beat.id.length).toBeGreaterThan(0);
      expect(["card", "capture"]).toContain(beat.kind);
      expect(typeof beat.target_s).toBe("number");
      expect(beat.target_s).toBeGreaterThan(0);
      if (beat.kind === "capture") {
        expect(typeof (beat as { scene?: string }).scene).toBe("string");
      } else {
        expect(typeof (beat as { card?: string }).card).toBe("string");
      }
    }
  });

  it("beat ids are unique", () => {
    const ids = beatsJson.beats.map((b) => b.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("script targets sum to 3:30–4:30", () => {
    const total = beatsJson.beats.reduce((sum, b) => sum + b.target_s, 0);
    expect(total).toBeGreaterThanOrEqual(210);
    expect(total).toBeLessThanOrEqual(270);
  });

  it("the pacing numbers match the contract", () => {
    expect(beatsJson.pacing.keystrokeMs).toBe(40);
    expect(beatsJson.pacing.mouseSteps).toBe(25);
    expect(beatsJson.pacing.clickSettleMs).toBe(80);
    expect(beatsJson.pacing.stateHoldMs).toBe(600);
    expect(beatsJson.pacing.scrollPxPerS).toBe(400);
    expect(beatsJson.pacing.tailFreezeS).toBe(0.5);
    expect(beatsJson.pacing.interBeatGapS).toBe(0.4);
    expect(beatsJson.pacing.voLufs).toBe(-16);
    expect(beatsJson.video).toEqual({ width: 1920, height: 1080, fps: 30, crf: 18 });
  });
});
