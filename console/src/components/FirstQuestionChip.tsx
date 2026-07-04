"use client";

import { TYPE } from "@/lib/tokens";
import { askHrefFor, suggestedQuestionFor } from "@/lib/firstQuestion";

/**
 * FIRST-RUN SUGGESTED QUESTION (A2): Home shows ONE suggested first question
 * for the current identity, as a single chip above the fold. Clicking it
 * stages the question on Ask (never auto-submits — the person presses Ask).
 * The data + helpers live in @/lib/firstQuestion so the server-rendered
 * identity picker can share them without crossing the client boundary.
 */
export function FirstQuestionChip({ principal }: { principal: string | null }) {
  if (principal === null) {
    return null;
  }
  const question = suggestedQuestionFor(principal);
  return (
    <div className="mb-1 flex flex-wrap items-center gap-2" data-testid="first-question">
      <span className="ap-soft ap-register-chrome" style={{ fontSize: TYPE.scale.xs }}>
        Try asking
      </span>
      <a
        href={askHrefFor(principal)}
        className="ap-chip ap-washable ap-register-chrome inline-flex min-h-9 items-center rounded-full border px-3 py-1.5"
        style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
        data-testid="first-question-chip"
      >
        {question}
      </a>
    </div>
  );
}
