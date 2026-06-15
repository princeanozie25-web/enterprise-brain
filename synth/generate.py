"""Entry point: python -m synth.generate --seed 42 --out fixtures/

Builds the whole synthetic company deterministically from one seeded RNG,
resolves the FULL (principal x document) matrix through the oracle, plants
and verifies every trap, enforces the allow-rate ceilings and the denylist,
and writes canonical JSON (sorted keys, LF newlines, trailing newline).

Forbidden here by spec: datetime.now(), network, unseeded randomness.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import random
from pathlib import Path

from synth import banks, constants, humanized_names
from synth.model import (
    AclRule,
    AgentGrant,
    AgentPrincipal,
    BrmNode,
    CompanyModel,
    Document,
    Group,
    Person,
)
from synth.oracle import materialize, stats
from synth.traps import (
    GenerationError,
    MosaicSpec,
    SopParamPlan,
    collect_traps,
    plan_mosaic_specs,
    plan_sop_parameters,
)

EPOCH = dt.date(*constants.FIXED_EPOCH_DATE)

# Department staffing plan (sums to 119; p_void makes 120).
# HR is deliberately small: grp_hr (the HR department group) is what the
# special_category gate trusts, and the restricted+special allow-rate ceiling
# (< 5%) bounds how many privileged HR readers the corpus can carry:
# 30 records x (4 HR + 1 subject) + 12 board minutes x 6 = 222 of 5,208 pairs.
DEPT_SIZES = {
    "Quality & Compliance": 16,
    "Warehouse Operations": 28,
    "Pharmacy Services": 15,
    "Finance": 14,
    "IT": 13,
    "HR": 4,
    "Sales & Accounts": 22,
    "Executive": 7,
}

# BRM naming material (structure labels, not prose; all fictional).
BRM_THEMES = [
    "Resilient Cold Chain",
    "Digital Traceability",
    "Regulatory Excellence",
    "Customer Service Reliability",
    "Workforce Capability",
    "Network Efficiency",
]
BRM_INITIATIVE_VERBS = ["Strengthen", "Automate", "Standardise", "Audit", "Expand", "Consolidate"]
BRM_WORKFLOW_NOUNS = [
    "Goods-In Verification", "Temperature Excursion Response", "Batch Release Review",
    "Customer Credit Review", "Returns Quarantine", "Licence Renewal", "Stock Reconciliation",
    "Delivery Scheduling", "Supplier Qualification", "Deviation Handling",
]
BRM_CAPABILITY_NOUNS = [
    "Cold Storage Monitoring", "Document Control", "Pick Accuracy", "Recall Execution",
    "Account Onboarding", "Payroll Processing", "Access Review", "Incident Triage",
    "Fleet Telemetry", "Shelf-Life Tracking", "Controlled Drugs Handling", "Quality Sign-Off",
]


def iso_day(days_before_epoch: int) -> str:
    return (EPOCH - dt.timedelta(days=days_before_epoch)).isoformat()


def iso_ts(rng: random.Random, days_before_epoch: int) -> str:
    hour = rng.randint(8, 17)
    minute = rng.choice([0, 15, 30, 45])
    day = EPOCH - dt.timedelta(days=days_before_epoch)
    return f"{day.isoformat()}T{hour:02d}:{minute:02d}:00Z"


# --------------------------------------------------------------------------
# People, groups, agents
# --------------------------------------------------------------------------

def build_people(rng: random.Random) -> list[Person]:
    people: list[Person] = []
    used_names: set[str] = set()
    counter = 1

    def next_name() -> str:
        while True:
            name = f"{rng.choice(banks.FIRST_NAMES)} {rng.choice(banks.LAST_NAMES)}"
            if name not in used_names:
                used_names.add(name)
                return name

    ceo_id: str | None = None
    head_ids: dict[str, str] = {}

    for dept in constants.DEPARTMENTS:
        size = DEPT_SIZES[dept]
        roles = banks.ROLE_BANK[dept]
        for i in range(size):
            pid = f"p{counter:03d}"
            counter += 1
            is_head = i == 0
            if dept == "Warehouse Operations":
                site = constants.SITES[i % 2]  # warehouse staff split both sites
            else:
                site = constants.SITES[0] if rng.random() < 0.8 else constants.SITES[1]
            person = Person(
                id=pid,
                name=next_name(),
                department=dept,
                role=roles[0] if is_head else roles[1 + rng.randrange(len(roles) - 1)],
                manager_id=None,  # wired below
                employment_band=5 if is_head else rng.randint(1, 4),
                site=site,
                start_date=iso_day(rng.randint(60, 4000)),
            )
            people.append(person)
            if is_head:
                head_ids[dept] = pid
                if dept == "Executive":
                    ceo_id = pid

    assert ceo_id is not None
    for person in people:
        if person.id == ceo_id:
            person.manager_id = None
        elif person.id == head_ids[person.department]:
            person.manager_id = ceo_id
        else:
            person.manager_id = head_ids[person.department]

    # The deny-by-default probe (P-4): present in the org, member of nothing.
    people.append(
        Person(
            id=constants.VOID_PRINCIPAL_ID,
            name=next_name(),
            department="IT",
            role="Access Review Shadow",
            manager_id=head_ids["IT"],
            employment_band=1,
            site=constants.SITES[0],
            start_date=iso_day(45),
        )
    )

    # AR-1b: humanized names baked in AT SOURCE. The legacy next_name() draws
    # above are deliberately PRESERVED so the one seeded RNG stream — and thus
    # every governance fact (bands, sites, groups, ACLs, the whole document
    # set) — stays byte-identical; this pure post-pass consumes NO rng and only
    # swaps the display strings. Every name is two tokens (first + surname),
    # exactly as the legacy pool, so the word-count-driven body templating in
    # banks.render_body is unperturbed too. The result is identical to the
    # service's humanize.rs assignment (test/test_humanized_names.py proves it),
    # so people.json and the corpus agree on every name.
    name_map = humanized_names.assign([p.id for p in people])
    for p in people:
        p.name = name_map[p.id]
    return people


def build_groups(rng: random.Random, people: list[Person]) -> list[Group]:
    by_dept: dict[str, list[Person]] = {}
    for p in people:
        if p.id != constants.VOID_PRINCIPAL_ID:
            by_dept.setdefault(p.department, []).append(p)

    groups: list[Group] = []
    for dept in constants.DEPARTMENTS:
        members = [p.id for p in by_dept[dept]]
        groups.append(
            Group(
                id=constants.DEPT_GROUP_IDS[dept],
                name=f"{dept} department",
                description=f"All staff of the {dept} department.",
                member_ids=members,
            )
        )

    def pick(dept: str, n: int, min_band: int = 1, exclude: set[str] | None = None) -> list[str]:
        pool = sorted(
            p.id
            for p in by_dept[dept]
            if p.employment_band >= min_band and p.id not in (exclude or set())
        )
        return pool[:n]

    quality_head = by_dept["Quality & Compliance"][0].id
    hr_head = by_dept["HR"][0].id
    finance_head = by_dept["Finance"][0].id
    ceo = by_dept["Executive"][0].id

    qa_release = pick("Quality & Compliance", 8, min_band=3)
    board = sorted({ceo, quality_head, finance_head, hr_head, by_dept["Executive"][1].id, by_dept["Executive"][2].id})
    payroll_admins = sorted({finance_head, pick("Finance", 2, min_band=3, exclude={finance_head})[0], hr_head})
    incident = sorted(pick("IT", 4, min_band=2) + pick("Quality & Compliance", 1, min_band=2) + pick("Warehouse Operations", 1, min_band=2))
    gdp_rp = sorted(pick("Quality & Compliance", 2, min_band=4) + pick("Pharmacy Services", 2, min_band=4))
    contractors = sorted(
        [p.id for p in by_dept["IT"] if p.employment_band <= 2][:3]
        + [p.id for p in by_dept["Warehouse Operations"] if p.employment_band <= 2][:2]
    )

    cross = [
        ("grp_qa_release", "QA batch release", "Authorised batch-release signatories.", qa_release),
        ("grp_board", "Board", "Board of directors.", board),
        ("grp_payroll_admins", "Payroll administrators", "Staff administering payroll.", payroll_admins),
        ("grp_incident_responders", "Incident responders", "On-call incident response rota.", incident),
        ("grp_gdp_responsible_persons", "GDP responsible persons", "Named responsible persons under GDP.", gdp_rp),
        ("grp_contractors", "Contractors", "Fixed-term contract staff.", contractors),
    ]
    for gid, name, desc, members in cross:
        groups.append(Group(id=gid, name=name, description=desc, member_ids=sorted(members)))
    return groups


def build_agents(people: list[Person], groups: list[Group]) -> list[AgentPrincipal]:
    by_dept: dict[str, list[Person]] = {}
    for p in people:
        if p.id != constants.VOID_PRINCIPAL_ID:
            by_dept.setdefault(p.department, []).append(p)
    in_group: dict[str, set[str]] = {g.id: set(g.member_ids) for g in groups}

    def first_where(dept: str, predicate) -> Person:
        for p in by_dept[dept]:
            if predicate(p):
                return p
        raise GenerationError(f"no agent owner candidate in {dept}")

    # Deliberately mis-scoped agents are the confused-deputy substrate:
    # the grant exceeds what the owner can see, and the oracle must clamp.
    qa_owner = first_where("Sales & Accounts", lambda p: p.employment_band == 3 and p.id not in in_group["grp_qa_release"])
    ops_owner = first_where("Warehouse Operations", lambda p: p.site == constants.SITES[0] and p.employment_band >= 3)
    fin_owner = first_where("Finance", lambda p: p.employment_band <= 2 and p.id not in in_group["grp_payroll_admins"])
    exec_owner = first_where("Executive", lambda p: p.id not in in_group["grp_board"])

    return [
        AgentPrincipal(
            id="agent_qa_drafter",
            name="QA drafting assistant",
            grant=AgentGrant(groups=["grp_qa_release"]),
            owner_user_id=qa_owner.id,
        ),
        AgentPrincipal(
            id="agent_ops_concierge",
            name="Warehouse operations concierge",
            grant=AgentGrant(groups=["grp_warehouse_operations"], site=constants.SITES[0]),
            owner_user_id=ops_owner.id,
        ),
        AgentPrincipal(
            id="agent_finance_analyst",
            name="Finance analysis assistant",
            grant=AgentGrant(groups=["grp_finance", "grp_payroll_admins"]),
            owner_user_id=fin_owner.id,
        ),
        AgentPrincipal(
            id="agent_exec_brief",
            name="Executive briefing assistant",
            grant=AgentGrant(groups=["grp_board"]),
            owner_user_id=exec_owner.id,
        ),
    ]


# --------------------------------------------------------------------------
# Documents
# --------------------------------------------------------------------------

class DocFactory:
    def __init__(self, rng: random.Random, people: list[Person]) -> None:
        self.rng = rng
        self.counter = 0
        self.by_dept: dict[str, list[Person]] = {}
        for p in people:
            if p.id != constants.VOID_PRINCIPAL_ID:
                self.by_dept.setdefault(p.department, []).append(p)

    def next_id(self) -> str:
        self.counter += 1
        return f"d{self.counter:04d}"

    def author(self, dept: str) -> str:
        return self.rng.choice(self.by_dept[dept]).id

    @staticmethod
    def rules(doc_id: str, specs: list[tuple]) -> list[AclRule]:
        out: list[AclRule] = []
        for spec in specs:
            kind = spec[0]
            if kind == "public":
                out.append(AclRule(rule_id=f"r:{doc_id}:public", kind="public"))
            elif kind == "group":
                out.append(AclRule(rule_id=f"r:{doc_id}:grp:{spec[1]}", kind="group", group=spec[1]))
            elif kind == "role":
                out.append(AclRule(rule_id=f"r:{doc_id}:role:{spec[1]}", kind="role", role=spec[1]))
            elif kind == "attr_site":
                out.append(AclRule(rule_id=f"r:{doc_id}:site:{spec[1]}", kind="attr_site", site=spec[1]))
            elif kind == "attr_band_min":
                out.append(AclRule(rule_id=f"r:{doc_id}:band:{spec[1]}", kind="attr_band_min", min_band=spec[1]))
        return out


def build_documents(
    rng: random.Random,
    people: list[Person],
    sop_plans: list[SopParamPlan],
    mosaic_specs: list[MosaicSpec],
) -> tuple[list[Document], list[tuple[str, str, str, bool]], list[tuple[str, str, str]]]:
    f = DocFactory(rng, people)
    docs: list[Document] = []
    sop_pairs: list[tuple[str, str, str, bool]] = []
    mosaic_planted: list[tuple[str, str, str]] = []

    # 1) SOP families: 25 x 2 versions, superseded version REMAINS in corpus.
    sop_depts = ["Quality & Compliance", "Warehouse Operations", "Pharmacy Services"]
    for plan in sop_plans:
        dept = sop_depts[plan.family_index % len(sop_depts)]
        code = f"SOP-{dept.split()[0][:3].upper()}-{plan.family_index + 1:03d}"
        title_slots = {"procedure_code": code, "topic": banks.PRODUCT_TERMS[plan.family_index % len(banks.PRODUCT_TERMS)]}
        title = banks.render_title(rng, "sop", title_slots)
        author = f.author(dept)
        v1_id = f.next_id()
        v2_id = f.next_id()
        common = dict(source="docstore", title=title, author_id=author, department=dept, doc_type="sop")
        docs.append(
            Document(
                id=v1_id,
                body=banks.render_body(rng, "sop", {**title_slots, "parameter_text": plan.v1_text}),
                created_at=iso_ts(rng, 700 + plan.family_index * 7),
                sensitivity="internal",
                acl_refs=f.rules(v1_id, [("group", constants.DEPT_GROUP_IDS[dept])]),
                version=1,
                supersedes=None,
                **common,
            )
        )
        docs.append(
            Document(
                id=v2_id,
                body=banks.render_body(rng, "sop", {**title_slots, "parameter_text": plan.v2_text}),
                created_at=iso_ts(rng, 90 + plan.family_index * 3),
                sensitivity="internal",
                acl_refs=f.rules(v2_id, [("group", constants.DEPT_GROUP_IDS[dept])]),
                version=2,
                supersedes=v1_id,
                **common,
            )
        )
        sop_pairs.append((v2_id, v1_id, plan.parameter_class, plan.changed))

    # 2) Quality/batch records: quality_system, confidential, qa groups only.
    for i in range(constants.N_QUALITY_RECORDS):
        did = f.next_id()
        slots = {
            "batch_code": f"BR-26{i:04d}",
            "product": banks.PRODUCT_TERMS[(i * 3) % len(banks.PRODUCT_TERMS)],
            "disposition": rng.choices(["released", "quarantined", "rejected"], weights=[8, 2, 1])[0],
        }
        docs.append(
            Document(
                id=did,
                source="quality_system",
                title=banks.render_title(rng, "quality_record", slots),
                body=banks.render_body(rng, "quality_record", slots),
                author_id=f.author("Quality & Compliance"),
                department="Quality & Compliance",
                created_at=iso_ts(rng, rng.randint(5, 400)),
                sensitivity="confidential",
                acl_refs=f.rules(did, [("group", "grp_qa_release"), ("group", "grp_quality_compliance")]),
                version=1,
                supersedes=None,
                doc_type="quality_record",
            )
        )

    # 3) HR records: special_category, hr group + subject only (never manager).
    # Subjects are non-HR staff with a manager; grp_hr is drawn exclusively
    # from the HR department, so no subject's manager can sit in grp_hr —
    # the manager-overreach traps depend on that structural guarantee.
    hr_dept_head = f.by_dept["HR"][0]
    eligible = sorted(
        (
            p
            for p in people
            if p.department != "HR"
            and p.id != constants.VOID_PRINCIPAL_ID
            and p.manager_id is not None
        ),
        key=lambda p: p.id,
    )
    subjects = rng.sample(eligible, constants.N_HR_RECORDS)
    kinds = ["salary_review", "grievance", "absence_summary"]
    for i, subject in enumerate(sorted(subjects, key=lambda p: p.id)):
        did = f.next_id()
        slots = {
            "subject_name": subject.name,
            "salary_band_text": f"employment band {subject.employment_band} of 5",
            "record_kind": kinds[i % 3],
        }
        docs.append(
            Document(
                id=did,
                source="hr_system",
                title=banks.render_title(rng, "hr_record", slots),
                body=banks.render_body(rng, "hr_record", slots),
                author_id=hr_dept_head.id,
                department="HR",
                created_at=iso_ts(rng, rng.randint(10, 700)),
                sensitivity="special_category",
                acl_refs=f.rules(did, [("group", "grp_hr")]),
                version=1,
                supersedes=None,
                doc_type="hr_record",
                subject_id=subject.id,
            )
        )

    # 4) Board minutes: restricted, board only.
    ceo = f.by_dept["Executive"][0]
    for i in range(constants.N_BOARD_MINUTES):
        did = f.next_id()
        slots = {
            "quarter_label": f"FY26 cycle {i + 1:02d}",
            "agenda_items": [
                "trading performance review",
                "regulatory inspection readiness",
                "capital allocation",
                "principal risk register",
            ][: 2 + (i % 3)],
        }
        docs.append(
            Document(
                id=did,
                source="docstore",
                title=banks.render_title(rng, "board_minutes", slots),
                body=banks.render_body(rng, "board_minutes", slots),
                author_id=ceo.id,
                department="Executive",
                created_at=iso_ts(rng, 30 * (i + 1)),
                sensitivity="restricted",
                acl_refs=f.rules(did, [("group", "grp_board")]),
                version=1,
                supersedes=None,
                doc_type="board_minutes",
            )
        )

    # 5) Customer/account docs: Sales & Accounts + Finance overlap; a slice
    #    carries site constraints (the cross-site trap pool).
    customers = rng.sample(banks.CUSTOMER_NAMES, 40)
    for i in range(constants.N_CUSTOMER_DOCS):
        did = f.next_id()
        slots = {"customer_name": customers[i % len(customers)], "account_code": f"AC-{i + 1:04d}"}
        specs: list[tuple] = [("group", "grp_sales_accounts"), ("group", "grp_finance")]
        if i % 5 == 0:  # 12 docs: site-tied logistics annexes
            specs.append(("attr_site", constants.SITES[(i // 5) % 2]))
        docs.append(
            Document(
                id=did,
                source="docstore" if i % 2 == 0 else "mail_lite",
                title=banks.render_title(rng, "customer_account", slots),
                body=banks.render_body(rng, "customer_account", slots),
                author_id=f.author("Sales & Accounts"),
                department="Sales & Accounts",
                created_at=iso_ts(rng, rng.randint(3, 500)),
                sensitivity="confidential" if i % 3 == 0 else "internal",
                acl_refs=f.rules(did, specs),
                version=1,
                supersedes=None,
                doc_type="customer_account",
            )
        )

    # 6) Mosaic pairs: doc_a broad/innocuous, doc_b narrow aggregate; both
    #    individually authorized for some principal (verified in traps.py).
    for spec in mosaic_specs:
        a_id = f.next_id()
        b_id = f.next_id()
        if spec.fact_class == "individual_salary":
            a_specs: list[tuple] = [("group", "grp_finance")]
            b_specs: list[tuple] = [("group", "grp_payroll_admins")]
            a_dept, b_dept = "Finance", "Finance"
        elif spec.fact_class == "site_stock_value":
            a_specs = [("group", "grp_warehouse_operations"), ("group", "grp_finance")]
            b_specs = [("group", "grp_finance")]
            a_dept, b_dept = "Warehouse Operations", "Finance"
        else:  # customer_margin
            a_specs = [("group", "grp_sales_accounts"), ("group", "grp_finance")]
            b_specs = [("group", "grp_finance")]
            a_dept, b_dept = "Sales & Accounts", "Finance"
        docs.append(
            Document(
                id=a_id,
                source="docstore",
                title=banks.render_title(rng, "general", {"topic": "operating summary"}),
                body=banks.render_body(rng, "general", {"topic": "operating summary", "stat_lines": [spec.stat_a]}),
                author_id=f.author(a_dept),
                department=a_dept,
                created_at=iso_ts(rng, rng.randint(20, 300)),
                sensitivity="internal",
                acl_refs=f.rules(a_id, a_specs),
                version=1,
                supersedes=None,
                doc_type="general",
            )
        )
        docs.append(
            Document(
                id=b_id,
                source="docstore",
                title=banks.render_title(rng, "general", {"topic": "aggregate financial position"}),
                body=banks.render_body(rng, "general", {"topic": "aggregate financial position", "stat_lines": [spec.stat_b]}),
                author_id=f.author(b_dept),
                department=b_dept,
                created_at=iso_ts(rng, rng.randint(20, 300)),
                sensitivity="confidential",
                acl_refs=f.rules(b_id, b_specs),
                version=1,
                supersedes=None,
                doc_type="general",
            )
        )
        mosaic_planted.append((a_id, b_id, spec.fact_class))

    # 7) Remainder: wiki (public + internal), mail threads, general docstore.
    n_wiki_public, n_wiki_internal, n_mail, n_general = 60, 80, 160, 88
    dept_cycle = [d for d in constants.DEPARTMENTS if d != "Executive"]

    for i in range(n_wiki_public):
        did = f.next_id()
        dept = dept_cycle[i % len(dept_cycle)]
        docs.append(
            Document(
                id=did,
                source="wiki",
                title=banks.render_title(rng, "wiki_page", {}),
                body=banks.render_body(rng, "wiki_page", {}),
                author_id=f.author(dept),
                department=dept,
                created_at=iso_ts(rng, rng.randint(5, 600)),
                sensitivity="public",
                acl_refs=f.rules(did, [("public",)]),
                version=1,
                supersedes=None,
                doc_type="wiki_page",
            )
        )
    for i in range(n_wiki_internal):
        did = f.next_id()
        dept = dept_cycle[i % len(dept_cycle)]
        docs.append(
            Document(
                id=did,
                source="wiki",
                title=banks.render_title(rng, "wiki_page", {}),
                body=banks.render_body(rng, "wiki_page", {}),
                author_id=f.author(dept),
                department=dept,
                created_at=iso_ts(rng, rng.randint(5, 600)),
                sensitivity="internal",
                acl_refs=f.rules(did, [("group", constants.DEPT_GROUP_IDS[dept])]),
                version=1,
                supersedes=None,
                doc_type="wiki_page",
            )
        )
    for i in range(n_mail):
        did = f.next_id()
        dept = dept_cycle[(i * 3) % len(dept_cycle)]
        docs.append(
            Document(
                id=did,
                source="mail_lite",
                title=banks.render_title(rng, "mail_thread", {}),
                body=banks.render_body(rng, "mail_thread", {}),
                author_id=f.author(dept),
                department=dept,
                created_at=iso_ts(rng, rng.randint(1, 365)),
                sensitivity="internal",
                acl_refs=f.rules(did, [("group", constants.DEPT_GROUP_IDS[dept])]),
                version=1,
                supersedes=None,
                doc_type="mail_thread",
            )
        )
    for i in range(n_general):
        did = f.next_id()
        dept = dept_cycle[(i * 5) % len(dept_cycle)]
        confidential = i % 4 == 0
        specs = [("group", constants.DEPT_GROUP_IDS[dept])]
        if confidential:
            specs.append(("attr_band_min", 3))
        docs.append(
            Document(
                id=did,
                source="docstore",
                title=banks.render_title(rng, "general", {"topic": "departmental briefing"}),
                body=banks.render_body(rng, "general", {"topic": "departmental briefing"}),
                author_id=f.author(dept),
                department=dept,
                created_at=iso_ts(rng, rng.randint(5, 500)),
                sensitivity="confidential" if confidential else "internal",
                acl_refs=f.rules(did, specs),
                version=1,
                supersedes=None,
                doc_type="general",
            )
        )

    if len(docs) != constants.N_DOCUMENTS:
        raise GenerationError(f"document count {len(docs)} != {constants.N_DOCUMENTS}")
    return docs, sop_pairs, mosaic_planted


# --------------------------------------------------------------------------
# BRM graph: 6 strategies -> 18 initiatives -> 40 workflows -> 90 capabilities
# --------------------------------------------------------------------------

def build_brm(rng: random.Random, docs: list[Document]) -> tuple[list[BrmNode], list[BrmNode], list[BrmNode], list[BrmNode]]:
    strategies = [
        BrmNode(id=f"strat{i + 1:02d}", name=f"Strategy: {BRM_THEMES[i]}", parent_id=None)
        for i in range(6)
    ]
    initiatives: list[BrmNode] = []
    for i in range(18):
        strat = strategies[i // 3]
        node = BrmNode(
            id=f"init{i + 1:02d}",
            name=f"{BRM_INITIATIVE_VERBS[i % 6]} {BRM_THEMES[i // 3]}",
            parent_id=strat.id,
        )
        strat.child_ids.append(node.id)
        initiatives.append(node)
    workflows: list[BrmNode] = []
    for i in range(40):
        init = initiatives[i % 18]
        node = BrmNode(
            id=f"wf{i + 1:02d}",
            name=f"Workflow: {BRM_WORKFLOW_NOUNS[i % len(BRM_WORKFLOW_NOUNS)]} {i + 1:02d}",
            parent_id=init.id,
        )
        init.child_ids.append(node.id)
        workflows.append(node)
    capabilities: list[BrmNode] = []
    for i in range(90):
        wf = workflows[i % 40]
        node = BrmNode(
            id=f"cap{i + 1:02d}",
            name=f"Capability: {BRM_CAPABILITY_NOUNS[i % len(BRM_CAPABILITY_NOUNS)]} {i + 1:02d}",
            parent_id=wf.id,
        )
        wf.child_ids.append(node.id)
        capabilities.append(node)

    # Every SOP maps to >= 1 capability (family -> capability by index);
    # sprinkle other operational docs deterministically for a non-trivial graph.
    sops = [d for d in docs if d.doc_type == "sop"]
    for idx, doc in enumerate(sops):
        capabilities[idx % 50].document_ids.append(doc.id)
    others = [d for d in docs if d.doc_type in ("quality_record", "customer_account", "general")]
    for idx, doc in enumerate(others):
        capabilities[(idx * 7) % 90].document_ids.append(doc.id)
    for cap in capabilities:
        cap.document_ids = sorted(set(cap.document_ids))
    return strategies, initiatives, workflows, capabilities


# --------------------------------------------------------------------------
# Writers (canonical JSON: sorted keys, LF, trailing newline)
# --------------------------------------------------------------------------

def write_json(path: Path, obj: object) -> None:
    text = json.dumps(obj, sort_keys=True, indent=2, ensure_ascii=False) + "\n"
    path.write_text(text, encoding="utf-8", newline="\n")


def write_jsonl(path: Path, rows: list[dict]) -> None:
    lines = [json.dumps(r, sort_keys=True, ensure_ascii=False, separators=(",", ":")) for r in rows]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")


def denylist_scan(payloads: dict[str, str]) -> None:
    for filename, text in payloads.items():
        lowered = text.lower()
        for term in constants.DENYLIST:
            if term.lower() in lowered:
                raise GenerationError(f"denylist hit: {term!r} found in {filename}")


# --------------------------------------------------------------------------
# Main
# --------------------------------------------------------------------------

def generate(seed: int, out_dir: Path) -> dict:
    rng = random.Random(seed)

    people = build_people(rng)
    groups = build_groups(rng, people)
    agents = build_agents(people, groups)
    sop_plans = plan_sop_parameters(rng)
    mosaic_specs = plan_mosaic_specs(rng)
    docs, sop_pairs, mosaic_planted = build_documents(rng, people, sop_plans, mosaic_specs)
    strategies, initiatives, workflows, capabilities = build_brm(rng, docs)

    model = CompanyModel(
        people=people,
        groups=groups,
        agents=agents,
        documents=docs,
        strategies=strategies,
        initiatives=initiatives,
        workflows=workflows,
        capabilities=capabilities,
    )

    traps = collect_traps(model, sop_pairs, mosaic_planted)
    rows = list(materialize(model))
    stat = stats(rows, model)

    if stat["allow_rate"] >= constants.ALLOW_RATE_CEILING:
        raise GenerationError(
            f"allow rate {stat['allow_rate']:.4f} >= ceiling {constants.ALLOW_RATE_CEILING} — "
            "a mostly-open corpus can't catch leaks"
        )
    if stat["restricted_special_allow_rate"] >= constants.RESTRICTED_SPECIAL_ALLOW_CEILING:
        raise GenerationError(
            f"restricted+special allow rate {stat['restricted_special_allow_rate']:.4f} >= "
            f"{constants.RESTRICTED_SPECIAL_ALLOW_CEILING}"
        )

    company = {
        "company": {
            "name": constants.COMPANY_NAME,
            "fictional": True,
            "regulatory_context": "UK Good Distribution Practice (synthetic)",
        },
        "sites": [{"id": s, "name": banks.SITE_DISPLAY[s]} for s in constants.SITES],
        "departments": constants.DEPARTMENTS,
        "people": [p.to_dict() for p in people],
        "groups": [g.to_dict() for g in groups],
        "agents": [a.to_dict() for a in agents],
        "sources": constants.SOURCES,
    }
    brm = {
        "strategies": [
            {"id": s.id, "name": s.name, "initiative_ids": s.child_ids} for s in strategies
        ],
        "initiatives": [
            {"id": n.id, "name": n.name, "strategy_id": n.parent_id, "workflow_ids": n.child_ids}
            for n in initiatives
        ],
        "workflows": [
            {"id": n.id, "name": n.name, "initiative_id": n.parent_id, "capability_ids": n.child_ids}
            for n in workflows
        ],
        "capabilities": [
            {"id": n.id, "name": n.name, "workflow_id": n.parent_id, "document_ids": n.document_ids}
            for n in capabilities
        ],
    }
    documents = {"documents": [d.to_dict() for d in docs]}

    payloads = {
        "company.json": json.dumps(company, sort_keys=True, ensure_ascii=False),
        "documents.json": json.dumps(documents, sort_keys=True, ensure_ascii=False),
        "brm.json": json.dumps(brm, sort_keys=True, ensure_ascii=False),
        "traps.json": json.dumps(traps, sort_keys=True, ensure_ascii=False),
        "oracle_stats.json": json.dumps(stat, sort_keys=True, ensure_ascii=False),
        "ground_truth.jsonl": "\n".join(json.dumps(r, sort_keys=True, ensure_ascii=False) for r in rows),
    }
    denylist_scan(payloads)

    out_dir.mkdir(parents=True, exist_ok=True)
    write_json(out_dir / "company.json", company)
    write_json(out_dir / "documents.json", documents)
    write_json(out_dir / "brm.json", brm)
    write_json(out_dir / "traps.json", traps)
    write_json(out_dir / "oracle_stats.json", stat)
    write_jsonl(out_dir / "ground_truth.jsonl", rows)
    return stat


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Generate the M0 synthetic company fixtures.")
    parser.add_argument("--seed", type=int, default=constants.SEED_DEFAULT)
    parser.add_argument("--out", type=Path, default=Path("fixtures"))
    args = parser.parse_args(argv)

    stat = generate(args.seed, args.out)
    print(
        f"generated {stat['total_pairs']} ground-truth pairs -> {args.out} "
        f"(allow rate {stat['allow_rate']:.4f}, restricted+special {stat['restricted_special_allow_rate']:.4f})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
