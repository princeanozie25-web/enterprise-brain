"""P-4: a principal with no groups and no granting attributes gets DENY on
100% of non-public documents (deny-by-default holds end to end)."""

from synth.constants import VOID_PRINCIPAL_ID


def test_void_principal_denied_everywhere_nonpublic(company, documents, ground_truth) -> None:
    # The probe exists and is in no group.
    assert any(p["id"] == VOID_PRINCIPAL_ID for p in company["people"])
    for group in company["groups"]:
        assert VOID_PRINCIPAL_ID not in group["member_ids"], group["id"]

    sensitivity = {d["id"]: d["sensitivity"] for d in documents}
    rows = [r for r in ground_truth if r["principal_id"] == VOID_PRINCIPAL_ID]
    assert len(rows) == 600

    non_public = [r for r in rows if sensitivity[r["resource_id"]] != "public"]
    assert non_public, "corpus has no non-public documents?"
    violators = [r for r in non_public if r["decision"] != "DENY"]
    assert violators == [], f"deny-by-default broken: {violators[:5]}"

    # And public docs are readable (the probe is a person, not a ghost).
    public = [r for r in rows if sensitivity[r["resource_id"]] == "public"]
    assert public and all(r["decision"] == "ALLOW" for r in public)
