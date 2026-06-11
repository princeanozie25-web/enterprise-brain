"""Dataclasses for every M0 entity. These shapes ARE the fixture contract:
to_dict() output is what lands in fixtures/ (canonical JSON, sorted keys).

Nothing here may carry real PII; every principal record carries synthetic=True.
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class Person:
    id: str
    name: str
    department: str
    role: str
    manager_id: str | None
    employment_band: int  # 1-5
    site: str
    start_date: str  # YYYY-MM-DD, derived from FIXED_EPOCH only
    synthetic: bool = True

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name,
            "department": self.department,
            "role": self.role,
            "manager_id": self.manager_id,
            "employment_band": self.employment_band,
            "site": self.site,
            "start_date": self.start_date,
            "synthetic": self.synthetic,
        }


@dataclass
class Group:
    id: str
    name: str
    description: str
    member_ids: list[str] = field(default_factory=list)

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name,
            "description": self.description,
            "member_ids": sorted(self.member_ids),
        }


@dataclass
class AgentGrant:
    groups: list[str] = field(default_factory=list)
    # Attribute set the agent itself carries for ABAC checks. Absent keys are
    # absent attributes: fail-closed against any attribute constraint.
    site: str | None = None
    employment_band: int | None = None

    def to_dict(self) -> dict:
        out: dict = {"groups": sorted(self.groups)}
        if self.site is not None:
            out["site"] = self.site
        if self.employment_band is not None:
            out["employment_band"] = self.employment_band
        return out


@dataclass
class AgentPrincipal:
    """First-class principal. NEVER inherits owner scope implicitly: the
    oracle computes grant INTERSECT owner access explicitly per resource."""

    id: str
    name: str
    grant: AgentGrant
    owner_user_id: str
    synthetic: bool = True

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name,
            "grant": self.grant.to_dict(),
            "owner_user_id": self.owner_user_id,
            "synthetic": self.synthetic,
        }


@dataclass
class AclRule:
    """One explicit rule attached to a document.

    kinds:
      public         — matches every principal (used only on sensitivity=public)
      group          — principal is a member (person) / holds it in grant (agent)
      role           — person.role equals `role` (never matches agents)
      attr_site      — CONSTRAINT: principal's site attribute must equal `site`
      attr_band_min  — CONSTRAINT: principal's employment_band must be >= min_band

    grant kinds (public/group/role) are OR'd; constraint kinds (attr_*) must
    ALL pass. No matching grant or any failing constraint = DENY (deny-by-default).
    """

    rule_id: str
    kind: str
    group: str | None = None
    role: str | None = None
    site: str | None = None
    min_band: int | None = None

    def to_dict(self) -> dict:
        out: dict = {"rule_id": self.rule_id, "kind": self.kind}
        if self.group is not None:
            out["group"] = self.group
        if self.role is not None:
            out["role"] = self.role
        if self.site is not None:
            out["site"] = self.site
        if self.min_band is not None:
            out["min_band"] = self.min_band
        return out


@dataclass
class Document:
    id: str
    source: str
    title: str
    body: str
    author_id: str
    department: str
    created_at: str  # ISO 8601 Z, derived from FIXED_EPOCH only
    sensitivity: str  # public|internal|confidential|restricted|special_category
    acl_refs: list[AclRule]
    version: int
    supersedes: str | None
    doc_type: str  # sop|quality_record|hr_record|board_minutes|customer_account|wiki_page|mail_thread|general
    subject_id: str | None = None  # hr_record only: whose record this is

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "source": self.source,
            "title": self.title,
            "body": self.body,
            "author_id": self.author_id,
            "department": self.department,
            "created_at": self.created_at,
            "sensitivity": self.sensitivity,
            "acl_refs": [r.to_dict() for r in self.acl_refs],
            "version": self.version,
            "supersedes": self.supersedes,
            "doc_type": self.doc_type,
            "subject_id": self.subject_id,
        }


@dataclass
class BrmNode:
    id: str
    name: str
    parent_id: str | None
    child_ids: list[str] = field(default_factory=list)
    document_ids: list[str] = field(default_factory=list)  # capabilities only


@dataclass
class CompanyModel:
    """Everything the oracle resolves over. Built once per generation run."""

    people: list[Person]
    groups: list[Group]
    agents: list[AgentPrincipal]
    documents: list[Document]
    strategies: list[BrmNode]
    initiatives: list[BrmNode]
    workflows: list[BrmNode]
    capabilities: list[BrmNode]

    def person(self, person_id: str) -> Person | None:
        return self._people_by_id.get(person_id)

    def agent(self, agent_id: str) -> AgentPrincipal | None:
        return self._agents_by_id.get(agent_id)

    def document(self, doc_id: str) -> Document | None:
        return self._docs_by_id.get(doc_id)

    def groups_of(self, person_id: str) -> set[str]:
        return self._groups_by_member.get(person_id, set())

    def principal_ids(self) -> list[str]:
        return [p.id for p in self.people] + [a.id for a in self.agents]

    def __post_init__(self) -> None:
        self._people_by_id = {p.id: p for p in self.people}
        self._agents_by_id = {a.id: a for a in self.agents}
        self._docs_by_id = {d.id: d for d in self.documents}
        self._groups_by_member: dict[str, set[str]] = {}
        for g in self.groups:
            for m in g.member_ids:
                self._groups_by_member.setdefault(m, set()).add(g.id)
