import { COLOR, DERIVED, SENSITIVITY_SCALE, TYPE } from "@/lib/tokens";

/**
 * The fixed five-level sensitivity badge. Always labeled — color is never
 * the only signal. The five hues live in tokens.ts (the reserved-color
 * law); an unknown level falls back to neutral ink-soft.
 */
export function SensitivityBadge({ sensitivity }: { sensitivity: string }) {
  const scale = SENSITIVITY_SCALE[sensitivity] ?? {
    label: sensitivity,
    background: DERIVED.wash,
    border: COLOR.inkSoft,
  };
  return (
    <span
      className="ap-register-chrome inline-block rounded border px-1.5 py-0.5 leading-none"
      style={{
        fontSize: TYPE.scale.xs,
        fontWeight: 500,
        backgroundColor: scale.background,
        borderColor: scale.border,
      }}
      data-testid="sensitivity-badge"
    >
      {scale.label}
    </span>
  );
}
