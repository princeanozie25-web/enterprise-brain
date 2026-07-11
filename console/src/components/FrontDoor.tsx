"use client";

import { useEffect, useState } from "react";
import { EntryScreen } from "./EntryScreen";
import { ProductHome } from "./ProductHome";

/**
 * SHOWREEL TRACK A — the two-phase front door at `/`.
 *
 * Cold arrival opens on the cinematic EntryScreen; "Enter the demo" fires the
 * ONE entry->picker transition (the iris budget class, dead under
 * prefers-reduced-motion) and reveals the reframed identity picker.
 *
 * Honest exceptions that skip the cold open and land directly on the picker:
 * - `?expired=1` — the K3 re-auth landing. An expired session is a return
 *   trip, not an arrival; the picker's own expiry line + return-intent
 *   restore logic (ProductHome) must render immediately.
 * No auth logic lives here: phases are presentation only, and the picker's
 * behavior (links, session, restore) is byte-identical to before.
 */
export function FrontDoor() {
  const [phase, setPhase] = useState<"entry" | "picker">("entry");

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (params.get("expired") === "1") setPhase("picker");
  }, []);

  if (phase === "entry") {
    return <EntryScreen onEnter={() => setPhase("picker")} />;
  }
  return (
    <div className="ap-entry-iris" data-testid="front-door-picker">
      <ProductHome />
    </div>
  );
}
