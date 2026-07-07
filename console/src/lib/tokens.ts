// APERTURE DESIGN TOKENS — the single source of every visual constant.
//
// THE RESERVED-COLOR LAW: the four neutrals + affordance below, plus the five
// labeled sensitivity hues, are the ONLY colors permitted anywhere in
// Aperture. Components never declare color, duration, or font literals; they
// import from here (or consume the CSS variables layout.tsx generates from
// here). U-6 enforces this mechanically over src/components and src/app.
//
// Material depth is centralized here. Components may opt into layers, but do
// not invent one-off color, shadow, blur, or elevation values.

export const COLOR = {
  /** App background — a faint cool-tinted off-white (not flat white), calm. */
  paper: "#F4F7FC",
  /** Primary text — deep navy-charcoal, never pure black. */
  ink: "#1A2233",
  /** Secondary text, rules; borders at 24% where hairline. */
  inkSoft: "#5A6478",
  /** Interactive elements ONLY — links, buttons, focus rings. The signature
   * blue-violet (periwinkle/indigo), deep enough to carry paper-white text at
   * AA. Never decorative. */
  affordance: "#4954C9",
} as const;

/** Derived washes of permitted colors (alpha only — no new hues). */
export const DERIVED = {
  /** The one elevation: a 1px hairline at ink-soft 24%. */
  hairline: "rgba(92, 92, 84, 0.24)",
  /** Hover/selection wash at ink-soft 8%. */
  wash: "rgba(92, 92, 84, 0.08)",
} as const;

/**
 * DARK THEME (the org-graph command surface defaults to it). A designed dark
 * palette — NOT an inversion: a rich, desaturated deep navy-charcoal paper
 * (never pure black — murkiness comes from flat near-black, fixed here with a
 * cool blue undertone and layered elevation), an off-white ink that kills
 * glare, a luminous periwinkle affordance legible on navy, and SOLID
 * hairline/wash tinted toward the navy (dark can't reuse the ink-soft alphas —
 * they vanish on the base). The five labeled sensitivity hues are shared
 * across themes (they read as luminous chips on dark, soft tints on light).
 * Selected via [data-theme="dark"] in layout.tsx; U-6 still sees every literal
 * HERE and nowhere else.
 */
export const DARK = {
  paper: "#0F1422",
  ink: "#E6EBF5",
  inkSoft: "#AEB8CC",
  affordance: "#93A7F2",
  hairline: "#2A3142",
  wash: "#1A2030",
} as const;

/**
 * The ONE warm accent — reserved for the lit connection path and the org
 * core's glow, nothing decorative. Shared by both themes (it carries on paper
 * and on near-black). A deliberate new hue, declared here so U-6 stays clean.
 */
export const ACCENT = {
  warm: "#C77F3A",
} as const;

/**
 * ATMOSPHERE — the cinematic ambient wash (the colour the surface was missing).
 * A desaturated blue->violet depth haze, applied ONLY to the backdrop and a
 * few hero surfaces (never painted on every panel), always at low opacity via
 * color-mix so it reads as volumetric light, not paint. Deliberately OUTSIDE
 * amber's hue range so the reserved governance signal (ACCENT.warm) stays
 * semantically distinct. Declared here so U-6 sees these two hues centrally.
 */
export const ATMOSPHERE = {
  blue: "#2D4A7C",
  violet: "#4A3A8C",
} as const;

/**
 * Premium material hierarchy. This is intentionally conservative: depth is
 * created through tone, hairline strength, and narrow shadows, not through
 * neon glow or blanket glass.
 */
export const MATERIAL = {
  light: {
    surface0: COLOR.paper,
    surface1: `color-mix(in srgb, ${COLOR.paper} 96%, ${COLOR.ink} 4%)`,
    surface2: `color-mix(in srgb, ${COLOR.paper} 91%, ${COLOR.ink} 9%)`,
    surface3: `color-mix(in srgb, ${COLOR.paper} 86%, ${COLOR.ink} 14%)`,
    focus: `color-mix(in srgb, ${COLOR.paper} 82%, ${COLOR.affordance} 18%)`,
    glass: `color-mix(in srgb, ${COLOR.paper} 74%, transparent)`,
    chip: `color-mix(in srgb, ${COLOR.paper} 92%, ${COLOR.inkSoft} 8%)`,
    hairlineStrong: "rgba(92, 92, 84, 0.38)",
    edge: "rgba(92, 92, 84, 0.14)",
    shadow1: "0 1px 0 rgba(92, 92, 84, 0.10), 0 20px 48px -32px rgba(92, 92, 84, 0.48)",
    shadow2: "0 1px 0 rgba(92, 92, 84, 0.14), 0 34px 82px -42px rgba(92, 92, 84, 0.58)",
    shadowFocus: "0 1px 0 rgba(92, 92, 84, 0.14), 0 42px 96px -48px rgba(92, 92, 84, 0.46)",
  },
  dark: {
    surface0: DARK.paper,
    surface1: `color-mix(in srgb, ${DARK.paper} 92%, ${DARK.ink} 8%)`,
    surface2: `color-mix(in srgb, ${DARK.paper} 86%, ${DARK.ink} 14%)`,
    surface3: `color-mix(in srgb, ${DARK.paper} 80%, ${DARK.ink} 20%)`,
    focus: `color-mix(in srgb, ${DARK.paper} 78%, ${DARK.affordance} 22%)`,
    glass: `color-mix(in srgb, ${DARK.paper} 68%, transparent)`,
    chip: `color-mix(in srgb, ${DARK.paper} 80%, ${DARK.inkSoft} 20%)`,
    hairlineStrong: "rgba(92, 92, 84, 0.52)",
    edge: "rgba(92, 92, 84, 0.18)",
    shadow1: "0 1px 0 rgba(92, 92, 84, 0.12), 0 28px 66px -36px rgba(92, 92, 84, 0.72)",
    shadow2: "0 1px 0 rgba(92, 92, 84, 0.18), 0 44px 104px -48px rgba(92, 92, 84, 0.84)",
    shadowFocus: "0 1px 0 rgba(92, 92, 84, 0.20), 0 50px 120px -52px rgba(92, 92, 84, 0.82)",
  },
  blur: "24px",
} as const;

/**
 * The five sensitivity hues — moved unchanged from the M3b console scale
 * (labeled, colorblind-safe Okabe–Ito). Labels always accompany color.
 */
export const SENSITIVITY_SCALE: Record<
  string,
  { label: string; background: string; border: string }
> = {
  public: { label: "public", background: "#E8F1F8", border: "#0072B2" },
  internal: { label: "internal", background: "#E6F4EF", border: "#009E73" },
  confidential: { label: "confidential", background: "#FBF1DC", border: "#E69F00" },
  restricted: { label: "restricted", background: "#F9E8DE", border: "#D55E00" },
  special_category: { label: "special category", background: "#F6EAF1", border: "#CC79A7" },
};

/**
 * AR-1: department → avatar disc tint, drawn ENTIRELY from the reserved
 * sensitivity palette above. No new hue enters the system (U-6 still sees
 * exactly the 14 hexes — these are references, not literals), avatars group
 * visually by department, and the palette is colorblind-safe by construction.
 * Eight departments over five hues: three hues each serve two departments —
 * this is grouping, not unique identity. Initials always render in ink.
 */
export const DEPARTMENT_TINT: Record<string, { background: string; border: string }> = {
  "Quality & Compliance": {
    background: SENSITIVITY_SCALE.internal.background,
    border: SENSITIVITY_SCALE.internal.border,
  },
  "Pharmacy Services": {
    background: SENSITIVITY_SCALE.public.background,
    border: SENSITIVITY_SCALE.public.border,
  },
  "Warehouse Operations": {
    background: SENSITIVITY_SCALE.confidential.background,
    border: SENSITIVITY_SCALE.confidential.border,
  },
  Finance: {
    background: SENSITIVITY_SCALE.restricted.background,
    border: SENSITIVITY_SCALE.restricted.border,
  },
  Executive: {
    background: SENSITIVITY_SCALE.special_category.background,
    border: SENSITIVITY_SCALE.special_category.border,
  },
  IT: {
    background: SENSITIVITY_SCALE.public.background,
    border: SENSITIVITY_SCALE.public.border,
  },
  HR: {
    background: SENSITIVITY_SCALE.special_category.background,
    border: SENSITIVITY_SCALE.special_category.border,
  },
  "Sales & Accounts": {
    background: SENSITIVITY_SCALE.confidential.background,
    border: SENSITIVITY_SCALE.confidential.border,
  },
} as const;

/** The neutral disc for a principal with no known department (the fallback —
 * ink-soft wash + hairline, both already derived from the reserved palette). */
export const AVATAR_FALLBACK_TINT = {
  background: DERIVED.wash,
  border: DERIVED.hairline,
} as const;

/**
 * THE GRAPH NEUTRAL RAMP (comprehension pass, B1/B4): department coding on the
 * Company/Operating Map uses ONLY this sensitivity-safe neutral ramp —
 * saturated color stays reserved for the sensitivity classes (and amber for
 * the lit connection path). Eight tonal steps of the theme ink over the theme
 * paper (CSS-var driven, so both themes resolve correctly). `surface` fills,
 * `line` strokes. Departments are distinguished primarily by POSITION (their
 * arc); the ramp is a quiet secondary cue, not an identity system.
 */
export const GRAPH_NEUTRAL_RAMP: ReadonlyArray<{ surface: string; line: string }> = [
  { surface: "color-mix(in srgb, var(--ink) 6%, var(--paper))", line: "color-mix(in srgb, var(--ink) 34%, var(--paper))" },
  { surface: "color-mix(in srgb, var(--ink) 8%, var(--paper))", line: "color-mix(in srgb, var(--ink) 38%, var(--paper))" },
  { surface: "color-mix(in srgb, var(--ink) 10%, var(--paper))", line: "color-mix(in srgb, var(--ink) 42%, var(--paper))" },
  { surface: "color-mix(in srgb, var(--ink) 12%, var(--paper))", line: "color-mix(in srgb, var(--ink) 46%, var(--paper))" },
  { surface: "color-mix(in srgb, var(--ink) 14%, var(--paper))", line: "color-mix(in srgb, var(--ink) 50%, var(--paper))" },
  { surface: "color-mix(in srgb, var(--ink) 16%, var(--paper))", line: "color-mix(in srgb, var(--ink) 54%, var(--paper))" },
  { surface: "color-mix(in srgb, var(--ink) 18%, var(--paper))", line: "color-mix(in srgb, var(--ink) 58%, var(--paper))" },
  { surface: "color-mix(in srgb, var(--ink) 20%, var(--paper))", line: "color-mix(in srgb, var(--ink) 62%, var(--paper))" },
] as const;

/** Deterministic ramp step for a department: by its index in the scoped
 * payload's department order (stable per payload; no hashing, no hue). */
export function graphRampStep(index: number): { surface: string; line: string } {
  return GRAPH_NEUTRAL_RAMP[((index % GRAPH_NEUTRAL_RAMP.length) + GRAPH_NEUTRAL_RAMP.length) % GRAPH_NEUTRAL_RAMP.length];
}

/**
 * SHOWCASE-1 (Track B): THE DEPARTMENT PASTEL FAMILY — an owner-ratified
 * AMENDMENT to the reserved-color law. These eight pastels are DEPARTMENT
 * IDENTITY colors on the admin Operating Map ONLY. Saturated amber/red remain
 * EXCLUSIVELY sensitivity + signal (amber = the lit connection / signals-
 * unavailable state); these pastels never touch a sensitivity surface. Each
 * clears 3:1 against the deep-navy canvas for its ring/arc/dot (non-text) use;
 * node labels stay in the standard text inks. Declared here so U-6 sees every
 * hue centrally.
 */
export const DEPARTMENT_PASTEL: ReadonlyArray<{ hex: string; keywords: string[] }> = [
  { hex: "#E8A0BF", keywords: ["hr", "people"] }, // People & HR
  { hex: "#E7C86E", keywords: ["finance"] }, // Finance
  { hex: "#9DC7A0", keywords: ["quality", "compliance"] }, // Quality & Compliance
  { hex: "#7FA8E8", keywords: ["sales", "account"] }, // Sales & Accounts
  { hex: "#B7A6E3", keywords: ["executive"] }, // Executive
  { hex: "#E8A76E", keywords: ["operations", "warehouse"] }, // Operations
  { hex: "#8FBFA8", keywords: ["logistics", "pharmacy"] }, // Logistics
  { hex: "#7EC8D8", keywords: ["it", "security"] }, // IT & Security
];

/**
 * Deterministic department → pastel assignment. Labels are matched to the
 * family by keyword (semantic intent) in SORTED label order so the result is
 * stable per payload; a label matching no family keyword takes the next
 * unused pastel in palette order. Every department gets a distinct pastel
 * (8 pastels, ≤8 departments in the corpus).
 */
export function departmentPastelMap(labels: string[]): Map<string, string> {
  const used = new Set<number>();
  const out = new Map<string, string>();
  const sorted = [...labels].sort((a, b) => a.localeCompare(b));
  // First pass: keyword-matched (semantic) assignment.
  for (const label of sorted) {
    const lower = label.toLowerCase();
    const idx = DEPARTMENT_PASTEL.findIndex(
      (p, i) => !used.has(i) && p.keywords.some((k) => lower.includes(k)),
    );
    if (idx >= 0) {
      used.add(idx);
      out.set(label, DEPARTMENT_PASTEL[idx].hex);
    }
  }
  // Second pass: any unmatched label takes the next unused pastel.
  for (const label of sorted) {
    if (out.has(label)) continue;
    const idx = DEPARTMENT_PASTEL.findIndex((_, i) => !used.has(i));
    const use = idx >= 0 ? idx : 0;
    used.add(use);
    out.set(label, DEPARTMENT_PASTEL[use].hex);
  }
  return out;
}

/**
 * SHOWCASE-1 (Track B): the Operating Map STAGE — a deep-navy radial field.
 * The reference's glow is radial-gradient fills + opacity, NEVER a blur filter
 * (the no-filter law holds). Declared here so U-6 sees the canvas hues.
 */
export const GRAPH_STAGE = {
  /** Radial field: center → edge. */
  canvasCenter: "#0D1526",
  canvasEdge: "#0A0F1E",
  /** The org core's soft blue glow (radial-gradient core → transparent). */
  coreGlow: "#6EA8FE",
  /** The rim's barely-there radial texture (person→center spokes). */
  rimSpoke: "#FFFFFF",
  /** The edge vignette — pure black at low opacity (a darkening wash, no hue). */
  vignette: "#000000",
  /**
   * MAP LABEL INK — pinned light-on-navy. The stage is fixed deep-navy in BOTH
   * app themes, so node labels must NOT inherit the theme ink (in light theme
   * var(--ink) is dark navy-charcoal → ~1.2:1 dark-on-navy, effectively
   * invisible). We reuse the DARK-theme inks (off-white / cool grey) — already
   * in the reserved palette, so U-6 sees no new hue — and pair them with the
   * navy canvasEdge halo for ≥4.5:1 in every theme by construction.
   */
  label: DARK.ink,
  labelSoft: DARK.inkSoft,
} as const;

/**
 * Sensitivity badge ink (B4): the five badge tints are FIXED pale hues shared
 * by both themes, so the label must NOT inherit the theme ink (near-white on
 * dark collapsed to 1.0:1 against the pale tints). Badge text is pinned to the
 * light-theme ink — ≥4.5:1 on every tint in BOTH themes by construction.
 */
export const SENSITIVITY_BADGE_INK = COLOR.ink;

/**
 * RADIUS MICRO-TOKENS (graph-presence pass, B4). The surface radius law stays
 * {8, 16, 9999} — `glyph` exists ONLY for SVG GLYPH INTERIORS (icon-scale
 * rects inside graph nodes, e.g. the ~14-20px agent square), where an 8px
 * corner would misdraw the mark. It is NOT a surface radius: no card, chip,
 * panel, button, or any HTML element may use it. The radius-law test allows
 * exactly this token inside SVG and nothing else.
 */
export const RADIUS = {
  glyph: 3,
} as const;

/** Type registers (bundled woff2; see src/fonts). */
export const FONT = {
  /** chrome/data: UI, tables, labels, scope chips. */
  chrome: "'Inter', ui-sans-serif, system-ui, sans-serif",
  /** answer voice: generated text ONLY — the model's voice is its own
   * register, visibly distinct from fact. */
  answer: "'Source Serif 4', Georgia, 'Times New Roman', serif",
  /** evidence: doc ids, rule chips, hashes, ordinals. */
  evidence: "'IBM Plex Mono', ui-monospace, 'Courier New', monospace",
} as const;

/** Type scale (px) and line heights. */
export const TYPE = {
  scale: { xs: 13, sm: 15, md: 18, lg: 24, xl: 32 },
  line: { body: 1.45, display: 1.2 },
} as const;

/** Fixed geometry (AP-2). The emblem and ego-graph are EMBLEMATIC, NOT
 * PROPORTIONAL — these numbers are identity, and area encodes nothing. */
export const GEOMETRY = {
  /** Ego-graph: square viewport, ring radius, node radius, node cap. */
  egoViewport: 320,
  egoRingRadius: 110,
  egoNodeRadius: 7,
  egoNodeCap: 21,
  /** Intersection emblem: two fixed fields with 40% overlap. */
  emblemFieldWidth: 120,
  emblemFieldHeight: 72,
  emblemOverlap: 0.4,
  /** AP-3 ADDITIONS (flagged in the AP-3 closeout): the Atlas room.
   * Capability sheet width (same anatomy as the doc inspector), the
   * card's evidence preview row cap, and the initiative column minimum. */
  atlasSheetWidth: 420,
  atlasPreviewRows: 3,
  atlasColumnMin: 260,
  /** THE ORG BRAIN — true CONCENTRIC RINGS, deterministic polar layout (no
   * force-scatter). From the still center outward: the org core; the 8
   * department hubs; the 4 agents; the 5 systems-of-record (sources); and ONE
   * continuous outer circle of all 120 people, ordered so each department owns
   * an unbroken arc whose angular WIDTH is proportional to its headcount —
   * area encodes how many people work there. Every node is a real entity. */
  graphViewport: 920,
  graphMargin: 60,
  /** The org sits slightly BELOW exact center (the still point, weighted). */
  graphCenterOffsetY: 16,
  graphCoreRadius: 30,
  /** Concentric ring radii (center outward). */
  graphRingDept: 150,
  graphRingAgents: 232,
  graphRingSources: 300,
  graphRingPeople: 396,
  /** Node sizes. Anchors (leadership) are prominent; members secondary. */
  graphHubRadius: 16,
  graphAgentSize: 12,
  graphSourceSize: 13,
  graphAnchorAvatar: 30,
  graphMemberAvatar: 15,
  /** A small angular gap (radians) between adjacent department arcs so the
   * colored arcs read as distinct sectors. */
  graphArcGap: 0.05,
  /** Zoom limits; member names appear once the view zooms past the reveal
   * scale (anchors are always named). */
  graphScaleMin: 0.45,
  graphScaleMax: 4,
  graphLodReveal: 1.7,
  /** Label legibility: a paper halo (stroke painted under the fill). */
  graphLabelHalo: 3,
  /** Ego-focus de-emphasis (A2: non-connected rests at 15%) and focus-mode
   * ghost (deeper). */
  graphDimOpacity: 0.15,
  graphGhostOpacity: 0.1,
} as const;

/** MOTION BUDGET — tokens, not vibes. Nothing else animates.
 * The budget (comprehension pass, B7): fades 120–180ms; the iris (≤240ms) is
 * the ONE choreographed motion in the product; ONE hover-lift repo-wide
 * (-2px over `lift` ease-out). */
export const MOTION = {
  /** hover, chip reveal */
  fadeQuick: "140ms",
  /** panel/sheet enter */
  fadeView: "180ms",
  /** the ONE hover-lift transition (B3): translateY(-2px), 150ms ease-out */
  lift: "150ms",
  /** the lens transition — the ONE choreographed motion in the product */
  iris: "220ms",
  easeOut: "ease-out",
  irisEase: "cubic-bezier(0.4, 0, 0.2, 1)",
  /** Framer Motion: calm enterprise micro-motion, in seconds. framerView sits
   * at the top of the 120–180ms fade budget (was 360ms — over budget). */
  framerQuick: 0.16,
  framerView: 0.18,
  framerStagger: 0.075,
  framerEnterY: 18,
  /** THE hover-lift (B3): one value repo-wide, shared with .ap-washable. */
  framerHoverY: -2,
  framerTapScale: 0.975,
  framerEase: [0.16, 1, 0.3, 1] as const,
  /**
   * Skeleton opacity pulse. REPORTED COLLISION: a literal fade-quick loop
   * (140ms/leg ≈ 3.6 flashes/sec) sits at the photosensitive-flash
   * boundary, so the pulse period is fadeQuick × 10 — same token, derived,
   * no shimmer. See the AP-1 closeout.
   */
  skeletonPulse: "1400ms",
} as const;
