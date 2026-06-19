import { TYPE } from "@/lib/tokens";
import { GuidedJourney } from "./GuidedJourney";
import { MotionAnchor, MotionArticle, MotionPanel, MotionSection } from "./MotionPrimitives";
import { BuyerTrustPosture, DemoIdentityNotice } from "./TrustPosture";

const DESTINATIONS = [
  {
    detail: "Begin with a selected Work Identity so the console has a permission scope for work, access, and knowledge.",
    href: "/me?as=p060",
    label: "Work Identity",
    testid: "root-link-me",
  },
  {
    detail: "Review real workflow only after a capability is selected from Work Identity or the Operating Map.",
    href: "/project",
    label: "Workflow Command",
    testid: "root-link-project",
  },
  {
    detail: "Ask within the selected Work Identity, with granted context sent for server validation when present.",
    href: "/ask?as=p060",
    label: "Permission-aware Ask",
    testid: "root-link-ask",
  },
  {
    detail: "Open the admin-side operating map as a pilot preview. Production admin authority is not connected in this build.",
    href: "/admin/graph?as=p060",
    label: "Operating Map Preview",
    testid: "root-link-admin-graph",
  },
  {
    detail: "Open the admin, finance, and executive-domain spend room. Neither a ledger fixture nor Bursar authority is connected here.",
    href: "/admin/bursar",
    label: "Bursar Ledger Room Preview",
    testid: "root-link-admin-bursar",
  },
];

const PRINCIPLES = [
  {
    label: "Governed knowledge",
    text: "Enterprise Brain controls which knowledge can be used by a selected Work Identity.",
  },
  {
    label: "Governed workflows",
    text: "Workflow Command shows real work, requests, approvals, and grants without inventing rows.",
  },
  {
    label: "Governed spend",
    text: "Bursar frames what the model may spend: authorization before action, audit before effect.",
  },
];

export function ProductHome() {
  return (
    <main
      className="mx-auto flex min-h-[100dvh] max-w-6xl flex-col justify-center gap-5 px-4 py-8 md:py-10"
      data-testid="root-home"
    >
      <header className="grid gap-5">
        <MotionPanel className="ap-card ap-elevated max-w-3xl rounded p-5 md:p-7">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Enterprise Brain
          </p>
          <h1
            className="ap-register-chrome mt-2"
            style={{ fontSize: TYPE.scale.xl, fontWeight: 600, lineHeight: TYPE.line.display }}
          >
            Company Operating System
          </h1>
          <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.md, lineHeight: TYPE.line.body }}>
            Governed knowledge, governed workflows, permission-aware Ask, the Operating Map, and
            the Bursar Ledger Room in one calm operating surface.
          </p>
          <p className="ap-soft mt-3 max-w-2xl" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
            Enterprise Brain governs what the model may know and do. Bursar governs what the model
            may spend. Start with a Work Identity, then move through work, access, answers, the map,
            and spend governance.
          </p>
        </MotionPanel>
      </header>

      <GuidedJourney adminLinks current="home" principal={null} testId="root-demo-flow" />

      <DemoIdentityNotice context="standard" testId="root-demo-identity-mode" />

      <BuyerTrustPosture testId="root-buyer-trust-posture" />

      <MotionSection className="grid grid-cols-1 gap-3 md:grid-cols-[1.15fr_0.95fr_1.05fr]" aria-label="Product doctrine" delayIndex={2}>
        {PRINCIPLES.map((principle, index) => (
          <MotionArticle key={principle.label} className="ap-card rounded p-4" delayIndex={2 + index}>
            <h2 className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
              {principle.label}
            </h2>
            <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
              {principle.text}
            </p>
          </MotionArticle>
        ))}
      </MotionSection>

      <section className="grid grid-cols-1 gap-3 md:grid-cols-2" aria-label="Product routes">
        {DESTINATIONS.map((destination, index) => (
          <MotionAnchor
            key={destination.href}
            href={destination.href}
            className="ap-card ap-washable block min-h-36 rounded p-4 md:p-5"
            data-testid={destination.testid}
            delayIndex={index + 5}
          >
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.lg, fontWeight: 600 }}>
                  {destination.label}
                </p>
                <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
                  {destination.detail}
                </p>
              </div>
              <span
                className="ap-chip ap-register-evidence rounded px-2 py-1"
                style={{ fontSize: TYPE.scale.xs }}
                aria-hidden="true"
              >
                Open
              </span>
            </div>
          </MotionAnchor>
        ))}
      </section>

      <MotionSection className="ap-card ap-glass rounded p-3" data-testid="root-admin-note" delayIndex={10}>
        <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
          Authorization boundary
        </p>
        <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
          Operating Map and Bursar Ledger Room are route-separated admin-side previews. Production
          admin authority, Bursar authority, and ledger producers are not connected in this build.
        </p>
      </MotionSection>
    </main>
  );
}
