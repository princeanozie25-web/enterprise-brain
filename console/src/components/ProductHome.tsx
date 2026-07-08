"use client";

import { useEffect, useState } from "react";
import { TYPE } from "@/lib/tokens";
import { suggestedQuestionFor } from "@/lib/firstQuestion";
import { takeReturnIntent, type ReturnIntent } from "@/lib/session";
import { MotionPanel, MotionSection } from "./MotionPrimitives";
import { PersonAvatar } from "./PersonAvatar";

/**
 * THE FRONT DOOR (comprehension pass, A2): a full-screen, calm identity
 * picker. One product name, one sentence, one decision — "Who are you
 * today?". No hardwired identity; the person picks, then Home (/me) renders
 * FOR that identity.
 *
 * The three featured identities are REAL fixture people (fixtures/people.json
 * + company.json): p060 Felix Osei (Finance head, rich scope), p088 Tomas
 * Reyes (HR associate, a different slice), and p_void Zara Castillo — a real
 * roster entry that deliberately holds no access, so fail-closed is
 * demonstrable, not asserted. The other 121 demo identities remain reachable
 * from the Work Identity switcher on every room.
 *
 * A4 note: this page's single demo-status line is the mandated picker
 * sub-line below the heading (verbatim from the pass brief).
 *
 * K3 Track 2: this is also the re-authentication landing. When a session
 * expires mid-use, the console routes here with `?expired=1` and a stashed
 * return intent. The picker announces the expiry (aria-live, calm register —
 * an expired session is a fact, not a failure) and rewrites each identity's
 * href to RESTORE the room + the staged (never auto-submitted) query.
 */
const FEATURED_IDENTITIES: ReadonlyArray<{
  id: string;
  name: string;
  role: string;
  department: string | null;
  hint: string;
  /** Showreel Track A: the journey's subject gets the visually-primary card.
   * Styling only — the href/behavior is identical to every other card. */
  primary?: boolean;
}> = [
  {
    id: "p060",
    name: "Felix Osei",
    role: "Finance head",
    department: "Finance",
    hint: "A rich scope: a full department slice of the map and its documents.",
    primary: true,
  },
  {
    id: "p088",
    name: "Tomas Reyes",
    role: "HR associate",
    department: "HR",
    hint: "A different slice: the same company, seen from HR.",
  },
  {
    id: "p_void",
    name: "Zara Castillo",
    role: "No access — see what fail-closed looks like",
    department: null,
    hint: "Ask the same question and watch it refused, calmly and honestly.",
  },
];

/** Build the restore href for an identity: the return room + staged query,
 * but ONLY when the SAME identity that staged it is re-picked — a query is
 * that identity's own content, so a different pick starts fresh at Home
 * (never carrying one identity's staged text onto another). No expiry intent
 * → the plain Home door (unchanged). */
function identityHref(id: string, intent: ReturnIntent | null): string {
  const as = encodeURIComponent(id);
  if (intent === null) return `/me?as=${as}`;
  if (intent.principal !== id) return `/me?as=${as}`;
  const base = intent.path && intent.path !== "/" ? intent.path : "/me";
  const q = intent.query ? `&q=${encodeURIComponent(intent.query)}` : "";
  return `${base}?as=${as}${q}`;
}

export function ProductHome() {
  // The expiry landing is client-only: the stashed intent is read (and
  // cleared) exactly once on mount, held in state for the render.
  const [expired, setExpired] = useState(false);
  const [intent, setIntent] = useState<ReturnIntent | null>(null);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (params.get("expired") === "1") {
      setExpired(true);
      setIntent(takeReturnIntent());
    }
  }, []);

  return (
    <main
      id="main"
      className="mx-auto flex min-h-[100dvh] max-w-3xl flex-col justify-center gap-6 px-4 py-10"
      data-testid="root-home"
    >
      <MotionPanel className="ap-hero rounded-2xl p-6 md:p-8">
        <h1
          className="ap-register-chrome"
          style={{ fontSize: TYPE.scale.xl, fontWeight: 600, lineHeight: TYPE.line.display }}
        >
          Enterprise Brain
        </h1>
        <p
          className="ap-soft mt-3"
          style={{ fontSize: TYPE.scale.md, lineHeight: TYPE.line.body }}
          data-testid="root-one-sentence"
        >
          Ask your company&apos;s knowledge. Every answer respects what you&apos;re allowed to see.
        </p>
      </MotionPanel>

      {/* K3 Track 2: the calm re-auth line, ABOVE the picker, aria-live so it
          announces the expiry + navigation on landing. Neutral register, no
          error styling — the color law holds. */}
      <p
        className="ap-soft"
        role="status"
        aria-live="polite"
        style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body, minHeight: expired ? undefined : 0 }}
        data-testid="session-expired-line"
      >
        {expired ? "Your session ended. Pick who you are to continue." : ""}
      </p>

      <MotionSection aria-labelledby="identity-picker-heading" data-testid="identity-picker">
        <h2
          id="identity-picker-heading"
          className="ap-register-chrome"
          style={{ fontSize: TYPE.scale.lg, fontWeight: 600, lineHeight: TYPE.line.display }}
        >
          Choose a work identity
        </h2>
        <p
          className="ap-soft mt-1"
          style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
          data-testid="identity-picker-demo-line"
        >
          Demo access — no account, no password. You&apos;ll see exactly what that person is
          authorized to see.
        </p>

        <ul className="mt-4 grid grid-cols-1 gap-3 md:grid-cols-2">
          {FEATURED_IDENTITIES.map((identity) => (
            <li key={identity.id} className={identity.primary ? "md:col-span-2" : undefined}>
              <a
                href={identityHref(identity.id, intent)}
                className={`${identity.primary ? "ap-focus-surface" : "ap-card"} ap-washable flex items-center gap-4 rounded-2xl border p-4`}
                data-testid={`identity-option-${identity.id}`}
              >
                <PersonAvatar
                  principalId={identity.id}
                  displayName={identity.name}
                  department={identity.department}
                  size={identity.primary ? 52 : 44}
                />
                <span className="min-w-0 flex-1">
                  <span
                    className="ap-register-chrome block"
                    style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
                  >
                    {identity.name}
                  </span>
                  <span className="ap-soft block" style={{ fontSize: TYPE.scale.xs }}>
                    {identity.role}
                  </span>
                  <span
                    className="ap-soft mt-1 block"
                    style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
                  >
                    {identity.hint} Try: &ldquo;{suggestedQuestionFor(identity.id)}&rdquo;
                  </span>
                </span>
                <span
                  className="ap-register-evidence ap-soft shrink-0"
                  style={{ fontSize: TYPE.scale.xs }}
                  aria-hidden="true"
                >
                  {identity.id}
                </span>
              </a>
            </li>
          ))}
        </ul>

        <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
          121 more demo identities are available from the Work Identity switcher on any room.
        </p>
      </MotionSection>
    </main>
  );
}
