"use client";

import { GRAPH_STAGE, STAGE_DRESSING } from "@/lib/tokens";

/**
 * SHOWCASE-2 (Track B6) — the pipeline stage's constellation backdrop. PURE
 * DECORATION: aria-hidden, pointer-events none, no data-id, deliberately
 * non-node-like (radii ≤1.5px, faint). It is the ONLY non-payload layer on the
 * stage and it opts out of the F6 real-entity law by being unmistakably fabric,
 * not data. One token (STAGE_DRESSING.opacity) governs its visibility; no new
 * hue (dots reuse rimSpoke, links reuse coreGlow); it never animates, so
 * reduced motion is irrelevant. Deterministic: a fixed-seed LCG makes the field
 * identical every render (stable SSR / tests / screenshots).
 */
const W = 1600;
const H = 900;
const MAX_R = 1.5;

function starField() {
  let seed = 20260707;
  const rnd = () => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed / 0x7fffffff;
  };
  const stars = Array.from({ length: 88 }, () => ({
    x: Math.round(rnd() * W * 10) / 10,
    y: Math.round(rnd() * H * 10) / 10,
    r: Math.round((0.5 + rnd() * (MAX_R - 0.5)) * 100) / 100,
  }));
  // Sparse links: join a star to the next only when they are already near —
  // thin threads, never a lattice that could read as edges.
  const links: Array<{ x1: number; y1: number; x2: number; y2: number }> = [];
  for (let i = 0; i < stars.length - 1; i++) {
    const a = stars[i];
    const b = stars[i + 1];
    if (Math.hypot(a.x - b.x, a.y - b.y) < 190) {
      links.push({ x1: a.x, y1: a.y, x2: b.x, y2: b.y });
    }
  }
  return { stars, links };
}

const FIELD = starField();

export function ConstellationBackdrop() {
  return (
    <svg
      viewBox={`0 0 ${W} ${H}`}
      preserveAspectRatio="xMidYMid slice"
      aria-hidden="true"
      focusable="false"
      className="pointer-events-none absolute inset-0 h-full w-full"
      style={{ opacity: STAGE_DRESSING.opacity }}
      data-testid="pipeline-constellation"
    >
      <g stroke={GRAPH_STAGE.coreGlow} strokeWidth={0.5}>
        {FIELD.links.map((link, index) => (
          <line key={`l${index}`} x1={link.x1} y1={link.y1} x2={link.x2} y2={link.y2} />
        ))}
      </g>
      <g fill={GRAPH_STAGE.rimSpoke}>
        {FIELD.stars.map((star, index) => (
          <circle key={`s${index}`} cx={star.x} cy={star.y} r={star.r} />
        ))}
      </g>
    </svg>
  );
}
