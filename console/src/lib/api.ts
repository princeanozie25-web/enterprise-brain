// THE CONTRACT.
//
// These types are mirrored EXACTLY, field for field, from the service:
//   AnswerEnvelope / Answer / EnrichedResult / ScopeStatement
//     <- service/src/answer.rs (canonical JSON, sorted keys)
//   DocCard <- service/src/answer.rs (GET /doc/{id})
//
// No extra fields. No convenience counts. The types CANNOT represent a count
// of suppressed, hidden, or filtered documents — that is the no-dark-counts
// rule held at the type layer (U-3 proves it at compile time).

import { SERVICE_URL } from "./constants";

export type Sensitivity =
  | "public"
  | "internal"
  | "confidential"
  | "restricted"
  | "special_category";

export type RetrievalMode = "lexical_only" | "hybrid";

export interface ScopeStatement {
  band: number | null;
  groups: string[];
  sites: string[];
}

export interface Answer {
  citations: string[];
  text: string;
}

export interface EnrichedResult {
  document_id: string;
  effective_successor?: string;
  reasons_ref: string[];
  score_rank: number;
  sensitivity: Sensitivity;
  superseded?: boolean;
  title: string;
}

export interface AnswerEnvelope {
  aggregation_bounded: boolean;
  answer?: Answer;
  demo_identity_mode: boolean;
  generation_applied: boolean;
  index_version: string;
  judge_applied: boolean;
  principal_id: string;
  query_hash: string;
  results: EnrichedResult[];
  retrieval_mode: RetrievalMode;
  scope_statement: ScopeStatement;
  snapshot_version: string;
}

export interface DocCard {
  document_id: string;
  effective_successor?: string;
  sensitivity: Sensitivity;
  snippet: string;
  superseded?: boolean;
  title: string;
}

export interface ScopeResponse {
  demo_identity_mode: boolean;
  principal_id: string;
  scope_statement: ScopeStatement;
}

/** 401: the request carried no principal. */
export class Unauthenticated extends Error {}

function headers(principal: string): HeadersInit {
  return {
    "content-type": "application/json",
    "x-demo-principal": principal,
  };
}

async function parse<T>(response: Response): Promise<T> {
  if (response.status === 401) {
    throw new Unauthenticated("missing principal");
  }
  if (!response.ok) {
    throw new Error(`service error ${response.status}`);
  }
  return (await response.json()) as T;
}

export async function ask(
  principal: string,
  query: string,
  options: { hybrid: boolean; judge: boolean },
): Promise<AnswerEnvelope> {
  const response = await fetch(`${SERVICE_URL}/ask`, {
    method: "POST",
    headers: headers(principal),
    body: JSON.stringify({ query, hybrid: options.hybrid, judge: options.judge }),
  });
  return parse<AnswerEnvelope>(response);
}

export async function getScope(principal: string): Promise<ScopeResponse> {
  const response = await fetch(`${SERVICE_URL}/scope`, {
    headers: headers(principal),
  });
  return parse<ScopeResponse>(response);
}

/**
 * GET /doc/{id}. Returns null on 404 — and the service guarantees that
 * out-of-scope and nonexistent are byte-identical 404s, so null is ALL the
 * console can ever know. The inspector renders one empty state for it (U-5).
 */
export async function getDoc(principal: string, docId: string): Promise<DocCard | null> {
  const response = await fetch(`${SERVICE_URL}/doc/${encodeURIComponent(docId)}`, {
    headers: headers(principal),
  });
  if (response.status === 404) {
    return null;
  }
  return parse<DocCard>(response);
}
