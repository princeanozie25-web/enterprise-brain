import { TYPE } from "@/lib/tokens";
import { MotionAnchor, MotionArticle, MotionPanel, MotionSection } from "./MotionPrimitives";

const DESTINATIONS = [
  {
    detail: "Start with the selected actor's governed work surface, visible scope, requests, and Granted Knowledge.",
    href: "/me",
    label: "Work Identity",
    testid: "root-link-me",
  },
  {
    detail: "Open project workflow only when a real capability id is carried from Work Identity or the Operating Map.",
    href: "/project",
    label: "Workflow Command",
    testid: "root-link-project",
  },
  {
    detail: "Ask within the selected lens, with granted context sent for server validation when present.",
    href: "/ask",
    label: "Permission-aware Ask",
    testid: "root-link-ask",
  },
  {
    detail: "View the operating map as a derived admin preview. It is not a server-enforced admin gate yet.",
    href: "/admin/graph",
    label: "Operating Map",
    testid: "root-link-admin-graph",
  },
  {
    detail: "Open the governed spend axis. No connected ledger fixture or enforced Bursar role exists in this UI lane yet.",
    href: "/admin/bursar",
    label: "Bursar Ledger Room",
    testid: "root-link-admin-bursar",
  },
];

const PRINCIPLES = [
  {
    label: "Governed knowledge",
    text: "Enterprise Brain controls what the model may know and which context can be used.",
  },
  {
    label: "Governed workflows",
    text: "Workflow Command follows real capability and access-request state already exposed by the APIs.",
  },
  {
    label: "Governed spend",
    text: "Bursar frames what the model may spend: authorization before action, audit before effect.",
  },
];

const DEMO_FLOW = [
  "Work Identity",
  "Workflow Command",
  "Request Access",
  "Granted Knowledge",
  "Ask",
  "Operating Map",
  "Bursar Ledger Room",
];

export function ProductHome() {
  return (
    <main
      className="mx-auto flex min-h-[100dvh] max-w-6xl flex-col justify-center gap-5 px-4 py-8"
      data-testid="root-home"
    >
      <header className="grid gap-5 lg:grid-cols-[1.16fr_0.84fr] lg:items-end">
        <MotionPanel className="max-w-3xl">
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
            the Bursar Ledger Room live as one command surface.
          </p>
          <p className="ap-soft mt-3 max-w-2xl" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
            Enterprise Brain governs what the model may know and do. Bursar governs what the model
            may spend. This route never infers access; it gives the pilot a safe map through the
            product.
          </p>
        </MotionPanel>
        <MotionPanel className="ap-card rounded p-3" data-testid="root-demo-flow" delayIndex={1}>
          <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            Pilot path
          </p>
          <div className="mt-2 flex flex-wrap gap-1.5">
            {DEMO_FLOW.map((step) => (
              <span
                key={step}
                className="ap-hairline ap-register-evidence ap-soft rounded border px-2 py-1"
                style={{ fontSize: TYPE.scale.xs }}
              >
                {step}
              </span>
            ))}
          </div>
        </MotionPanel>
      </header>

      <MotionSection className="grid grid-cols-1 gap-3 md:grid-cols-[1.15fr_0.95fr_1.05fr]" aria-label="Product doctrine" delayIndex={2}>
        {PRINCIPLES.map((principle, index) => (
          <MotionArticle key={principle.label} className="ap-card rounded p-3" delayIndex={2 + index}>
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
            className="ap-card ap-washable block min-h-36 rounded p-4"
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
                className="ap-register-evidence ap-soft ap-hairline rounded border px-2 py-1"
                style={{ fontSize: TYPE.scale.xs }}
                aria-hidden="true"
              >
                Open
              </span>
            </div>
          </MotionAnchor>
        ))}
      </section>

      <MotionSection className="ap-card rounded p-3" data-testid="root-admin-note" delayIndex={10}>
        <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
          Authorization boundary
        </p>
        <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
          Operating Map and Bursar Ledger Room are route-separated. Admin authority and governed
          spend authority remain derived-only in this UI lane until server-enforced authorization
          and ledger producers are connected.
        </p>
      </MotionSection>
    </main>
  );
}
