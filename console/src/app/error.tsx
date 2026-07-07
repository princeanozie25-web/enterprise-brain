"use client";

// K3 Track 3 — the app-level error boundary (Next convention). The last line
// of defense if an exception escapes every RoomBoundary: still a CALM neutral
// card, never a white screen, never red. It reports to no network endpoint.
// `reset` is Next's remount hook — the app retries without a full reload.

import { TYPE } from "@/lib/tokens";

export default function AppError({ reset }: { error: Error; reset: () => void }) {
  return (
    <main
      id="main"
      className="mx-auto flex min-h-[100dvh] max-w-3xl flex-col justify-center gap-4 px-4 py-10"
      data-testid="app-error"
    >
      <section className="ap-card rounded-lg p-6" role="alert" aria-live="polite">
        <p
          className="ap-register-chrome"
          style={{ fontSize: TYPE.scale.md, fontWeight: 600, lineHeight: TYPE.line.body }}
        >
          The console hit an error. Nothing was sent anywhere.
        </p>
        <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
          This is a local pilot workspace. Reloading starts the console again from a clean state.
        </p>
        <button
          type="button"
          onClick={reset}
          className="ap-washable ap-register-chrome mt-4 rounded-lg border px-3 py-2"
          style={{ borderColor: "var(--hairline)", fontSize: TYPE.scale.xs, fontWeight: 600 }}
          data-testid="app-error-reset"
        >
          Reload the console
        </button>
      </section>
    </main>
  );
}
