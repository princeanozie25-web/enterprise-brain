// T-R1: the VO-contract timing math across its cases.
import { describe, expect, it } from "vitest";
import { computeBeatTiming } from "../src/timing.ts";

describe("T-R1: freeze/trim duration computation", () => {
  it("footage longer than VO+0.5 keeps its full length (nothing lost)", () => {
    const t = computeBeatTiming(40, 30, 35);
    expect(t).toEqual({ durationS: 40, freezeS: 0, trimS: 0, voS: 30, silent: false });
  });

  it("footage shorter than VO+0.5 freeze-frames its last frame", () => {
    const t = computeBeatTiming(30, 35, 35);
    expect(t.durationS).toBe(35.5);
    expect(t.freezeS).toBe(5.5);
    expect(t.trimS).toBe(0);
    expect(t.silent).toBe(false);
  });

  it("exact fit: footage == VO + tail freeze", () => {
    const t = computeBeatTiming(35.5, 35, 35);
    expect(t.durationS).toBe(35.5);
    expect(t.freezeS).toBe(0);
  });

  it("silence fallback uses the script target as the VO clock", () => {
    const t = computeBeatTiming(30, null, 35);
    expect(t).toEqual({ durationS: 35.5, freezeS: 5.5, trimS: 0, voS: 35, silent: true });
  });

  it("silence fallback never trims real footage", () => {
    const t = computeBeatTiming(60, null, 50);
    expect(t.durationS).toBe(60);
    expect(t.freezeS).toBe(0);
    expect(t.trimS).toBe(0);
  });

  it("card beats: footage == target, silent", () => {
    const t = computeBeatTiming(8, null, 8);
    expect(t.durationS).toBe(8.5);
    expect(t.freezeS).toBe(0.5);
  });

  it("rounds to milliseconds", () => {
    const t = computeBeatTiming(10.3333, 10, 10);
    expect(t.durationS).toBe(10.5);
    expect(t.freezeS).toBe(0.167);
  });

  it("refuses nonsense inputs", () => {
    expect(() => computeBeatTiming(-1, null, 10)).toThrow();
    expect(() => computeBeatTiming(10, null, 0)).toThrow();
  });
});
