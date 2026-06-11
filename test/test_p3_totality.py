"""P-3: ground_truth.jsonl is the FULL matrix — exactly principals x documents
rows, every decision carrying at least one reason rule id."""


def test_matrix_is_total_with_reasons(company, documents, ground_truth) -> None:
    principal_ids = {p["id"] for p in company["people"]} | {a["id"] for a in company["agents"]}
    doc_ids = {d["id"] for d in documents}
    assert len(principal_ids) == 124
    assert len(doc_ids) == 600

    assert len(ground_truth) == len(principal_ids) * len(doc_ids)

    seen_pairs = set()
    for row in ground_truth:
        assert row["decision"] in ("ALLOW", "DENY")
        assert isinstance(row["reasons"], list) and len(row["reasons"]) >= 1, row
        assert all(isinstance(r, str) and r for r in row["reasons"])
        seen_pairs.add((row["principal_id"], row["resource_id"]))

    assert seen_pairs == {(p, d) for p in principal_ids for d in doc_ids}
