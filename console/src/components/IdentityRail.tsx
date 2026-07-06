import type { ScopeStatement } from "@/lib/api";
import { TYPE } from "@/lib/tokens";

/**
 * The identity rail, simplified for AP-1: the principal switcher and the
 * demo caption now live in the LensBar; the rail keeps the always-visible
 * "What I can see" scope panel — the honesty line as furniture. (The Lens
 * room itself is AP-2; scope chips here anticipate the masthead pattern.)
 */
export function IdentityRail({
  principal,
  scope,
}: {
  principal: string | null;
  /** null while loading or when no principal is selected. */
  scope: ScopeStatement | null;
}) {
  return (
    <div className="flex h-full flex-col gap-3" data-testid="identity-rail">
      <div className="ap-card rounded-lg p-3" data-testid="scope-panel">
        {/* B2: the rail is wayfinding furniture, not a document section — a
            styled label, so the room's h1 stays the first heading. */}
        <p
          className="ap-soft uppercase tracking-wide"
          style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
        >
          What I can see
        </p>
        {principal === null ? (
          <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.sm }} data-testid="rail-empty-state">
            Choose a Work Identity to see granted scope.
          </p>
        ) : scope === null ? (
          <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.sm }}>
            Loading scope…
          </p>
        ) : (
          <div className="mt-2 space-y-2" style={{ fontSize: TYPE.scale.xs }}>
            <ScopeChips label="Groups" values={scope.groups} />
            <ScopeChips label="Sites" values={scope.sites} />
            <div>
              <span className="ap-soft">Band</span>
              <div className="mt-1">
                {scope.band === null ? (
                  <span className="ap-soft">none</span>
                ) : (
                  <span className="ap-card ap-register-chrome inline-block rounded-lg px-1.5 py-0.5">
                    {scope.band}
                  </span>
                )}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function ScopeChips({ label, values }: { label: string; values: string[] }) {
  return (
    <div>
      <span className="ap-soft">{label}</span>
      <div className="mt-1 flex flex-wrap gap-1">
        {values.length === 0 ? (
          <span className="ap-soft">none</span>
        ) : (
          values.map((value) => (
            <span
              key={value}
              className="ap-card ap-register-chrome inline-block rounded-lg px-1.5 py-0.5"
              style={{ fontWeight: 500 }}
              data-testid="scope-chip"
            >
              {value}
            </span>
          ))
        )}
      </div>
    </div>
  );
}
