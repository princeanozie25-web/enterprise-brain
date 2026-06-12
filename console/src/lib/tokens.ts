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
