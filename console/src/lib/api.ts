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
import { authHeader, currentToken, notifyExpiry } from "./session";
import { serviceRequest, parseJson, isRequestError } from "./request";

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

/** K1: one ADMITTED claim — the sentence and the anchor that proves it
 * (locator = "doc_id@byte_offset" in the cited document's full body). */
export interface AnswerClaim {
  doc_id: string;
  locator: string;
  text: string;
}

/** K1 drop-with-disclosure: the model's OWN draft claims, admitted vs
 * removed by the verbatim-anchoring gate. Never about documents. */
export interface GroundingCounts {
  admitted: number;
  refused: number;
}

export interface Answer {
  citations: string[];
  /** K1, additive: admitted claims only. Absent on pre-grounding envelopes. */
  claims?: AnswerClaim[];
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

export interface GrantedContextNode {
  id: string;
  name: string;
}

export interface GrantedContextSummary {
  active: boolean;
  approver_id: string;
  capability: GrantedContextNode;
  grant_id: string;
  grant_scope: string;
  grant_status: "active";
  initiative: GrantedContextNode;
  request_id: string;
  strategy: GrantedContextNode;
  target_kind: "capability" | "project";
  workflow: GrantedContextNode;
}

export interface AnswerEnvelope {
  aggregation_bounded: boolean;
  answer?: Answer;
  demo_identity_mode: boolean;
  generation_applied: boolean;
  granted_context?: GrantedContextSummary;
  /** K1, additive: present whenever the grounding gate ran — including the
   * all-refused case where `answer` is omitted (the drop is disclosed). */
  grounding?: GroundingCounts;
  /** K1, additive: optional in the mirror so captured pre-grounding
   * envelopes stay valid (ignore-don't-drop). */
  grounding_applied?: boolean;
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

export type DerivedRoleLevel =
  | "employee"
  | "team_lead"
  | "department_head"
  | "executive_candidate"
  | "super_admin_candidate";

export interface RoleDepartmentScope {
  band?: number | null;
  department_id: string;
  seniority: string;
}

export interface RoleTeamScope {
  direct_report_count: number;
  has_team_scope: boolean;
}

export interface RoleProjectScope {
  capability_ids: string[];
  project_count: number;
}

export interface RoleApprovalScope {
  has_approval_scope: boolean;
  pending_count: number;
}

export interface RoleScopeSummary {
  actor_id: string;
  admin_surface_allowed: boolean;
  approval_scope: RoleApprovalScope;
  bursar_surface_allowed: boolean;
  confidence: string;
  demo_identity_mode: boolean;
  department_scope: RoleDepartmentScope;
  derived_level: DerivedRoleLevel;
  enforcement: "derived_only" | "server_enforced";
  governance_surface_allowed: boolean;
  project_scope: RoleProjectScope;
  reasons: string[];
  team_scope: RoleTeamScope;
}

// FC-A1: identity rides the session bearer (Authorization), never a header the
// caller asserts. The engine resolves the principal from the validated session.
function headers(): HeadersInit {
  return {
    "content-type": "application/json",
    ...authHeader(),
  };
}

// K3 Track 1/2: EVERY service call routes through this one wrapper. It binds
// the loopback base, and on { kind: 'unauthorized' } fires the expiry notifier
// ONCE before re-throwing — so a room's own catch still lands in its honest
// empty state AND the app edge navigates to the identity picker. Timeout /
// network / service / invalid_body all propagate as ServiceRequestError; the
// pre-K3 rooms already catch generically, so their behavior is unchanged
// except that an infinite spinner is now an aborted, stated failure.
async function request(path: string, init: RequestInit = {}): Promise<Response> {
  // Capture the token this call rides so a late 401 is attributed to the
  // session that issued it, not to whatever session happens to be active
  // when the failure lands (prevents a stale 401 from killing a fresh one).
  const issuedToken = currentToken() ?? undefined;
  try {
    return await serviceRequest(SERVICE_URL, path, init);
  } catch (cause) {
    if (isRequestError(cause, "unauthorized")) notifyExpiry(issuedToken);
    throw cause;
  }
}

export async function ask(
  principal: string,
  query: string,
  options: { capabilityId?: string; grantId?: string; hybrid: boolean; judge: boolean },
): Promise<AnswerEnvelope> {
  const body: {
    capability_id?: string;
    grant_id?: string;
    hybrid: boolean;
    judge: boolean;
    query: string;
  } = { query, hybrid: options.hybrid, judge: options.judge };
  if (options.grantId && options.capabilityId) {
    body.grant_id = options.grantId;
    body.capability_id = options.capabilityId;
  }
  const response = await request(`/ask`, {
    method: "POST",
    headers: headers(),
    body: JSON.stringify(body),
  });
  return parseJson<AnswerEnvelope>(response);
}

export async function getScope(principal: string): Promise<ScopeResponse> {
  const response = await request(`/scope`, {
    headers: headers(),
  });
  return parseJson<ScopeResponse>(response);
}

/** GET /me/scope. Read-only role posture, not an access grant. 404 -> null. */
export async function getRoleScope(principal: string): Promise<RoleScopeSummary | null> {
  const response = await request(`/me/scope`, {
    headers: headers(),
  });
  if (response.status === 404) {
    return null;
  }
  return parseJson<RoleScopeSummary>(response);
}

/**
 * GET /doc/{id}. Returns null on 404 — and the service guarantees that
 * out-of-scope and nonexistent are byte-identical 404s, so null is ALL the
 * console can ever know. The inspector renders one empty state for it (U-5).
 */
export async function getDoc(principal: string, docId: string): Promise<DocCard | null> {
  const response = await request(`/doc/${encodeURIComponent(docId)}`, {
    headers: headers(),
  });
  if (response.status === 404) {
    return null;
  }
  return parseJson<DocCard>(response);
}

// ---------------------------------------------------------------------------
// AR-1: the humanization layer — mirrored field-for-field from
// service/src/humanize.rs. DISPLAY ONLY: a card carries name + title +
// department + avatar (org-structural, the Atlas-BRM tier), NEVER a holding
// or document id. The masthead record adds bio / location / reporting lines /
// projects; projects are DERIVED from the same Lane rule, so a project's
// evidence is always inside the subject's own holdings.
// ---------------------------------------------------------------------------

export interface PersonCard {
  avatar_ref: string;
  department_label: string;
  display_name: string;
  id: string;
  title: string;
}

export interface ProjectRecord {
  capability_id: string;
  capability_name: string;
  initiative_name: string;
  role: string;
  status: string;
  strategy_name: string;
  workflow_name: string;
}

export interface HumanRecord {
  avatar_ref: string;
  bio: string;
  department_label: string;
  display_name: string;
  id: string;
  location: string;
  manages: string[];
  personality_tag: string;
  projects: ProjectRecord[];
  reports_to: string | null;
  seniority: string;
  title: string;
  work_style: string;
}

export interface PeopleResponse {
  demo_identity_mode: boolean;
  people: PersonCard[];
}

/**
 * GET /people — the org-structural directory (names + titles, NOT holdings).
 * Internal-grade, demo-open here; in production the roster is itself a
 * permissioned resource (the service comments the swap point). Returns `[]`
 * when this world has no humanization layer.
 */
export async function getPeople(actor: string): Promise<PersonCard[]> {
  const response = await request(`/people`, { headers: headers() });
  const data = await parseJson<PeopleResponse>(response);
  return data.people ?? [];
}

// ---------------------------------------------------------------------------
// AR-2: GET /graph — the Org Graph, mirrored field-for-field from
// service/src/graph.rs. INTERNAL-GRADE structure (consistent with /people),
// NO holding/count/document id can appear. Anchors are the Leadership tier.
// ---------------------------------------------------------------------------

export interface GraphCenter {
  id: string;
  label: string;
}

export interface GraphDept {
  id: string;
  label: string;
  tint_key: string;
}

export interface GraphPerson {
  avatar_ref: string;
  department_id: string;
  display_name: string;
  id: string;
  is_self: boolean;
  ring: "anchor" | "member";
  title: string;
}

export interface GraphTool {
  department_id?: string;
  id: string;
  kind: "tool" | "agent";
  label: string;
}

/** A real system of record (company.json sources): docstore, wiki, etc. */
export interface GraphSource {
  id: string;
  kind: "source";
  label: string;
}

/** A real project/capability grouped from HumanRecord.projects. */
export interface GraphProject {
  departments: string[];
  id: string;
  initiative_name: string;
  label: string;
  people: number;
  primary_department_id: string;
  status_counts: Record<string, number>;
  strategy_name: string;
  workflow_name: string;
}

export interface GraphEdge {
  from: string;
  kind: "reports_to" | "member_of" | "uses" | "owns_agent" | "system_of" | "works_on" | "involves_department";
  to: string;
}

export interface GraphResponse {
  actor_id: string;
  center: GraphCenter;
  departments: GraphDept[];
  edges: GraphEdge[];
  people: GraphPerson[];
  projects: GraphProject[];
  snapshot_version: string;
  sources: GraphSource[];
  tools: GraphTool[];
}

/** GET /graph. 404 (unknown actor / no humanization layer) -> null. */
export async function getGraph(actor: string): Promise<GraphResponse | null> {
  const response = await request(`/graph`, { headers: headers() });
  if (response.status === 404) {
    return null;
  }
  return parseJson<GraphResponse>(response);
}

// ---------------------------------------------------------------------------
// Access requests and read grants. Requests cannot target raw documents, and
// grants cannot represent write/admin rights or hidden document identities.
// ---------------------------------------------------------------------------

export type AccessRequestStatus = "pending" | "approved" | "denied" | "cancelled" | "expired";
export type AccessGrantStatus = "active" | "revoked" | "expired";
export type AccessGrantPermission = "read";

export type AccessTarget =
  | { kind: "capability"; capability_id: string }
  | { kind: "project"; capability_id: string };

export interface AccessDecision {
  actor_principal: string;
  decided_ordinal: number;
  outcome: "approved" | "denied";
  reason_code?: string;
}

export interface AccessRequestRecord {
  approver_id: string;
  created_ordinal: number;
  decision?: AccessDecision;
  justification: string;
  request_id: string;
  request_key: string;
  requester_id: string;
  snapshot_version: string;
  status: AccessRequestStatus;
  target: AccessTarget;
}

export interface AccessRequestsResponse {
  actor_id: string;
  demo_identity_mode: boolean;
  requests: AccessRequestRecord[];
  snapshot_version: string;
}

export interface AccessRequestMutationResponse {
  demo_identity_mode: boolean;
  request: AccessRequestRecord;
  snapshot_version: string;
}

export interface AccessGrantRecord {
  approver_id: string;
  created_ordinal: number;
  expires_at?: string;
  grant_id: string;
  grantee_id: string;
  permission: AccessGrantPermission;
  reason: string;
  request_id: string;
  revocation_reason?: string;
  revoked_by?: string;
  revoked_ordinal?: number;
  snapshot_version: string;
  status: AccessGrantStatus;
  target: AccessTarget;
}

export interface AccessGrantsResponse {
  actor_id: string;
  demo_identity_mode: boolean;
  grants: AccessGrantRecord[];
  snapshot_version: string;
}

export interface AccessGrantResponse {
  demo_identity_mode: boolean;
  grant: AccessGrantRecord;
  snapshot_version: string;
}

export async function getAccessRequests(actor: string): Promise<AccessRequestsResponse | null> {
  const response = await request(`/access-requests`, { headers: headers() });
  if (response.status === 404) {
    return null;
  }
  return parseJson<AccessRequestsResponse>(response);
}

export async function getAccessRequestInbox(actor: string): Promise<AccessRequestsResponse | null> {
  const response = await request(`/access-requests/inbox`, { headers: headers() });
  if (response.status === 404) {
    return null;
  }
  return parseJson<AccessRequestsResponse>(response);
}

export async function postAccessRequest(
  actor: string,
  target: AccessTarget,
  justification: string,
): Promise<AccessRequestMutationResponse> {
  const response = await request(`/access-requests`, {
    method: "POST",
    headers: headers(),
    body: JSON.stringify({ target, justification }),
  });
  return parseJson<AccessRequestMutationResponse>(response);
}

export async function postAccessRequestDecision(
  actor: string,
  requestId: string,
  decision: "approve" | "deny",
  reasonCode?: string,
): Promise<AccessRequestMutationResponse> {
  const response = await request(
    `/access-requests/${encodeURIComponent(requestId)}/${decision}`,
    {
      method: "POST",
      headers: headers(),
      body: reasonCode ? JSON.stringify({ reason_code: reasonCode }) : undefined,
    },
  );
  return parseJson<AccessRequestMutationResponse>(response);
}

export async function getAccessGrants(actor: string): Promise<AccessGrantsResponse | null> {
  const response = await request(`/access-grants`, { headers: headers() });
  if (response.status === 404) {
    return null;
  }
  return parseJson<AccessGrantsResponse>(response);
}

export async function getAccessGrant(
  actor: string,
  grantId: string,
): Promise<AccessGrantResponse | null> {
  const response = await request(`/access-grants/${encodeURIComponent(grantId)}`, {
    headers: headers(),
  });
  if (response.status === 404) {
    return null;
  }
  return parseJson<AccessGrantResponse>(response);
}

export async function postAccessGrantRevoke(
  actor: string,
  grantId: string,
  reasonCode?: string,
): Promise<AccessGrantResponse> {
  const response = await request(
    `/access-grants/${encodeURIComponent(grantId)}/revoke`,
    {
      method: "POST",
      headers: headers(),
      body: reasonCode ? JSON.stringify({ reason_code: reasonCode }) : undefined,
    },
  );
  return parseJson<AccessGrantResponse>(response);
}

// ---------------------------------------------------------------------------
// Workflow projection: read-only execution view for one real capability.
// Items come from lane boxes, accepted agent boxes, or access-request rows.
// ---------------------------------------------------------------------------

export interface WorkflowNode {
  id: string;
  name: string;
}

export interface WorkflowProvenance {
  capability: WorkflowNode;
  initiative: WorkflowNode;
  strategy: WorkflowNode;
  workflow: WorkflowNode;
}

export type WorkflowItemKind = "lane_box" | "access_request" | "accepted_agent_box";

export interface WorkflowItem {
  agent_id?: string;
  approver_id?: string;
  capability_id: string;
  dependencies: string[];
  item_id: string;
  kind: WorkflowItemKind;
  owner_id?: string;
  provenance: WorkflowProvenance;
  requester_id?: string;
  snapshot_version: string;
  status: string;
  title: string;
  // SHOWCASE-III: present ONLY on materialized-proposal items (the service
  // skips them when absent, so every pre-existing item is byte-identical).
  description?: string;
  anchors?: ProposalAnchorView[];
  sources_outside_view?: number;
  proposal_id?: string;
}

export interface ProjectWorkflowResponse {
  actor_id: string;
  capability_id: string;
  demo_identity_mode: boolean;
  items: WorkflowItem[];
  provenance: WorkflowProvenance;
  snapshot_version: string;
}

/** GET /workflow/project/{capability_id}. 404 -> null. */
export async function getProjectWorkflow(
  actor: string,
  capabilityId: string,
): Promise<ProjectWorkflowResponse | null> {
  const response = await request(
    `/workflow/project/${encodeURIComponent(capabilityId)}`,
    { headers: headers() },
  );
  if (response.status === 404) {
    return null;
  }
  return parseJson<ProjectWorkflowResponse>(response);
}

// ---------------------------------------------------------------------------
// SHOWCASE-III: grounded workflow proposals — the first mutation path. A model
// PROPOSES (grounded, proposer-scoped); only the approver's audited decision
// MATERIALIZES. Types mirror service/src/{lib,proposals}.rs field-for-field.
// ---------------------------------------------------------------------------

/** S4 anchor redaction: verbatim fields are ABSENT (not empty) when the
 * viewer's own scope lacks the document — only the withheld marker crosses. */
export interface ProposalAnchorView {
  visible: boolean;
  doc_id?: string;
  title?: string;
  quote?: string;
  locator?: string;
}

export interface ProposalBoxView {
  box_index: number;
  stage: string;
  title: string;
  description: string;
  anchors: ProposalAnchorView[];
  sources_total: number;
  sources_outside_view: number;
}

export interface ProposalGroundingCounts {
  admitted: number;
  refused: number;
}

export type WorkflowProposalStatus = "pending" | "approved" | "denied";

export interface WorkflowProposal {
  proposal_id: string;
  proposer_id: string;
  capability_id: string;
  approver_id: string;
  title: string;
  goal: string;
  /** S4 accountability line — the proposer owns the prose. */
  drafted_from: string;
  boxes: ProposalBoxView[];
  grounding: ProposalGroundingCounts;
  status: WorkflowProposalStatus | string;
  created_ordinal: number;
  decided_by?: string;
  materialized: boolean;
  snapshot_version: string;
}

export interface WorkflowProposalEnvelope {
  demo_identity_mode: boolean;
  proposal: WorkflowProposal;
  snapshot_version: string;
}

/** The honest empty: generation faulted or zero boxes could be grounded.
 * NOTHING was written server-side; `reason` is the disclosure line. */
export interface WorkflowProposalEmpty {
  demo_identity_mode: boolean;
  generated: false;
  reason: string;
  grounding?: ProposalGroundingCounts;
  snapshot_version: string;
}

export type WorkflowProposalCreateResult = WorkflowProposalEnvelope | WorkflowProposalEmpty;

/** Discriminate a create result: a drafted proposal carries `proposal`. */
export function proposalWasDrafted(
  result: WorkflowProposalCreateResult,
): result is WorkflowProposalEnvelope {
  return "proposal" in result;
}

export interface WorkflowProposalListResponse {
  actor_id: string;
  demo_identity_mode: boolean;
  role: "proposer" | "approver" | string;
  proposals: WorkflowProposal[];
  snapshot_version: string;
}

/** POST /workflow/proposals — generate a grounded draft (proposer-scoped).
 * Server budget is 20s (one attempt); request.ts gives this path the long
 * timeout class. Returns the envelope OR the honest empty; fault/zero-admitted
 * write nothing server-side. */
export async function postWorkflowProposal(
  actor: string,
  body: { capability_id: string; title: string; goal: string },
): Promise<WorkflowProposalCreateResult> {
  const response = await request(`/workflow/proposals`, {
    method: "POST",
    headers: headers(),
    body: JSON.stringify(body),
  });
  return parseJson<WorkflowProposalCreateResult>(response);
}

/** GET /workflow/proposals?role=proposer|approver — my proposals, or my
 * decision inbox. */
export async function getWorkflowProposals(
  actor: string,
  role: "proposer" | "approver",
): Promise<WorkflowProposalListResponse> {
  const response = await request(`/workflow/proposals?role=${encodeURIComponent(role)}`, {
    headers: headers(),
  });
  return parseJson<WorkflowProposalListResponse>(response);
}

/** GET /workflow/proposals/{id}. Existence-hiding: only the proposer or the
 * approver may see it — everyone else gets the same 404 as a missing id. */
export async function getWorkflowProposal(
  actor: string,
  proposalId: string,
): Promise<WorkflowProposalEnvelope | null> {
  const response = await request(`/workflow/proposals/${encodeURIComponent(proposalId)}`, {
    headers: headers(),
  });
  if (response.status === 404) {
    return null;
  }
  return parseJson<WorkflowProposalEnvelope>(response);
}

/** POST /workflow/proposals/{id}/{approve|deny} — ApprovalGated server-side
 * (only the resolved approver; audit flushed BEFORE any effect; approve is
 * the ONE materialize path). Mirrors postAccessRequestDecision. */
export async function postWorkflowProposalDecision(
  actor: string,
  proposalId: string,
  decision: "approve" | "deny",
): Promise<WorkflowProposalEnvelope> {
  const response = await request(
    `/workflow/proposals/${encodeURIComponent(proposalId)}/${decision}`,
    { method: "POST", headers: headers() },
  );
  return parseJson<WorkflowProposalEnvelope>(response);
}

// ---------------------------------------------------------------------------
// GET /node/{id}/summary — the org-graph inspector's REAL governance data.
// Org core -> corpus cardinalities; person/agent -> compiled scope + reason-
// grouped access COUNTS (never documents). Metadata only; 404 for non-nodes.
// ---------------------------------------------------------------------------

export interface NodeReasonGroup {
  granted: number;
  reason: string;
  sentence: string;
}

export interface NodeAgentRef {
  id: string;
  name: string;
}

export interface OrgStats {
  agents: number;
  capabilities: number;
  departments: number;
  document_total: number;
  groups: number;
  initiatives: number;
  people: number;
  permission_edges: number;
  principals: number;
  sites: number;
  sources: number;
  strategies: number;
  total_decisions: number;
  workflows: number;
}

export interface NodeSummary {
  access_by_reason?: NodeReasonGroup[];
  agents_owned?: NodeAgentRef[];
  band?: number;
  blocked_actions?: string[];
  corpus_documents?: number;
  demo_identity_mode: boolean;
  department?: string;
  grant_groups?: string[];
  groups?: string[];
  id: string;
  kind: "org" | "human" | "agent";
  manages?: number;
  name: string;
  owner_user_id?: string;
  permitted_actions?: string[];
  reports_to?: string;
  sites?: string[];
  stats?: OrgStats;
  title?: string;
  visible_documents?: number;
}

/** GET /node/{id}/summary. 404 (dept/source/unknown) -> null. */
export async function getNodeSummary(actor: string, id: string): Promise<NodeSummary | null> {
  const response = await request(`/node/${encodeURIComponent(id)}/summary`, {
    headers: headers(),
  });
  if (response.status === 404) {
    return null;
  }
  return parseJson<NodeSummary>(response);
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
  /** AR-1: the viewer's own directory card (absent with no humanization layer). */
  actor?: PersonCard;
  actor_id: string;
  /** Honesty contract: the service carries this on every response (demo mode). */
  demo_identity_mode?: boolean;
  agents: LensAgent[];
  cross_lens: boolean;
  holdings: LensSection[];
  snapshot_version: string;
  subject: LensSubject;
  /** AR-1: the subject's masthead — bio, location, reporting lines, projects. */
  subject_human?: HumanRecord;
}

/** GET /lens/{subject}. 404 (unknown/malformed, byte-identical) -> null. */
export async function getLens(
  actor: string,
  subjectId: string,
): Promise<LensResponse | null> {
  const response = await request(`/lens/${encodeURIComponent(subjectId)}`, {
    headers: headers(),
  });
  if (response.status === 404) {
    return null;
  }
  return parseJson<LensResponse>(response);
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
  /** AR-1: the viewer's own directory card (the BRM names no other principal). */
  actor?: PersonCard;
  actor_id: string;
  /** Honesty contract: the service carries this on every response (demo mode). */
  demo_identity_mode?: boolean;
  snapshot_version: string;
  /** `[]` = the actor has no standing (the empty atlas, their own produce). */
  strategies: AtlasStrategy[];
}

/** GET /atlas. 404 (this world has no BRM) -> null. */
export async function getAtlas(actor: string): Promise<AtlasResponse | null> {
  const response = await request(`/atlas`, {
    headers: headers(),
  });
  if (response.status === 404) {
    return null;
  }
  return parseJson<AtlasResponse>(response);
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
  /** AR-1: the viewer's own directory card. */
  actor?: PersonCard;
  actor_id: string;
  /** Honesty contract: the service carries this on every response (demo mode). */
  demo_identity_mode?: boolean;
  left: DiffPassport;
  /** AR-1: the left principal's directory card (name/title/department/avatar). */
  left_human?: PersonCard;
  left_only: DiffSection[];
  right: DiffPassport;
  /** AR-1: the right principal's directory card. */
  right_human?: PersonCard;
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
  const response = await request(
    `/lens/diff?left=${encodeURIComponent(left)}&right=${encodeURIComponent(right)}`,
    { headers: headers() },
  );
  if (response.status === 404) {
    return null;
  }
  return parseJson<DiffResponse>(response);
}

// ---------------------------------------------------------------------------
// AP-5: POST /export — THE SERVER DERIVES, NEVER RECEIVES. The request
// names a view in PARAMETERS ONLY; there is no field that could carry
// content, so the attestation can never bless client bytes. The response
// is the attested PDF.
// ---------------------------------------------------------------------------

export interface ExportRequest {
  view: "lens" | "diff" | "atlas_capability" | "ask";
  lens?: { subject_id: string };
  diff?: { left: string; right: string };
  atlas_capability?: { capability_id: string };
  ask?: { query: string; hybrid: boolean; judge: boolean };
}

export async function exportEvidence(actor: string, exportRequest: ExportRequest): Promise<Blob> {
  const response = await request(`/export`, {
    method: "POST",
    headers: headers(),
    body: JSON.stringify(exportRequest),
  });
  if (!response.ok) {
    throw new Error(`service error ${response.status}`);
  }
  return await response.blob();
}

/** aperture-<view>-<subject-or-pair-or-cap-or-queryhash8>-<snapshot8>.pdf */
export function exportFilename(
  view: ExportRequest["view"],
  slug: string,
  snapshotVersion: string,
): string {
  return `aperture-${view}-${slug}-${snapshotVersion.slice(0, 8)}.pdf`;
}

// ---------------------------------------------------------------------------
// AP-6: the Lane — mirrored field-for-field from service/src/lane.rs.
// v4a, DISPLAY ONLY: effect_class carries the full vocabulary so the v4b
// door stays visible, but the service can never construct the amber class
// (AW-3) and the console renders none (U-28).
// ---------------------------------------------------------------------------

export interface ProvenanceNode {
  id: string;
  name: string;
}

export interface LaneBox {
  blocked_by: string[];
  blocks: string[];
  box_id: string;
  capability: ProvenanceNode;
  derived: boolean;
  deviation?: { kind: string };
  effect_class: "read_only" | "side_effecting";
  evidence: DiffDocRow[];
  honesty: ScopeStatement;
  provenance: {
    initiative: ProvenanceNode;
    strategy: ProvenanceNode;
    workflow: ProvenanceNode;
  };
  snapshot_version: string;
  sop_state: "current" | "blocked_superseded";
  status: "candidate" | "active" | "done" | "dismissed" | "blocked";
  why: string;
}

export interface LaneResponse {
  /** AR-1: the worker's own directory card (the lane is self-only). */
  actor?: PersonCard;
  actor_id: string;
  boxes: LaneBox[];
  snapshot_version: string;
}

export interface InboxPreview {
  agent_id: string;
  citations: string[];
  proposal_id: string;
  standing_query: string;
}

export interface InboxResponse {
  actor_id: string;
  proposals: InboxPreview[];
  snapshot_version: string;
}

export interface RollupRow {
  capability_id: string;
  status_counts: Record<string, number>;
}

export interface RollupResponse {
  capabilities: RollupRow[];
  honesty: string;
  snapshot_version: string;
}

/** GET /lane — SELF-ONLY: the actor header is the only input. */
export async function getLane(actor: string): Promise<LaneResponse | null> {
  const response = await request(`/lane`, { headers: headers() });
  if (response.status === 404) {
    return null;
  }
  return parseJson<LaneResponse>(response);
}

export async function postBoxStatus(
  actor: string,
  boxId: string,
  to: "active" | "done" | "dismissed",
): Promise<void> {
  const response = await request(
    `/lane/box/${encodeURIComponent(boxId)}/status`,
    { method: "POST", headers: headers(), body: JSON.stringify({ to }) },
  );
  if (!response.ok) {
    throw new Error(`service error ${response.status}`);
  }
}

export async function getInbox(actor: string): Promise<InboxResponse | null> {
  const response = await request(`/lane/inbox`, { headers: headers() });
  if (response.status === 404) {
    return null;
  }
  return parseJson<InboxResponse>(response);
}

export async function postInboxDecision(
  actor: string,
  proposalId: string,
  decision: "accept" | "dismiss",
): Promise<void> {
  const response = await request(
    `/lane/inbox/${encodeURIComponent(proposalId)}/${decision}`,
    { method: "POST", headers: headers() },
  );
  if (!response.ok) {
    throw new Error(`service error ${response.status}`);
  }
}

export async function getRollup(actor: string): Promise<RollupResponse | null> {
  const response = await request(`/lane/rollup`, { headers: headers() });
  if (response.status === 404) {
    return null;
  }
  return parseJson<RollupResponse>(response);
}
