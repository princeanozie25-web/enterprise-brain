// The take report is the RECORDING BRIEF: per beat, what actually happened
// (judge applied or elided, retrieval mode, degradation, real durations) so
// the VO can be recorded to match reality — the applied line vs the elide
// line — never the other way around.

export interface BeatFacts {
  id: string;
  kind: "card" | "capture";
  scene?: string;
  judge_requested?: boolean;
  judge_applied?: boolean;
  retrieval_mode?: string;
  generation_applied?: boolean;
  degradation?: string;
  notes?: string;
}

export interface AssemblyRow {
  id: string;
  vo: "present" | "silent";
  footage_s: number;
  vo_s: number;
  duration_s: number;
  freeze_s: number;
  trim_s: number;
}

export interface MasterInfo {
  path: string;
  bytes: number;
  duration_s: number;
}

export function renderTakeReport(opts: {
  date: string;
  facts: BeatFacts[];
  assembly?: AssemblyRow[];
  master?: MasterInfo;
}): string {
  const lines: string[] = [];
  lines.push(`# Demo-reel take report — ${opts.date}`);
  lines.push("");
  lines.push("Per-beat facts from the REAL run. Record each VO take against the");
  lines.push("variant that actually happened (the applied line vs the elide line).");
  lines.push("");
  lines.push("| beat | kind | scene | judge | mode | generation | degradation |");
  lines.push("| --- | --- | --- | --- | --- | --- | --- |");
  for (const f of opts.facts) {
    const judge =
      f.judge_requested === undefined
        ? "n/a"
        : `${f.judge_requested ? "requested" : "off"} / ${f.judge_applied ? "APPLIED" : "not applied"}`;
    lines.push(
      `| ${f.id} | ${f.kind} | ${f.scene ?? "—"} | ${judge} | ${f.retrieval_mode ?? "n/a"} | ` +
        `${f.generation_applied === undefined ? "n/a" : String(f.generation_applied)} | ${f.degradation ?? "none"} |`,
    );
  }
  if (opts.assembly) {
    lines.push("");
    lines.push("| beat | vo | footage s | vo s | beat s | freeze s | trim s |");
    lines.push("| --- | --- | --- | --- | --- | --- | --- |");
    for (const row of opts.assembly) {
      lines.push(
        `| ${row.id} | ${row.vo} | ${row.footage_s} | ${row.vo_s} | ${row.duration_s} | ${row.freeze_s} | ${row.trim_s} |`,
      );
    }
  }
  if (opts.master) {
    lines.push("");
    lines.push(
      `Master: ${opts.master.path} — ${(opts.master.bytes / 1_048_576).toFixed(1)} MiB, ` +
        `${opts.master.duration_s.toFixed(1)}s`,
    );
  }
  const notes = opts.facts.filter((f) => f.notes);
  if (notes.length > 0) {
    lines.push("");
    lines.push("Notes:");
    for (const f of notes) {
      lines.push(`- ${f.id}: ${f.notes}`);
    }
  }
  lines.push("");
  return lines.join("\n");
}
