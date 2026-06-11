"""Tests for synth.banks: determinism, bounds, slot fidelity, denylist hygiene."""

import random

import pytest

from synth import banks, constants

DOC_TYPES = [
    "sop",
    "quality_record",
    "hr_record",
    "board_minutes",
    "customer_account",
    "wiki_page",
    "mail_thread",
    "general",
]

BASE_SLOTS = {
    "sop": {
        "parameter_text": "between 2°C and 8°C",
        "procedure_code": "SOP-WH-014",
        "topic": "Cold Chain Receipt and Putaway",
    },
    "quality_record": {
        "batch_code": "BTC-93412",
        "product": "Veltrazine",
        "disposition": "released",
    },
    "hr_record": {
        "subject_name": "Carwen Ashfeld",
        "salary_band_text": "Band 3 (GBP 32,400 to 37,900)",
        "record_kind": "salary_review",
    },
    "board_minutes": {
        "quarter_label": "Q3 FY26",
        "agenda_items": [
            "Trading update",
            "Risk register review",
            "Capital expenditure request",
        ],
    },
    "customer_account": {
        "customer_name": "Hollowbeck Pharmacy",
        "account_code": "ACC-0231",
    },
    "wiki_page": {"topic": "goods-in checklist"},
    "mail_thread": {"topic": "delivery schedule change"},
    "general": {
        "topic": "service performance",
        "stat_lines": [
            "Average weekly order volume: 412 lines",
            "On-time delivery rate: 97.2 percent",
        ],
    },
}


def test_doc_types_match_module():
    assert banks.DOC_TYPES == DOC_TYPES


def test_render_body_deterministic_for_every_doc_type():
    for doc_type in DOC_TYPES:
        slots = BASE_SLOTS[doc_type]
        body_a = banks.render_body(random.Random(7), doc_type, slots)
        body_b = banks.render_body(random.Random(7), doc_type, slots)
        assert body_a == body_b, f"non-deterministic body for {doc_type}"
        title_a = banks.render_title(random.Random(7), doc_type, slots)
        title_b = banks.render_title(random.Random(7), doc_type, slots)
        assert title_a == title_b, f"non-deterministic title for {doc_type}"
        # Empty slots must be deterministic too.
        empty_a = banks.render_body(random.Random(7), doc_type, {})
        empty_b = banks.render_body(random.Random(7), doc_type, {})
        assert empty_a == empty_b


def test_word_count_bounds_30_seeded_renders_per_doc_type():
    for doc_type in DOC_TYPES:
        for seed in range(30):
            body = banks.render_body(random.Random(seed), doc_type, BASE_SLOTS[doc_type])
            n = len(body.split())
            assert constants.BODY_WORDS_MIN <= n <= constants.BODY_WORDS_MAX, (
                f"{doc_type} seed={seed} gave {n} words"
            )
            # Bounds must also hold with no slots at all.
            empty = banks.render_body(random.Random(seed), doc_type, {})
            m = len(empty.split())
            assert constants.BODY_WORDS_MIN <= m <= constants.BODY_WORDS_MAX, (
                f"{doc_type} (empty slots) seed={seed} gave {m} words"
            )


def test_sop_parameter_text_appears_exactly_once():
    slots = BASE_SLOTS["sop"]
    for seed in range(10):
        body = banks.render_body(random.Random(seed), "sop", slots)
        assert body.count(slots["parameter_text"]) == 1, f"seed={seed}"
        assert slots["procedure_code"] in body


def test_quality_record_slots_present():
    slots = BASE_SLOTS["quality_record"]
    for seed in range(5):
        body = banks.render_body(random.Random(seed), "quality_record", slots)
        assert slots["batch_code"] in body
        assert slots["product"] in body
        assert slots["disposition"] in body


def test_hr_record_slots_present():
    slots = BASE_SLOTS["hr_record"]
    for seed in range(5):
        body = banks.render_body(random.Random(seed), "hr_record", slots)
        assert slots["subject_name"] in body
        assert slots["salary_band_text"] in body
        assert slots["record_kind"] in body


def test_board_minutes_slots_present():
    slots = BASE_SLOTS["board_minutes"]
    for seed in range(5):
        body = banks.render_body(random.Random(seed), "board_minutes", slots)
        assert slots["quarter_label"] in body
        for item in slots["agenda_items"]:
            assert item in body


def test_customer_account_slots_present():
    slots = BASE_SLOTS["customer_account"]
    for seed in range(5):
        body = banks.render_body(random.Random(seed), "customer_account", slots)
        assert slots["customer_name"] in body
        assert slots["account_code"] in body


def test_stat_lines_appear_verbatim():
    stat_lines = [
        "Quarterly order count for the account: 1,284",
        "Average basket value across the period: 612.40",
    ]
    for doc_type in ("general", "customer_account"):
        slots = dict(BASE_SLOTS[doc_type])
        slots["stat_lines"] = stat_lines
        for seed in range(5):
            body = banks.render_body(random.Random(seed), doc_type, slots)
            for line in stat_lines:
                assert line in body, f"{doc_type} seed={seed} missing stat line"


def test_unknown_doc_type_rejected():
    with pytest.raises(ValueError):
        banks.render_body(random.Random(0), "memo", {})
    with pytest.raises(ValueError):
        banks.render_title(random.Random(0), "memo", {})


def _all_bank_strings() -> list[str]:
    strings: list[str] = []
    strings.extend(banks.FIRST_NAMES)
    strings.extend(banks.LAST_NAMES)
    strings.extend(banks.PRODUCT_TERMS)
    strings.extend(banks.CUSTOMER_NAMES)
    for dept, roles in banks.ROLE_BANK.items():
        strings.append(dept)
        strings.extend(roles)
    strings.extend(banks.SITE_DISPLAY.keys())
    strings.extend(banks.SITE_DISPLAY.values())
    return strings


def test_denylist_never_appears_in_banks_or_rendered_output():
    denied = [d.lower() for d in constants.DENYLIST]

    corpus = list(_all_bank_strings())
    # 56 rendered bodies (>= 50) plus their titles, across types and seeds.
    rendered = 0
    for seed in range(7):
        for doc_type in DOC_TYPES:
            corpus.append(banks.render_body(random.Random(seed), doc_type, BASE_SLOTS[doc_type]))
            corpus.append(banks.render_title(random.Random(seed), doc_type, BASE_SLOTS[doc_type]))
            corpus.append(banks.render_body(random.Random(seed + 100), doc_type, {}))
            corpus.append(banks.render_title(random.Random(seed + 100), doc_type, {}))
            rendered += 2
    assert rendered >= 50

    for text in corpus:
        low = text.lower()
        for bad in denied:
            assert bad not in low, f"denylist hit {bad!r} in {text[:80]!r}"


def test_role_bank_covers_every_department_with_head_first():
    assert list(banks.ROLE_BANK.keys()) == constants.DEPARTMENTS
    seen_heads = set()
    for dept in constants.DEPARTMENTS:
        roles = banks.ROLE_BANK[dept]
        assert 5 <= len(roles) <= 8, f"{dept} has {len(roles)} roles"
        assert len(set(roles)) == len(roles), f"duplicate role in {dept}"
        head = roles[0]
        assert head.startswith("Head of") or head == "Chief Executive Officer", (
            f"{dept} first role is not a department head: {head!r}"
        )
        # Exactly one head-style role per department.
        assert sum(1 for r in roles if r.startswith("Head of")) <= 1
        assert head not in seen_heads
        seen_heads.add(head)


def test_name_banks_no_duplicates_and_sized():
    assert len(set(banks.FIRST_NAMES)) == len(banks.FIRST_NAMES)
    assert len(set(banks.LAST_NAMES)) == len(banks.LAST_NAMES)
    assert len(banks.FIRST_NAMES) >= 75
    assert len(banks.LAST_NAMES) >= 75
    assert len(set(banks.PRODUCT_TERMS)) == len(banks.PRODUCT_TERMS) >= 28
    assert len(set(banks.CUSTOMER_NAMES)) == len(banks.CUSTOMER_NAMES) >= 40


def test_site_display_matches_constants():
    assert banks.SITE_DISPLAY == {
        "site_keldonbury": "Keldonbury",
        "site_withermoor": "Withermoor",
    }
    assert sorted(banks.SITE_DISPLAY.keys()) == sorted(constants.SITES)
