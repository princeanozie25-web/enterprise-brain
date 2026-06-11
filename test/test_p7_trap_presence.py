"""P-7: trap counts meet minimums and every referenced id exists."""

from synth import constants


def test_trap_counts_and_referential_integrity(company, documents, traps) -> None:
    people_ids = {p["id"] for p in company["people"]}
    agent_ids = {a["id"] for a in company["agents"]}
    doc_ids = {d["id"] for d in documents}
    docs = {d["id"]: d for d in documents}

    assert len(traps["effective_version"]) >= constants.MIN_EFFECTIVE_VERSION_TRAPS
    assert len(traps["mosaic"]) >= constants.MIN_MOSAIC_PAIRS
    assert len(traps["confused_deputy"]) >= constants.MIN_CONFUSED_DEPUTY
    assert len(traps["manager_overreach"]) >= constants.MIN_MANAGER_OVERREACH
    assert len(traps["cross_site"]) >= constants.MIN_CROSS_SITE

    for t in traps["effective_version"]:
        assert t["current_id"] in doc_ids and t["superseded_id"] in doc_ids
        current, superseded = docs[t["current_id"]], docs[t["superseded_id"]]
        # The trap's substance: the old version is genuinely superseded by the
        # new one, stays in the corpus, and the parameter text differs.
        assert current["supersedes"] == t["superseded_id"]
        assert current["version"] > superseded["version"]
        assert current["body"] != superseded["body"]

    for t in traps["mosaic"]:
        assert t["doc_a"] in doc_ids and t["doc_b"] in doc_ids
        assert t["principal_id"] in people_ids | agent_ids
        assert t["inferred_fact_class"]

    for t in traps["confused_deputy"]:
        assert t["agent_id"] in agent_ids
        assert t["owner_id"] in people_ids
        assert t["resource_id"] in doc_ids

    for t in traps["manager_overreach"]:
        assert t["manager_id"] in people_ids
        assert t["subject_id"] in people_ids
        assert t["resource_id"] in doc_ids
        assert docs[t["resource_id"]]["subject_id"] == t["subject_id"]

    for t in traps["cross_site"]:
        assert t["principal_id"] in people_ids
        assert t["resource_id"] in doc_ids
        assert t["required_site"] != t["principal_site"]
