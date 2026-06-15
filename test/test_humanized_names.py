"""AR-1b: the M0 generator's name source is the SAME assignment the service's
humanize.rs uses. Proven against the committed fixtures/people.json — the
cross-language invariant that keeps the corpus and the humanization layer in
step on every name.
"""

import json

from conftest import FIXTURES_DIR

from synth import humanized_names


def _people_names() -> dict[str, str]:
    people = json.loads((FIXTURES_DIR / "people.json").read_text(encoding="utf-8"))["people"]
    return {p["id"]: p["display_name"] for p in people}


def test_generator_assignment_equals_people_json() -> None:
    expected = _people_names()
    got = humanized_names.assign(list(expected.keys()))
    assert got == expected, "synth.humanized_names diverged from people.json (humanize.rs)"


def test_company_names_baked_at_source_match_people_json(company) -> None:
    expected = _people_names()
    actual = {p["id"]: p["name"] for p in company["people"]}
    assert actual == expected, "company.json names must equal the humanized display names"
    assert len(actual) == 120
