// The assembly half: probe the takes, apply the VO contract, emit and run
// the ffmpeg plan — per-beat normalized segments, hard cuts, one master.
// `buildPlan` is PURE (T-R4 holds it to a golden file); the runner around
// it does the probing and process spawning.
//
// Usage:
//   node src/assemble.ts [--out out] [--vo vo] [--date yyyy-mm-dd]

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import beatsJson from "../beats.json" with { type: "json" };
import { computeBeatTiming } from "./timing.ts";
import type { AssemblyRow, BeatFacts } from "./take-report.ts";
import { renderTakeReport } from "./take-report.ts";

const here = dirname(fileURLToPath(import.meta.url));
const reelRoot = resolve(here, "..");

// The watermark font: ffmpeg's drawtext cannot consume the console's woff2
// subsets, so the silent-beat watermark uses the SAME Inter face from
// service/fonts (decompressed from the console's own subset in AP-5;
// flagged in the closeout). Relative path keeps drawtext free of drive
// colons on Windows.
const WATERMARK_FONT = "../service/fonts/Inter-Regular.ttf";
const INK_SOFT_HEX = "0x5C5C54";
const PAPER_HEX = "0xFAFAF7";

export interface PlanBeat {
  id: string;
  kind: "card" | "capture";
  /** Relative media path: takes/<id>.webm or cards/<id>.png. */
  media: string;
  /** Probed natural footage length (cards: the script target). */
  footageS: number;
  /** Relative VO path, or null for generated silence. */
  vo: string | null;
  /** Probed VO length when vo is present. */
  voS: number | null;
  targetS: number;
  isLast: boolean;
}

export interface PlanStep {
  comment: string;
  args: string[];
}

export interface Plan {
  steps: PlanStep[];
  segmentList: string;
  master: string;
  rows: AssemblyRow[];
}

const fps = String(beatsJson.video.fps);
const crf = String(beatsJson.video.crf);
const size = `${beatsJson.video.width}x${beatsJson.video.height}`;
const GAP = beatsJson.pacing.interBeatGapS;

function videoFilter(kind: "card" | "capture", freezeS: number, silent: boolean): string {
  const scale =
    `scale=${beatsJson.video.width}:${beatsJson.video.height}:force_original_aspect_ratio=decrease,` +
    `pad=${beatsJson.video.width}:${beatsJson.video.height}:(ow-iw)/2:(oh-ih)/2:color=${PAPER_HEX},` +
    `fps=${fps}`;
  const freeze = freezeS > 0 ? `,tpad=stop_mode=clone:stop_duration=${freezeS}` : "";
  const watermark = silent
    ? `,drawtext=fontfile='${WATERMARK_FONT}':text='VO PENDING':x=48:y=h-72:fontsize=40:fontcolor=${INK_SOFT_HEX}`
    : "";
  return `[0:v]${scale}${freeze}${watermark},format=yuv420p[v]`;
}

function audioFilter(silent: boolean): string {
  // VO mono -> stereo, -16 LUFS; generated silence is already stereo.
  return silent
    ? `[1:a]anull[a]`
    : `[1:a]loudnorm=I=${beatsJson.pacing.voLufs}:TP=-1.5:LRA=11,aformat=channel_layouts=stereo,apad[a]`;
}

/** The pure ffmpeg plan: deterministic, path-literal, golden-testable. */
export function buildPlan(beats: PlanBeat[], date: string): Plan {
  const steps: PlanStep[] = [];
  const rows: AssemblyRow[] = [];
  const segments: string[] = [];

  for (const beat of beats) {
    const timing = computeBeatTiming(
      beat.footageS,
      beat.voS,
      beat.targetS,
      beatsJson.pacing.tailFreezeS,
    );
    const gap = beat.isLast ? 0 : GAP;
    const segDuration = Math.round((timing.durationS + gap) * 1000) / 1000;
    const freeze = Math.round((timing.freezeS + gap) * 1000) / 1000;
    const segment = `segments/${beat.id}.mp4`;
    segments.push(segment);
    rows.push({
      id: beat.id,
      vo: timing.silent ? "silent" : "present",
      footage_s: beat.footageS,
      vo_s: timing.voS,
      duration_s: timing.durationS,
      freeze_s: timing.freezeS,
      trim_s: timing.trimS,
    });

    const visualInput =
      beat.kind === "card"
        ? ["-loop", "1", "-framerate", fps, "-i", beat.media]
        : ["-i", beat.media];
    const audioInput = timing.silent
      ? ["-f", "lavfi", "-t", String(segDuration), "-i", "anullsrc=r=48000:cl=stereo"]
      : ["-i", beat.vo!];

    steps.push({
      comment: `beat ${beat.id}: ${beat.kind}, ${timing.silent ? "silence" : "vo"}, ${segDuration}s`,
      args: [
        "-y",
        ...visualInput,
        ...audioInput,
        "-filter_complex",
        `${videoFilter(beat.kind, beat.kind === "card" ? 0 : freeze, timing.silent)};${audioFilter(timing.silent)}`,
        "-map",
        "[v]",
        "-map",
        "[a]",
        "-t",
        String(segDuration),
        "-c:v",
        "libx264",
        "-profile:v",
        "high",
        "-crf",
        crf,
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-b:a",
        "192k",
        "-ar",
        "48000",
        segment,
      ],
    });
  }

  const master = `master-${date}.mp4`;
  steps.push({
    comment: "concat: hard cuts only",
    args: ["-y", "-f", "concat", "-safe", "0", "-i", "segments.txt", "-c", "copy", master],
  });
  return {
    steps,
    segmentList: segments.map((s) => `file '${s}'`).join("\n") + "\n",
    master,
    rows,
  };
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

function ffprobeDuration(path: string): number {
  const probe = spawnSync(
    "ffprobe",
    ["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", path],
    { encoding: "utf8" },
  );
  if (probe.status !== 0) {
    throw new Error(`ffprobe failed for ${path}: ${probe.stderr}`);
  }
  const duration = Number.parseFloat(probe.stdout.trim());
  if (!Number.isFinite(duration)) {
    throw new Error(`ffprobe returned no duration for ${path}`);
  }
  return Math.round(duration * 1000) / 1000;
}

function main(): void {
  const argv = process.argv.slice(2);
  const get = (flag: string): string | null => {
    const at = argv.indexOf(flag);
    return at >= 0 && at + 1 < argv.length ? argv[at + 1] : null;
  };
  const outDir = resolve(get("--out") ?? join(reelRoot, "out"));
  const voDir = resolve(get("--vo") ?? join(reelRoot, "vo"));
  const date = get("--date") ?? new Date().toISOString().slice(0, 10);

  const factsPath = join(outDir, "take-report.json");
  if (!existsSync(factsPath)) {
    console.error(`REFUSED: ${factsPath} missing — run capture first`);
    process.exit(1);
  }
  const facts: BeatFacts[] = JSON.parse(readFileSync(factsPath, "utf8"));
  mkdirSync(join(outDir, "segments"), { recursive: true });

  const beats: PlanBeat[] = beatsJson.beats.map((beat, index) => {
    const isLast = index === beatsJson.beats.length - 1;
    if (beat.kind === "card") {
      const media = `cards/${beat.id}.png`;
      if (!existsSync(join(outDir, media))) {
        throw new Error(`card missing: ${media} — run capture first`);
      }
      const voFile = join(voDir, `vo-${beat.id}.wav`);
      const hasVo = existsSync(voFile);
      return {
        id: beat.id,
        kind: "card",
        media,
        footageS: beat.target_s,
        vo: hasVo ? voFile : null,
        voS: hasVo ? ffprobeDuration(voFile) : null,
        targetS: beat.target_s,
        isLast,
      };
    }
    const media = `takes/${beat.id}.webm`;
    if (!existsSync(join(outDir, media))) {
      throw new Error(`take missing: ${media} — run capture first`);
    }
    const voFile = join(voDir, `vo-${beat.id}.wav`);
    const hasVo = existsSync(voFile);
    return {
      id: beat.id,
      kind: "capture",
      media,
      footageS: ffprobeDuration(join(outDir, media)),
      vo: hasVo ? voFile : null,
      voS: hasVo ? ffprobeDuration(voFile) : null,
      targetS: beat.target_s,
      isLast,
    };
  });

  const plan = buildPlan(beats, date);
  writeFileSync(join(outDir, "segments.txt"), plan.segmentList);

  for (const step of plan.steps) {
    console.log(`\n# ${step.comment}\nffmpeg ${step.args.join(" ")}`);
    const run = spawnSync("ffmpeg", step.args, { cwd: outDir, encoding: "utf8" });
    if (run.status !== 0) {
      console.error(run.stderr.slice(-2000));
      console.error(`REFUSED: ffmpeg step failed (${step.comment})`);
      process.exit(1);
    }
  }

  const masterPath = join(outDir, plan.master);
  const master = {
    path: masterPath,
    bytes: statSync(masterPath).size,
    duration_s: ffprobeDuration(masterPath),
  };
  const report = renderTakeReport({ date, facts, assembly: plan.rows, master });
  writeFileSync(join(outDir, "take-report.md"), report);
  console.log(`\n${report}`);
}

const invokedDirectly =
  process.argv[1] !== undefined &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) {
  main();
}
