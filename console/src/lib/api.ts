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

// ---------------------------------------------------------------------------
// AP-2: GET /lens/{subject_id} — mirrored field-for-field from
// service/src/lens.rs. The actor is the header principal; cross-lens views
// are audited server-side BEFORE the response renders.
// ---------------------------------------------------------------------------

export interface LensSubject {
  band?: number;
  department?: string;
  groups: string[];
  id: string;
  kind: "human" | "agent";
  name: string;
  owner_user_id?: string;
  sites: string[];
}

export interface LensDoc {
  also_via: string[];
  document_id: string;
  effective_successor?: string;
  sensitivity: Sensitivity;
  superseded?: boolean;
  title: string;
}

export interface LensSection {
  docs: LensDoc[];
  reason: string;
  sentence: string;
}

export interface LensAgent {
  agent_id: string;
  grant_groups: string[];
  name: string;
}

export interface LensResponse {
  actor_id: string;
  agents: LensAgent[];
  cross_lens: boolean;
  holdings: LensSection[];
  snapshot_version: string;
  subject: LensSubject;
}

/** GET /lens/{subject}. 404 (unknown/malformed, byte-identical) -> null. */
export async function getLens(
  actor: string,
  subjectId: string,
): Promise<LensResponse | null> {
  const response = await fetch(`${SERVICE_URL}/lens/${encodeURIComponent(subjectId)}`, {
    headers: headers(actor),
  });
  if (response.status === 404) {
    return null;
  }
  return parse<LensResponse>(response);
}

// ---------------------------------------------------------------------------
// AP-3: GET /atlas — mirrored field-for-field from service/src/atlas.rs.
// STRUCTURE IS INTERNAL-GRADE; EVIDENCE IS GOVERNED: the types carry id,
// name, nesting, and the viewer's OWN docs, and nothing else. They are
// structurally incapable of expressing totals, hidden counts, or coverage —
// a capability with no visible evidence is an empty docs array, full stop.
// ---------------------------------------------------------------------------

export interface AtlasDoc {
  document_id: string;
  effective_successor?: string;
  sensitivity: Sensitivity;
  superseded?: boolean;
  title: string;
}

export interface AtlasCapability {
  /** VIEWER-SCOPED: empty when none of the mapped docs are the viewer's. */
  docs: AtlasDoc[];
  id: string;
  name: string;
}

export interface AtlasWorkflow {
  capabilities: AtlasCapability[];
  id: string;
  name: string;
}

export interface AtlasInitiative {
  id: string;
  name: string;
  workflows: AtlasWorkflow[];
}

export interface AtlasStrategy {
  id: string;
  initiatives: AtlasInitiative[];
  name: string;
}

export interface AtlasResponse {
  actor_id: string;
  snapshot_version: string;
  /** `[]` = the actor has no standing (the empty atlas, their own produce). */
  strategies: AtlasStrategy[];
}

/** GET /atlas. 404 (this world has no BRM) -> null. */
export async function getAtlas(actor: string): Promise<AtlasResponse | null> {
  const response = await fetch(`${SERVICE_URL}/atlas`, {
    headers: headers(actor),
  });
  if (response.status === 404) {
    return null;
  }
  return parse<AtlasResponse>(response);
}

// ---------------------------------------------------------------------------
// AP-4: GET /lens/diff — mirrored field-for-field from service/src/diff.rs.
// SET EXACTNESS lives at the type layer too: three columns and nothing
// else. No counts, no coverage, no summary fields exist to render.
// ---------------------------------------------------------------------------

export interface DiffPassport {
  id: string;
  kind: "human" | "agent";
  name: string;
}

export interface DiffDocRow {
  document_id: string;
  effective_successor?: string;
  sensitivity: Sensitivity;
  superseded?: boolean;
  title: string;
}

export interface DiffSection {
  docs: DiffDocRow[];
  reason: string;
  sentence: string;
}

export interface DiffSharedRow {
  divergent_route: boolean;
  doc: DiffDocRow;
  /** Verbatim from each side's artifact. */
  left_reasons: string[];
  right_reasons: string[];
}

export interface DiffResponse {
  actor_id: string;
  left: DiffPassport;
  left_only: DiffSection[];
  right: DiffPassport;
  right_only: DiffSection[];
  shared: DiffSharedRow[];
  snapshot_version: string;
}

/** GET /lens/diff. 404 (unknown side, byte-identical) -> null. */
export async function getLensDiff(
  actor: string,
  left: string,
  right: string,
): Promise<DiffResponse | null> {
  const response = await fetch(
    `${SERVICE_URL}/lens/diff?left=${encodeURIComponent(left)}&right=${encodeURIComponent(right)}`,
    { headers: headers(actor) },
  );
  if (response.status === 404) {
    return null;
  }
  return parse<DiffResponse>(response);
}
