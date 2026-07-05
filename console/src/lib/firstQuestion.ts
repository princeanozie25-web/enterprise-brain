// FIRST-RUN SUGGESTED QUESTIONS (A2) — pure data + helpers, framework-neutral
// so BOTH the server ProductHome (the identity picker) and the client
// FirstQuestionChip can import them without crossing the client boundary.
//
// These are PROMPTS, not claims: the engine answers only from what the chosen
// identity is allowed to see. p_void deliberately shares p060's question so
// the same words yield sourced documents for one identity and an honest,
// EMPTY refusal for the other — the product's whole point in twenty seconds.
//
// The phrasings are verified against the live corpus (this build runs
// keyword-only retrieval, broad/semantic search off): p060 (holds grp_board
// + grp_finance) sees the confidential Finance set; p_void (no standing) sees
// NOTHING for it, while p088 sees its HR (special-category) set. A natural,
// common-word question would pull public docs into p_void's result and blur
// the contrast, so the p060/p_void prompt is a deliberate keyword phrase.
export const SUGGESTED_FIRST_QUESTION: Record<string, string> = {
  p060: "confidential financial statements",
  p088: "What HR policies apply to my team?",
  p_void: "confidential financial statements",
};

const FALLBACK_QUESTION = "What did my department publish recently?";

export function suggestedQuestionFor(principal: string): string {
  return SUGGESTED_FIRST_QUESTION[principal] ?? FALLBACK_QUESTION;
}

export function askHrefFor(principal: string): string {
  const question = suggestedQuestionFor(principal);
  return `/ask?as=${encodeURIComponent(principal)}&q=${encodeURIComponent(question)}`;
}
