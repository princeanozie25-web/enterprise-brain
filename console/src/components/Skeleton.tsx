import { DERIVED } from "@/lib/tokens";

/** Loading is a quiet opacity pulse — no shimmer, no spinner, never a
 * number. (Pulse period is the derived skeleton token: see the reported
 * collision note in tokens.ts.) */
export function Skeleton({ lines = 3 }: { lines?: number }) {
  return (
    <div className="ap-skeleton-pulse space-y-2" data-testid="skeleton" aria-busy="true">
      {Array.from({ length: lines }, (_, i) => (
        <div
          key={i}
          className="h-3 rounded"
          style={{ width: `${88 - i * 14}%`, backgroundColor: DERIVED.wash }}
        />
      ))}
    </div>
  );
}
