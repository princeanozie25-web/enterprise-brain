// APERTURE DESIGN TOKENS — the single source of every visual constant.
//
// THE RESERVED-COLOR LAW: the four neutrals + affordance below, plus the five
// labeled sensitivity hues, are the ONLY colors permitted anywhere in
// Aperture. Components never declare color, duration, or font literals; they
// import from here (or consume the CSS variables layout.tsx generates from
// here). U-6 enforces this mechanically over src/components and src/app.
//
// No gradients. No shadows beyond the 1px hairline. Whitespace is the
// elevation system.

export const COLOR = {
  /** App background. */
  paper: "#FAFAF7",
  /** Primary text. */
  ink: "#16160F",
  /** Secondary text, rules; borders at 24% where hairline. */
  inkSoft: "#5C5C54",
  /** Interactive elements ONLY — links, buttons, focus rings. Never
   * decorative. */
  affordance: "#3D5A80",
} as const;

/** Derived washes of permitted colors (alpha only — no new hues). */
export const DERIVED = {
  /** The one elevation: a 1px hairline at ink-soft 24%. */
  hairline: "rgba(92, 92, 84, 0.24)",
  /** Hover/selection wash at ink-soft 8%. */
  wash: "rgba(92, 92, 84, 0.08)",
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
  /** AR-2 — the Org Graph (rebuilt). A force-directed company map: the org at
   * the still center, department "districts" as soft tinted fields, the
   * humanized people settled by a REAL simulation (charge + reporting links +
   * a pull toward each district's centroid + collision sized to the LABEL, not
   * just the disc), tools beside the cluster that owns them. The layout runs
   * once to equilibrium and is FROZEN — deterministic (seeded spiral, no
   * Math.random), no perpetual jitter. PROPORTIONAL, not emblematic: a
   * department's footprint (district radius + hub weight) grows with its
   * headcount — here, area DOES encode how many people work there. */
  graphViewport: 820,
  graphMargin: 40,
  /** Department hubs sit on this ring; their people cluster organically about
   * the hub. The hubs are radial (a sensible org skeleton); the PEOPLE are
   * not snapped to any arc — the simulation places them. */
  graphRingDept: 215,
  graphCenterRadius: 30,
  graphHubRadius: 15,
  graphAnchorAvatar: 34,
  graphMemberAvatar: 18,
  graphToolSize: 13,
  /** The force model (tokens, not vibes): repulsion strength, reporting-link
   * rest length + strength, the pull toward a node's district centroid,
   * collision padding around a node's footprint, solver iterations, and the
   * total settle ticks run before the layout freezes. */
  graphCharge: -94,
  graphLinkDistance: 48,
  graphLinkStrength: 0.3,
  graphClusterStrength: 0.16,
  graphCollidePad: 7,
  graphCollideIters: 4,
  graphForceTicks: 320,
  /** Level of detail: members reveal their name once the view zooms past this
   * scale (anchors are always named); below it, members are avatars only. */
  graphLodReveal: 1.8,
  /** Label legibility: a paper halo (stroke painted under the fill) of this
   * width keeps a name readable over edges and discs. */
  graphLabelHalo: 3,
  /** District field: the soft dept-tinted disc behind a cluster — radius
   * derived from headcount (this base + per-head growth, the √-scaled
   * footprint), drawn at low alpha so names sit legibly on top. */
  graphDistrictBase: 40,
  graphDistrictPerHead: 12,
  graphDistrictOpacity: 0.5,
  /** Focus de-emphasis: unrelated nodes drop to this opacity on hover/focus. */
  graphDimOpacity: 0.16,
} as const;

/** MOTION BUDGET — tokens, not vibes. Nothing else animates. */
export const MOTION = {
  /** hover, chip reveal */
  fadeQuick: "140ms",
  /** panel/sheet enter */
  fadeView: "180ms",
  /** the lens transition — the ONE choreographed motion in the product */
  iris: "220ms",
  easeOut: "ease-out",
  irisEase: "cubic-bezier(0.4, 0, 0.2, 1)",
  /**
   * Skeleton opacity pulse. REPORTED COLLISION: a literal fade-quick loop
   * (140ms/leg ≈ 3.6 flashes/sec) sits at the photosensitive-flash
   * boundary, so the pulse period is fadeQuick × 10 — same token, derived,
   * no shimmer. See the AP-1 closeout.
   */
  skeletonPulse: "1400ms",
} as const;
