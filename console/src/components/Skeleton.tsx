/** Loading is a quiet skeleton — never a spinner with numbers, never
 * "searching N documents". */
export function Skeleton({ lines = 3 }: { lines?: number }) {
  return (
    <div className="animate-pulse space-y-2" data-testid="skeleton" aria-busy="true">
      {Array.from({ length: lines }, (_, i) => (
        <div
          key={i}
          className="h-3 rounded bg-stone-200"
          style={{ width: `${88 - i * 14}%` }}
        />
      ))}
    </div>
  );
}
