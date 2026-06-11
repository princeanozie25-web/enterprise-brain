"""P-5: every person ALLOWs on their own HR record; their manager DENYs."""


def _decision_index(ground_truth):
    return {(r["principal_id"], r["resource_id"]): r for r in ground_truth}


def test_subject_allow_manager_deny(company, documents, ground_truth) -> None:
    people = {p["id"]: p for p in company["people"]}
    hr_records = [d for d in documents if d["doc_type"] == "hr_record"]
    assert len(hr_records) >= 30
    index = _decision_index(ground_truth)

    for record in hr_records:
        subject_id = record["subject_id"]
        assert subject_id in people, record["id"]

        subject_row = index[(subject_id, record["id"])]
        assert subject_row["decision"] == "ALLOW", (record["id"], subject_row)
        assert "R_SUBJECT_SELF" in subject_row["reasons"]

        manager_id = people[subject_id]["manager_id"]
        assert manager_id is not None, "HR subjects must have a manager for this trap"
        manager_row = index[(manager_id, record["id"])]
        assert manager_row["decision"] == "DENY", (
            f"manager {manager_id} can read report {subject_id}'s HR record {record['id']}"
        )
