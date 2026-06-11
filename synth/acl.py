"""The ACL model — deliberately boring and explicit (Pass B module 1).

Semantics, in full:

  ReBAC grants (OR'd):   public        matches every principal
                         group         person is a member / agent holds it in grant
                         role          person.role equals the rule's role (never agents)
  ABAC constraints (AND): attr_site     principal's site must equal the rule's site
                         attr_band_min principal's employment_band >= rule's min_band
  Overlays:              deny-by-default       no matching grant => DENY
                         subject-access        a person ALWAYS reads their own HR record
                         special_category gate requires hr membership or subject identity,
                                               on top of a matching grant
  Agents:                evaluated against their OWN grant + attributes only here;
                         the owner intersection lives in oracle.py, never implicitly.

Every decision carries >= 1 reason id. Vocabulary:
  allow: the matching grant rule_ids, plus passed constraint rule_ids,
         R_SUBJECT_SELF, R_SPECIAL_HR (hr membership satisfied the gate)
  deny:  D_DEFAULT, D_SITE:<rule_id>, D_BAND:<rule_id>, D_SPECIAL_CATEGORY
"""

from __future__ import annotations

from dataclasses import dataclass

from synth.model import AgentPrincipal, CompanyModel, Document, Person

HR_GROUP_ID = "grp_hr"

ALLOW = "ALLOW"
DENY = "DENY"

R_SUBJECT_SELF = "R_SUBJECT_SELF"
R_SPECIAL_HR = "R_SPECIAL_HR"
D_DEFAULT = "D_DEFAULT"
D_SPECIAL_CATEGORY = "D_SPECIAL_CATEGORY"


@dataclass
class Decision:
    decision: str  # ALLOW | DENY
    reasons: list[str]

    @property
    def allowed(self) -> bool:
        return self.decision == ALLOW


def _dedupe(reasons: list[str]) -> list[str]:
    seen: set[str] = set()
    out: list[str] = []
    for r in reasons:
        if r not in seen:
            seen.add(r)
            out.append(r)
    return out


def evaluate_person(model: CompanyModel, person: Person, doc: Document) -> Decision:
    """Full rule evaluation for a human principal. Pure; no caching."""
    # Subject-access rule is absolute: a person can always read their own
    # HR record, regardless of groups, constraints, or sensitivity gates.
    if doc.subject_id is not None and doc.subject_id == person.id:
        return Decision(ALLOW, [R_SUBJECT_SELF])

    member_of = model.groups_of(person.id)

    matched_grants: list[str] = []
    failed_constraints: list[str] = []
    passed_constraints: list[str] = []

    for rule in doc.acl_refs:
        if rule.kind == "public":
            matched_grants.append(rule.rule_id)
        elif rule.kind == "group":
            if rule.group in member_of:
                matched_grants.append(rule.rule_id)
        elif rule.kind == "role":
            if rule.role == person.role:
                matched_grants.append(rule.rule_id)
        elif rule.kind == "attr_site":
            if person.site == rule.site:
                passed_constraints.append(rule.rule_id)
            else:
                failed_constraints.append(f"D_SITE:{rule.rule_id}")
        elif rule.kind == "attr_band_min":
            if rule.min_band is not None and person.employment_band >= rule.min_band:
                passed_constraints.append(rule.rule_id)
            else:
                failed_constraints.append(f"D_BAND:{rule.rule_id}")
        else:  # unknown rule kinds fail closed: treated as a failed constraint
            failed_constraints.append(f"D_UNKNOWN_RULE:{rule.rule_id}")

    if not matched_grants:
        return Decision(DENY, [D_DEFAULT])
    if failed_constraints:
        return Decision(DENY, _dedupe(failed_constraints))

    # special_category overlay: a matching grant is NOT enough; the reader
    # must hold explicit hr membership (subject identity handled above).
    if doc.sensitivity == "special_category":
        if HR_GROUP_ID not in member_of:
            return Decision(DENY, [D_SPECIAL_CATEGORY])
        return Decision(ALLOW, _dedupe(matched_grants + passed_constraints + [R_SPECIAL_HR]))

    return Decision(ALLOW, _dedupe(matched_grants + passed_constraints))


def evaluate_agent_grant(model: CompanyModel, agent: AgentPrincipal, doc: Document) -> Decision:
    """The agent's OWN side of the intersection: its grant set and attributes,
    nothing inherited. Missing attributes fail constraints (fail closed)."""
    grant = agent.grant
    granted_groups = set(grant.groups)

    matched_grants: list[str] = []
    failed_constraints: list[str] = []
    passed_constraints: list[str] = []

    for rule in doc.acl_refs:
        if rule.kind == "public":
            matched_grants.append(rule.rule_id)
        elif rule.kind == "group":
            if rule.group in granted_groups:
                matched_grants.append(rule.rule_id)
        elif rule.kind == "role":
            pass  # agents hold no roles; role rules never match an agent grant
        elif rule.kind == "attr_site":
            if grant.site is not None and grant.site == rule.site:
                passed_constraints.append(rule.rule_id)
            else:
                failed_constraints.append(f"D_SITE:{rule.rule_id}")
        elif rule.kind == "attr_band_min":
            if grant.employment_band is not None and rule.min_band is not None and grant.employment_band >= rule.min_band:
                passed_constraints.append(rule.rule_id)
            else:
                failed_constraints.append(f"D_BAND:{rule.rule_id}")
        else:
            failed_constraints.append(f"D_UNKNOWN_RULE:{rule.rule_id}")

    if not matched_grants:
        return Decision(DENY, [D_DEFAULT])
    if failed_constraints:
        return Decision(DENY, _dedupe(failed_constraints))

    # An agent is never the subject of an HR record; only explicit hr
    # membership in its grant can pass the special_category gate.
    if doc.sensitivity == "special_category":
        if HR_GROUP_ID not in granted_groups:
            return Decision(DENY, [D_SPECIAL_CATEGORY])
        return Decision(ALLOW, _dedupe(matched_grants + passed_constraints + [R_SPECIAL_HR]))

    return Decision(ALLOW, _dedupe(matched_grants + passed_constraints))
