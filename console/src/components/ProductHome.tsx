import { TYPE } from "@/lib/tokens";
import { suggestedQuestionFor } from "@/lib/firstQuestion";
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
 */
const FEATURED_IDENTITIES: ReadonlyArray<{
  id: string;
  name: string;
  role: string;
  department: string | null;
  hint: string;
}> = [
  {
    id: "p060",
    name: "Felix Osei",
    role: "Finance head",
    department: "Finance",
    hint: "A rich scope: a full department slice of the map and its documents.",
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

export function ProductHome() {
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

      <MotionSection aria-labelledby="identity-picker-heading" data-testid="identity-picker">
        <h2
          id="identity-picker-heading"
          className="ap-register-chrome"
          style={{ fontSize: TYPE.scale.lg, fontWeight: 600, lineHeight: TYPE.line.display }}
        >
          Who are you today?
        </h2>
        <p
          className="ap-soft mt-1"
          style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
          data-testid="identity-picker-demo-line"
        >
          Demo mode: sign in as anyone — no password. View-as is open to everyone. Nothing here is
          deployed.
        </p>

        <ul className="mt-4 grid grid-cols-1 gap-3">
          {FEATURED_IDENTITIES.map((identity) => (
            <li key={identity.id}>
              <a
                href={`/me?as=${encodeURIComponent(identity.id)}`}
                className="ap-card ap-washable flex items-center gap-4 rounded-2xl border p-4"
                data-testid={`identity-option-${identity.id}`}
              >
                <PersonAvatar
                  principalId={identity.id}
                  displayName={identity.name}
                  department={identity.department}
                  size={44}
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
