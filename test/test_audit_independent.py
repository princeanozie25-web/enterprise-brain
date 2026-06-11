"""Adversarial audit: N-version cross-check of the oracle.

Re-derives the ENTIRE ground-truth matrix from the fixture JSON alone, using a
second evaluator written fresh from the spec text — it imports nothing from
synth.acl / synth.oracle, so a shared bug in the oracle cannot hide here.
Also sweeps the generation-path DO-NOTs (no wall clocks, no network, synthetic
flags, no cross-domain body leaks).
"""

import re
from pathlib import Path

SYNTH_DIR = Path(__file__).resolve().parent.parent / "synth"

HR_GROUP = "grp_hr"


def _expected_allowed_people(doc: dict, people: list[dict], members: dict[str, set[str]]) -> set[str]:
    """Spec rules, restated independently: OR'd grants, AND'd constraints,
    special_category needs hr membership, subject always reads own record."""
    allowed: set[str] = set()
    for p in people:
        pid = p["id"]
        if doc["subject_id"] is not None and doc["subject_id"] == pid:
            allowed.add(pid)
            continue
        grant = False
        constraints_ok = True
        for rule in doc["acl_refs"]:
            kind = rule["kind"]
            if kind == "public":
                grant = True
            elif kind == "group":
                if pid in members[rule["group"]]:
                    grant = True
            elif kind == "role":
                if p["role"] == rule["role"]:
                    grant = True
            elif kind == "attr_site":
                if p["site"] != rule["site"]:
                    constraints_ok = False
            elif kind == "attr_band_min":
                if p["employment_band"] < rule["min_band"]:
                    constraints_ok = False
        if grant and constraints_ok:
            if doc["sensitivity"] == "special_category" and pid not in members[HR_GROUP]:
                continue
            allowed.add(pid)
    return allowed


def _agent_grant_side_allows(doc: dict, agent: dict) -> bool:
    grant_groups = set(agent["grant"]["groups"])
    site = agent["grant"].get("site")
    band = agent["grant"].get("employment_band")
    grant = False
    constraints_ok = True
    for rule in doc["acl_refs"]:
        kind = rule["kind"]
        if kind == "public":
            grant = True
        elif kind == "group":
            if rule["group"] in grant_groups:
                grant = True
        elif kind == "attr_site":
            if site is None or site != rule["site"]:
                constraints_ok = False
        elif kind == "attr_band_min":
            if band is None or band < rule["min_band"]:
                constraints_ok = False
        # role rules never match agents
    if not (grant and constraints_ok):
        return False
    if doc["sensitivity"] == "special_category" and HR_GROUP not in grant_groups:
        return False
    return True


def test_full_matrix_matches_independent_rederivation(company, documents, ground_truth) -> None:
    people = company["people"]
    agents = company["agents"]
    members = {g["id"]: set(g["member_ids"]) for g in company["groups"]}

    truth_allows: dict[str, set[str]] = {}
    for row in ground_truth:
        if row["decision"] == "ALLOW":
            truth_allows.setdefault(row["resource_id"], set()).add(row["principal_id"])

    mismatches: list[str] = []
    for doc in documents:
        expected = _expected_allowed_people(doc, people, members)
        for agent in agents:
            if _agent_grant_side_allows(doc, agent) and agent["owner_user_id"] in expected:
                expected.add(agent["id"])
        actual = truth_allows.get(doc["id"], set())
        if expected != actual:
            mismatches.append(
                f"{doc['id']}: oracle-only={sorted(actual - expected)} "
                f"audit-only={sorted(expected - actual)}"
            )
    assert mismatches == [], (
        f"{len(mismatches)} documents disagree with independent re-derivation:\n"
        + "\n".join(mismatches[:10])
    )


def test_do_not_sweeps(company, documents) -> None:
    # Generation paths: no wall clocks, no randomness outside the seeded RNG,
    # no network machinery of any kind.
    forbidden = [
        r"datetime\.now", r"date\.today", r"\btime\.time\b", r"uuid\.",
        r"os\.urandom", r"\burllib\b", r"\brequests\b", r"\bsocket\b",
        r"\bhttp\.client\b", r"random\.seed\(",
    ]
    for path in sorted(SYNTH_DIR.glob("*.py")):
        text = path.read_text(encoding="utf-8")
        for pattern in forbidden:
            assert not re.search(pattern, text), f"{pattern} found in {path.name}"

    # Every principal record is flagged synthetic.
    for record in company["people"] + company["agents"]:
        assert record["synthetic"] is True, record["id"]

    # Cross-domain leak probe: salary-band phrasing exists ONLY inside
    # hr_system records (template inputs respected their sensitivity).
    for doc in documents:
        if doc["source"] != "hr_system":
            assert "employment band" not in doc["body"], doc["id"]
            assert doc["subject_id"] is None, doc["id"]
