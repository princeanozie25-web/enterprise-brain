import { TYPE } from "@/lib/tokens";

const DESTINATIONS = [
  {
    detail: "Open the daily work surface for the selected demo principal.",
    href: "/me",
    label: "Me",
    testid: "root-link-me",
  },
  {
    detail: "Open a capability execution surface. Project links usually come from My Projects.",
    href: "/project",
    label: "Project",
    testid: "root-link-project",
  },
  {
    detail: "Use the existing scoped question surface.",
    href: "/ask",
    label: "Ask",
    testid: "root-link-ask",
  },
  {
    detail: "Open the relationship graph as a derived preview, not a server-enforced admin gate.",
    href: "/admin/graph",
    label: "Admin Graph",
    testid: "root-link-admin-graph",
  },
];

export function ProductHome() {
  return (
    <main className="mx-auto flex min-h-screen max-w-6xl flex-col justify-center gap-5 px-4 py-8" data-testid="root-home">
      <header className="max-w-3xl">
        <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
          Enterprise Brain
        </p>
        <h1
          className="ap-register-chrome mt-2"
          style={{ fontSize: TYPE.scale.xl, fontWeight: 600, lineHeight: TYPE.line.display }}
        >
          Choose the surface for the work you are doing.
        </h1>
        <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
          Employee work, project execution, questions, and the admin graph are separate routes. This
          home route does not infer access or redirect by role.
        </p>
      </header>

      <section className="grid grid-cols-1 gap-3 md:grid-cols-2" aria-label="Product routes">
        {DESTINATIONS.map((destination) => (
          <a
            key={destination.href}
            href={destination.href}
            className="ap-card ap-washable block rounded p-4"
            data-testid={destination.testid}
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
          </a>
        ))}
      </section>

      <section className="ap-card rounded p-3" data-testid="root-admin-note">
        <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
          Admin enforcement note
        </p>
        <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
          Admin Graph is route-separated, but admin access is still marked as derived-only until a
          server-enforced authorization primitive exists.
        </p>
      </section>
    </main>
  );
}
