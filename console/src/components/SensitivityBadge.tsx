import { SENSITIVITY_BADGE_INK, SENSITIVITY_SCALE, TYPE } from "@/lib/tokens";

/**
 * The fixed five-level sensitivity badge. Always labeled — color is never
 * the only signal. The five hues live in tokens.ts (the reserved-color
 * law); an unknown level falls back to the theme chip surface.
 *
 * B4 (comprehension pass): the five tint backgrounds are FIXED pale hues
 * shared across themes, so the label ink is PINNED to the light-theme ink
 * (SENSITIVITY_BADGE_INK) rather than inheriting the theme ink — the theme
 * ink is near-white in dark mode and collapsed to ~1.0:1 on the pale tints.
 * Pinned, every known level is ≥4.5:1 in BOTH themes (tested in T-B1).
 */
export function SensitivityBadge({ sensitivity }: { sensitivity: string }) {
  const scale = SENSITIVITY_SCALE[sensitivity];
  if (!scale) {
    // Unknown level: theme-aware neutral chip (theme ink on theme chip
    // surface — both sides move together, AA in both themes).
    return (
      <span
        className="ap-chip ap-register-chrome inline-block rounded-lg border px-1.5 py-0.5 leading-none"
        style={{ fontSize: TYPE.scale.xs, fontWeight: 500, color: "var(--ink)" }}
        data-testid="sensitivity-badge"
      >
        {sensitivity}
      </span>
    );
  }
  return (
    <span
      className="ap-register-chrome inline-block rounded-lg border px-1.5 py-0.5 leading-none"
      style={{
        fontSize: TYPE.scale.xs,
        fontWeight: 500,
        backgroundColor: scale.background,
        borderColor: scale.border,
        color: SENSITIVITY_BADGE_INK,
      }}
      data-testid="sensitivity-badge"
    >
      {scale.label}
    </span>
  );
}
