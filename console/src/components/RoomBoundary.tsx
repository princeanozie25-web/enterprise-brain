"use client";

import { Component, type ReactNode } from "react";
import { TYPE } from "@/lib/tokens";

// K3 Track 3 — the shared room error boundary.
//
// A render exception in one room becomes a CALM neutral card, never a white
// screen and never the loss of the shell (nav, demo banner, scope masthead
// stay rendered because the boundary wraps only the room BODY). Color law
// holds even here: saturated color is sensitivity-only, so there is NO red
// error styling — a neutral ap-card and plain words. "Reload this room"
// REMOUNTS the body (bumps a key), never a full-page reload. The boundary
// reports to NO network endpoint — local-first doctrine, zero telemetry.

type Props = {
  /** Names the room for the aria/label only ("This room hit an error"). */
  label?: string;
  children: ReactNode;
};

type State = { failed: boolean; nonce: number };

export class RoomBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { failed: false, nonce: 0 };
  }

  static getDerivedStateFromError(): Partial<State> {
    return { failed: true };
  }

  // Deliberately swallowed: no telemetry, no network. componentDidCatch exists
  // only so React marks the error handled; the info is intentionally dropped.
  componentDidCatch(): void {
    // no-op (local-first: a boundary reports nothing anywhere).
  }

  private reload = () => {
    // Remount the body under a fresh key; the shell never blinks.
    this.setState((s) => ({ failed: false, nonce: s.nonce + 1 }));
  };

  render(): ReactNode {
    if (this.state.failed) {
      return (
        <section
          className="ap-card rounded-lg p-4"
          role="alert"
          aria-live="polite"
          data-testid="room-boundary-card"
        >
          <p
            className="ap-register-chrome"
            style={{ fontSize: TYPE.scale.sm, fontWeight: 600, lineHeight: TYPE.line.body }}
          >
            This room hit an error. The rest of the console is unaffected.
          </p>
          <button
            type="button"
            onClick={this.reload}
            className="ap-washable ap-register-chrome mt-3 rounded-lg border px-3 py-2"
            style={{ borderColor: "var(--hairline)", fontSize: TYPE.scale.xs, fontWeight: 600 }}
            data-testid="room-boundary-reload"
          >
            Reload this room
          </button>
        </section>
      );
    }
    // The nonce keys the subtree so a reload throws away the failed instance.
    return <BoundaryBody key={this.state.nonce}>{this.props.children}</BoundaryBody>;
  }
}

function BoundaryBody({ children }: { children: ReactNode }) {
  return <>{children}</>;
}
