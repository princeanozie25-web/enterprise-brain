"use client";

import { useEffect, useState } from "react";
import type { PersonCard } from "@/lib/api";
import { AVATAR_FALLBACK_TINT, COLOR, DEPARTMENT_TINT, FONT, TYPE } from "@/lib/tokens";

/**
 * AR-1 PERSON AVATAR — swap-ready by construction. If
 * console/public/faces/<id>.jpg exists it loads; otherwise the <img> errors
 * and we fall back to a DESIGNED monogram: the person's initials on a calm
 * disc tinted by department (from the reserved palette, colorblind-safe). Drop
 * a synthetic-face pack into public/faces/ later and every avatar upgrades
 * with ZERO code change. The monogram is intentional, not a placeholder.
 */
export function PersonAvatar({
  principalId,
  displayName,
  department,
  size = 36,
  tint: tintOverride,
}: {
  principalId: string;
  displayName?: string | null;
  department?: string | null;
  size?: number;
  /** B1: the Company/Operating Map passes its neutral-ramp tint here so the
   * graph never paints saturated (sensitivity-reserved) hues on departments.
   * Rooms that pass nothing keep the existing department tint behavior. */
  tint?: { background: string; border: string };
}) {
  const [failed, setFailed] = useState(false);
  // A new principal id is a new face to try — reset the fallback.
  useEffect(() => setFailed(false), [principalId]);

  const label = displayName ?? principalId;
  const tint = tintOverride ?? ((department && DEPARTMENT_TINT[department]) || AVATAR_FALLBACK_TINT);

  if (!failed) {
    return (
      // eslint-disable-next-line @next/next/no-img-element
      <img
        src={`/faces/${principalId}.jpg`}
        alt=""
        aria-hidden="true"
        width={size}
        height={size}
        onError={() => setFailed(true)}
        className="shrink-0 rounded-full object-cover"
        style={{ width: size, height: size }}
        data-testid="person-avatar-img"
      />
    );
  }

  return (
    <span
      aria-hidden="true"
      title={label}
      data-testid="person-avatar-monogram"
      data-department={department ?? ""}
      className="ap-register-chrome inline-flex shrink-0 select-none items-center justify-center rounded-full"
      style={{
        width: size,
        height: size,
        backgroundColor: tint.background,
        border: `1px solid ${tint.border}`,
        color: COLOR.ink,
        fontFamily: FONT.chrome,
        fontSize: Math.round(size * 0.4),
        fontWeight: 600,
        lineHeight: 1,
      }}
    >
      {initialsOf(label)}
    </span>
  );
}

/** First + last initial; a single name yields its first two letters. */
export function initialsOf(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) {
    return "?";
  }
  if (parts.length === 1) {
    return parts[0].slice(0, 2).toUpperCase();
  }
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

/**
 * AR-1 ROOM ACTOR — the compact "who you are viewing as" identity header for
 * the Atlas and Lane rooms (and anywhere a single principal heads a surface).
 * Renders nothing when no humanization card is available (the room falls back
 * to its existing chrome). The id stays in the evidence register — it is
 * still evidence, shown small beside the name.
 */
export function RoomActor({ card }: { card: PersonCard | null | undefined }) {
  if (!card) {
    return null;
  }
  return (
    <div
      className="ap-card mb-3 flex items-center gap-2 rounded-lg px-3 py-1.5"
      data-testid="room-actor"
    >
      <PersonAvatar
        principalId={card.id}
        displayName={card.display_name}
        department={card.department_label}
        size={28}
      />
      <span className="min-w-0">
        <span
          className="ap-register-chrome block truncate"
          style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
          data-testid="room-actor-name"
        >
          {card.display_name}
        </span>
        <span
          className="ap-soft block truncate"
          style={{ fontSize: TYPE.scale.xs }}
          data-testid="room-actor-title"
        >
          {card.title}
        </span>
      </span>
      <span
        className="ap-register-evidence ap-soft ml-auto shrink-0"
        style={{ fontSize: TYPE.scale.xs }}
        data-testid="room-actor-id"
      >
        {card.id}
      </span>
    </div>
  );
}
