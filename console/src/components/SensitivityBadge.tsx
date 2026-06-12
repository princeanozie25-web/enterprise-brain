import { SENSITIVITY_SCALE } from "@/lib/constants";

/**
 * The fixed five-level sensitivity badge. Always labeled — color is never
 * the only signal (colorblind-safe Okabe–Ito scale).
 */
export function SensitivityBadge({ sensitivity }: { sensitivity: string }) {
  const scale = SENSITIVITY_SCALE[sensitivity] ?? {
    label: sensitivity,
    background: "#f5f5f4",
    border: "#78716c",
  };
  return (
    <span
      className="inline-block rounded border px-1.5 py-0.5 text-[11px] font-medium leading-none"
      style={{ backgroundColor: scale.background, borderColor: scale.border }}
      data-testid="sensitivity-badge"
    >
      {scale.label}
    </span>
  );
}
