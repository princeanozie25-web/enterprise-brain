"use client";

import { TYPE } from "@/lib/tokens";

/**
 * SHOWREEL TRACK A — the cinematic entry screen (the cold open).
 *
 * This is CHROME, not authentication: a branded arrival frame that sets up
 * the honest demo-identity choice with confidence. The backdrop is a STILL
 * capture of Felix's live Operating Map (console/public/entry-plate.png,
 * sourced from docs/reference/plate-felix-map.png) behind a heavy scrim +
 * overlay glass — the org map is visibly the product without being
 * interactive, and a still image can never flake under the AUTH-4 session
 * cap while filming. Never a live graph here.
 *
 * Laws in force: no ambient motion (a still, confident frame); the CTA uses
 * the affordance register, NEVER amber (amber stays signal); every label
 * tells the truth (this is a demo on a synthetic company, not a deployment);
 * no credential affordance of any kind — real OAuth is K2's deployment
 * slice, against a real directory, later.
 */
export function EntryScreen({ onEnter }: { onEnter: () => void }) {
  return (
    <main
      id="main"
      data-testid="entry-screen"
      className="relative flex min-h-[100dvh] flex-col overflow-hidden"
    >
      {/* The hero backdrop: still plate + scrim, pure decoration. */}
      <div aria-hidden="true" className="absolute inset-0">
        {/* eslint-disable-next-line @next/next/no-img-element -- static export
            serves the still plate directly; next/image adds nothing here. */}
        <img src="/entry-plate.png" alt="" className="ap-entry-plate" />
        {/* Overlay glass (the lawful scrim class) + the entry's own
            darkening gradients — composed, so the glass law's two-class
            allowlist stays exact. */}
        <div className="ap-glass-scrim absolute inset-0" />
        <div className="ap-entry-scrim" />
      </div>

      <div className="relative z-10 mx-auto flex w-full max-w-2xl flex-1 flex-col items-center justify-center gap-5 px-6 text-center">
        <h1
          className="ap-register-chrome"
          style={{
            fontSize: TYPE.scale.display,
            fontWeight: 700,
            lineHeight: TYPE.line.display,
          }}
        >
          Enterprise Brain
        </h1>
        <p
          style={{ fontSize: TYPE.scale.md, fontWeight: 450, lineHeight: TYPE.line.body }}
          data-testid="entry-thesis"
        >
          The governed brain — every answer respects exactly what you&apos;re allowed to see.
        </p>
        <p
          className="ap-soft"
          style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}
          data-testid="entry-honesty-line"
        >
          A working demo on a synthetic company. Scope-proven, not certified secure.
        </p>
        <button
          type="button"
          onClick={onEnter}
          className="ap-affordance-button ap-register-chrome mt-2 rounded-full px-8 py-3"
          style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
          data-testid="entry-cta"
        >
          Enter the demo
        </button>
      </div>

      <p
        className="ap-soft relative z-10 pb-6 text-center"
        style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
        data-testid="entry-bottom-strip"
      >
        Governed stack · authorize before the act · prove it on demand
      </p>
    </main>
  );
}
