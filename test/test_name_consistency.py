"""AR-1b: the HR-title mismatch (AR-1 deviation 2) is fixed AT SOURCE. Every HR
record embeds its subject's humanized display name in both the title and the
body, so no person's own record ever contradicts their masthead.
"""

import json

from conftest import FIXTURES_DIR

from synth import constants


def _people_names() -> dict[str, str]:
    people = json.loads((FIXTURES_DIR / "people.json").read_text(encoding="utf-8"))["people"]
    return {p["id"]: p["display_name"] for p in people}


def test_every_hr_record_embeds_its_subjects_display_name(documents) -> None:
    name_of = _people_names()
    hr = [d for d in documents if d["doc_type"] == "hr_record"]
    assert len(hr) == constants.N_HR_RECORDS == 30, "all HR records present"
    for d in hr:
        name = name_of[d["subject_id"]]
        assert name in d["title"], f"{d['id']} title does not embed {name!r}"
        assert (
            f"Subject of record: {name}." in d["body"]
        ), f"{d['id']} body does not embed the subject name verbatim"


def test_no_document_title_embeds_a_stale_legacy_name(documents) -> None:
    # The only documents that name a principal are HR records, and those now
    # carry the humanized name. A spot guard: the AR-1 hero case (d0093) no
    # longer reads the legacy "Gethin Tarnwold".
    d0093 = next(d for d in documents if d["id"] == "d0093")
    assert "Gethin Tarnwold" not in d0093["title"]
    assert d0093["title"].startswith("HR Record (Absence Summary): ")
