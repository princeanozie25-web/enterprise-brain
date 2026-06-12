// T-R1's subject: the VO-contract timing math, pure.
//
// THE CONTRACT (interpretation flagged in the README and closeout):
//   voS      = the VO file's length when present, else the beat's script
//              target (the silence fallback "records" silence of the
//              script's target duration).
//   duration = max(footage, voS + tailFreezeS) — footage is never lost.
//   freeze   = duration - footage when positive: the last frame holds
//              while the voice finishes.
//   trim     = any footage beyond duration is cut from the TAIL, never the
//              head (head-anchored encode with an explicit -t). Under the
//              max() formula this is 0 by construction; it exists so a
//              future duration policy can only ever cut tails.

export interface BeatTiming {
  /** Final beat duration in seconds (before the inter-beat gap). */
  durationS: number;
  /** Seconds of last-frame freeze appended to the footage. */
  freezeS: number;
  /** Seconds trimmed off the footage tail (never the head). */
  trimS: number;
  /** The governing VO length (real file or silence target). */
  voS: number;
  /** True when no VO file exists — the beat gets generated silence and
   * the "VO PENDING" watermark. */
  silent: boolean;
}

const round3 = (n: number): number => Math.round(n * 1000) / 1000;

export function computeBeatTiming(
  footageS: number,
  voFileS: number | null,
  targetS: number,
  tailFreezeS = 0.5,
): BeatTiming {
  if (footageS < 0 || targetS <= 0) {
    throw new Error(`nonsense timing inputs: footage=${footageS} target=${targetS}`);
  }
  const silent = voFileS === null;
  const voS = silent ? targetS : voFileS;
  const durationS = round3(Math.max(footageS, voS + tailFreezeS));
  const freezeS = round3(Math.max(0, durationS - footageS));
  const trimS = round3(Math.max(0, footageS - durationS));
  return { durationS, freezeS, trimS, voS: round3(voS), silent };
}
