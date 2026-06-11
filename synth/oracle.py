"""The ground-truth oracle (Pass B module 2).

resolve(model, principal_id, resource_id) -> Decision is a PURE function over
the generated CompanyModel: no caching, no randomness, no I/O, and no
dependency on anything that will later be the system under test. It is
computed from first principles by direct rule evaluation (synth/acl.py).

Agent rule, computed explicitly per (agent, resource):
    effective = evaluate_agent_grant(agent)  INTERSECT  evaluate_person(owner)
Agents never inherit owner scope implicitly; an ALLOW requires BOTH sides.
"""

from __future__ import annotations

from collections.abc import Iterator

from synth.acl import ALLOW, DENY, Decision, evaluate_agent_grant, evaluate_person, _dedupe
from synth.model import CompanyModel

R_AGENT_INTERSECT = "R_AGENT_INTERSECT"
D_AGENT_GRANT = "D_AGENT_GRANT"
D_AGENT_OWNER = "D_AGENT_OWNER"


def resolve(model: CompanyModel, principal_id: str, resource_id: str) -> Decision:
    """ALLOW/DENY + reason rule ids for one (principal, resource) pair."""
    doc = model.document(resource_id)
    if doc is None:
        raise KeyError(f"unknown resource: {resource_id}")

    person = model.person(principal_id)
    if person is not None:
        return evaluate_person(model, person, doc)

    agent = model.agent(principal_id)
    if agent is not None:
        owner = model.person(agent.owner_user_id)
        if owner is None:  # a dangling owner can never widen access: fail closed
            return Decision(DENY, [D_AGENT_OWNER, "D_OWNER_MISSING"])
        grant_side = evaluate_agent_grant(model, agent, doc)
        owner_side = evaluate_person(model, owner, doc)
        if grant_side.allowed and owner_side.allowed:
            return Decision(
                ALLOW,
                _dedupe([R_AGENT_INTERSECT] + grant_side.reasons + owner_side.reasons),
            )
        reasons: list[str] = []
        if not grant_side.allowed:
            reasons += [D_AGENT_GRANT] + grant_side.reasons
        if not owner_side.allowed:
            reasons += [D_AGENT_OWNER] + owner_side.reasons
        return Decision(DENY, _dedupe(reasons))

    raise KeyError(f"unknown principal: {principal_id}")


def materialize(model: CompanyModel) -> Iterator[dict]:
    """The FULL matrix, every (principal, document) pair, in canonical order
    (sorted principal id, then sorted document id) for byte-stable output."""
    principal_ids = sorted(model.principal_ids())
    doc_ids = sorted(d.id for d in model.documents)
    for pid in principal_ids:
        for did in doc_ids:
            decision = resolve(model, pid, did)
            yield {
                "principal_id": pid,
                "resource_id": did,
                "decision": decision.decision,
                "reasons": decision.reasons,
            }


def stats(rows: list[dict], model: CompanyModel) -> dict:
    """Allow-rate accounting for fixtures/oracle_stats.json (gates live in
    generate.py: overall < 35%, restricted+special_category < 5%)."""
    sensitivity_of = {d.id: d.sensitivity for d in model.documents}
    by_sens: dict[str, dict[str, int]] = {}
    allow_total = 0
    for row in rows:
        sens = sensitivity_of[row["resource_id"]]
        bucket = by_sens.setdefault(sens, {"pairs": 0, "allows": 0})
        bucket["pairs"] += 1
        if row["decision"] == ALLOW:
            bucket["allows"] += 1
            allow_total += 1

    total_pairs = len(rows)
    rs_pairs = sum(by_sens.get(s, {}).get("pairs", 0) for s in ("restricted", "special_category"))
    rs_allows = sum(by_sens.get(s, {}).get("allows", 0) for s in ("restricted", "special_category"))
    return {
        "total_pairs": total_pairs,
        "allow_total": allow_total,
        "allow_rate": round(allow_total / total_pairs, 6) if total_pairs else 0.0,
        "by_sensitivity": {
            sens: {
                "pairs": b["pairs"],
                "allows": b["allows"],
                "allow_rate": round(b["allows"] / b["pairs"], 6) if b["pairs"] else 0.0,
            }
            for sens, b in sorted(by_sens.items())
        },
        "restricted_special_allow_rate": round(rs_allows / rs_pairs, 6) if rs_pairs else 0.0,
    }
