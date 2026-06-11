"""Unit tests for the P-9 schema harness: schema_lite plus test/schemas/.

These tests deliberately do NOT read fixtures/ -- those files are produced
and pushed through the schemas by test_p9_schema.py once generation lands.
What is locked down here:

  * every supported keyword accepts conforming values;
  * every supported keyword rejects non-conforming values, with the
    JSON-path of the offending value carried on the SchemaError;
  * the six checked-in schemas parse, stay inside the supported keyword
    subset, accept hand-built minimal instances shaped exactly like
    synth.model to_dict() output, and reject targeted corruptions;
  * none of this harness's own files trips synth.constants.DENYLIST.

No randomness anywhere: every instance below is a literal.
"""

import json
import sys
from pathlib import Path

import pytest

TEST_DIR = Path(__file__).resolve().parent
REPO_ROOT = TEST_DIR.parent
for _entry in (str(REPO_ROOT), str(TEST_DIR)):
    if _entry not in sys.path:
        sys.path.insert(0, _entry)

import schema_lite  # noqa: E402
from schema_lite import SchemaError, validate  # noqa: E402
from synth import constants  # noqa: E402

SCHEMAS_DIR = TEST_DIR / "schemas"

SCHEMA_FILES = [
    "brm.schema.json",
    "company.schema.json",
    "documents.schema.json",
    "ground_truth_row.schema.json",
    "oracle_stats.schema.json",
    "traps.schema.json",
]


def load_schema(name: str) -> dict:
    return json.loads((SCHEMAS_DIR / name).read_text(encoding="utf-8"))


def expect_error(instance: object, schema: object) -> SchemaError:
    with pytest.raises(SchemaError) as excinfo:
        validate(instance, schema)
    return excinfo.value


# ---------------------------------------------------------------------------
# Minimal valid instances, shaped exactly like synth.model to_dict() output.
# Builders return fresh structures so corruption tests can mutate freely.
# ---------------------------------------------------------------------------


def minimal_company() -> dict:
    return {
        "company": {
            "name": constants.COMPANY_NAME,
            "fictional": True,
            "regulatory_context": "Fictional GDP wholesale-distribution setting",
        },
        "sites": [
            {"id": site, "name": site.removeprefix("site_").title()}
            for site in constants.SITES
        ],
        "departments": list(constants.DEPARTMENTS),
        "people": [
            {
                "id": "p_0001",
                "name": "Synthetic Person 0001",
                "department": constants.DEPARTMENTS[4],
                "role": "Systems Engineer",
                "manager_id": None,
                "employment_band": 5,
                "site": constants.SITES[0],
                "start_date": "2019-02-11",
                "synthetic": True,
            },
            {
                "id": "p_0002",
                "name": "Synthetic Person 0002",
                "department": constants.DEPARTMENTS[0],
                "role": "Quality Technician",
                "manager_id": "p_0001",
                "employment_band": 2,
                "site": constants.SITES[1],
                "start_date": "2022-08-01",
                "synthetic": True,
            },
        ],
        "groups": [
            {
                "id": "grp_quality_compliance",
                "name": "Quality & Compliance",
                "description": "Everyone in the Quality & Compliance department.",
                "member_ids": ["p_0002"],
            }
        ],
        "agents": [
            {
                "id": "agent_0001",
                "name": "Release Checklist Agent",
                "grant": {
                    "groups": ["grp_quality_compliance"],
                    "site": constants.SITES[0],
                    "employment_band": 2,
                },
                "owner_user_id": "p_0001",
                "synthetic": True,
            },
            {
                "id": "agent_0002",
                "name": "Wiki Digest Agent",
                "grant": {"groups": []},
                "owner_user_id": "p_0002",
                "synthetic": True,
            },
        ],
        "sources": list(constants.SOURCES),
    }


def minimal_documents() -> dict:
    return {
        "documents": [
            {
                "id": "doc_0001",
                "source": constants.SOURCES[0],
                "title": "Goods-In Temperature Checks (SOP-014, v2)",
                "body": "Record probe readings at goods-in twice per shift and log exceptions.",
                "author_id": "p_0001",
                "department": constants.DEPARTMENTS[0],
                "created_at": constants.FIXED_EPOCH_ISO,
                "sensitivity": "internal",
                "acl_refs": [
                    {"rule_id": "acl_0001", "kind": "group", "group": "grp_quality_compliance"},
                    {"rule_id": "acl_0002", "kind": "role", "role": "Quality Technician"},
                    {"rule_id": "acl_0003", "kind": "attr_site", "site": constants.SITES[0]},
                    {"rule_id": "acl_0004", "kind": "attr_band_min", "min_band": 2},
                ],
                "version": 2,
                "supersedes": "doc_0000",
                "doc_type": "sop",
                "subject_id": None,
            },
            {
                "id": "doc_0002",
                "source": constants.SOURCES[3],
                "title": "Onboarding Record 0002",
                "body": "Synthetic onboarding checklist for one synthetic employee.",
                "author_id": "p_0001",
                "department": constants.DEPARTMENTS[5],
                "created_at": "2025-12-19T14:30:00Z",
                "sensitivity": "special_category",
                "acl_refs": [],
                "version": 1,
                "supersedes": None,
                "doc_type": "hr_record",
                "subject_id": "p_0002",
            },
        ]
    }


def minimal_brm() -> dict:
    return {
        "strategies": [
            {
                "id": "strat_001",
                "name": "Resilient Cold-Chain Coverage",
                "initiative_ids": ["init_001"],
            }
        ],
        "initiatives": [
            {
                "id": "init_001",
                "name": "Depot Telemetry Rollout",
                "strategy_id": "strat_001",
                "workflow_ids": ["wf_001"],
            }
        ],
        "workflows": [
            {
                "id": "wf_001",
                "name": "Probe Calibration",
                "initiative_id": "init_001",
                "capability_ids": ["cap_001"],
            }
        ],
        "capabilities": [
            {
                "id": "cap_001",
                "name": "Calibration Record Keeping",
                "workflow_id": "wf_001",
                "document_ids": ["doc_0001"],
            }
        ],
    }


def minimal_traps() -> dict:
    return {
        "effective_version": [
            {
                "current_id": "doc_0001",
                "superseded_id": "doc_0000",
                "parameter_class": "temperature_threshold",
            }
        ],
        "mosaic": [
            {
                "doc_a": "doc_0002",
                "doc_b": "doc_0003",
                "principal_id": "p_0001",
                "inferred_fact_class": "site_consolidation_plan",
            }
        ],
        "confused_deputy": [
            {"agent_id": "agent_0001", "owner_id": "p_0001", "resource_id": "doc_0004"}
        ],
        "manager_overreach": [
            {"manager_id": "p_0001", "subject_id": "p_0002", "resource_id": "doc_0005"}
        ],
        "cross_site": [
            {
                "principal_id": "p_0001",
                "resource_id": "doc_0006",
                "required_site": constants.SITES[1],
                "principal_site": constants.SITES[0],
            }
        ],
    }


def minimal_ground_truth_row() -> dict:
    return {
        "principal_id": "p_0001",
        "resource_id": "doc_0001",
        "decision": "ALLOW",
        "reasons": ["rule acl_0001 (group grp_quality_compliance) matched"],
    }


def minimal_oracle_stats() -> dict:
    return {
        "total_pairs": 74400,
        "allow_total": 14880,
        "allow_rate": 0.2,
        "by_sensitivity": {
            sensitivity: {"pairs": 14880, "allows": 1488, "allow_rate": 0.1}
            for sensitivity in constants.SENSITIVITIES
        },
        "restricted_special_allow_rate": 0.02,
    }


# ---------------------------------------------------------------------------
# 1. Each supported keyword accepts conforming values.
# ---------------------------------------------------------------------------

ACCEPT_CASES = [
    ("type-object", {}, {"type": "object"}),
    ("type-array", [], {"type": "array"}),
    ("type-string", "s", {"type": "string"}),
    ("type-integer", 7, {"type": "integer"}),
    ("type-number-float", 7.5, {"type": "number"}),
    ("type-number-int", 7, {"type": "number"}),
    ("type-boolean", False, {"type": "boolean"}),
    ("type-null", None, {"type": "null"}),
    ("type-list", None, {"type": ["string", "null"]}),
    (
        "properties",
        {"a": 1},
        {"type": "object", "properties": {"a": {"type": "integer"}}},
    ),
    ("required", {"a": 1}, {"type": "object", "required": ["a"]}),
    (
        "additionalProperties-false",
        {"a": 1},
        {"type": "object", "properties": {"a": {}}, "additionalProperties": False},
    ),
    ("items", [1, 2], {"type": "array", "items": {"type": "integer"}}),
    ("enum", "internal", {"enum": ["public", "internal"]}),
    ("const", True, {"const": True}),
    ("minimum-inclusive", 1, {"type": "integer", "minimum": 1}),
    ("maximum-inclusive", 5, {"type": "integer", "maximum": 5}),
    ("minItems", ["x"], {"type": "array", "minItems": 1}),
    ("maxItems", ["x"], {"type": "array", "maxItems": 1}),
    ("minLength", "x", {"type": "string", "minLength": 1}),
    (
        "pattern-anchored",
        "2026-01-05",
        {"type": "string", "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},
    ),
    ("pattern-search", "rev v12 final", {"type": "string", "pattern": "v[0-9]+"}),
    ("empty-schema-accepts-anything", {"x": [None]}, {}),
]


@pytest.mark.parametrize(
    ("instance", "schema"),
    [(c[1], c[2]) for c in ACCEPT_CASES],
    ids=[c[0] for c in ACCEPT_CASES],
)
def test_keyword_accepts(instance, schema):
    assert validate(instance, schema) is None


# ---------------------------------------------------------------------------
# 2. Each supported keyword rejects, with the correct JSON-path.
# ---------------------------------------------------------------------------

REJECT_CASES = [
    ("type-mismatch", "3", {"type": "integer"}, "$", "expected type integer"),
    (
        "type-mismatch-nested",
        {"n": "x"},
        {"type": "object", "properties": {"n": {"type": "integer"}}},
        "$.n",
        "expected type integer",
    ),
    (
        "missing-required",
        {},
        {"type": "object", "required": ["n"]},
        "$",
        "missing required property 'n'",
    ),
    (
        "additional-property",
        {"n": 1, "x": 2},
        {"type": "object", "properties": {"n": {}}, "additionalProperties": False},
        "$.x",
        "additional property not allowed",
    ),
    ("enum-violation", "secret", {"enum": ["public", "internal"]}, "$", "enum"),
    ("const-violation", False, {"const": True}, "$", "expected const"),
    ("minimum", 0, {"type": "integer", "minimum": 1}, "$", "minimum"),
    ("maximum", 9, {"type": "integer", "maximum": 5}, "$", "maximum"),
    ("minItems", [], {"type": "array", "minItems": 1}, "$", "minItems"),
    ("maxItems", [1, 2], {"type": "array", "maxItems": 1}, "$", "maxItems"),
    ("minLength", "", {"type": "string", "minLength": 1}, "$", "minLength"),
    (
        "pattern",
        "2026/01/05",
        {"type": "string", "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},
        "$",
        "pattern",
    ),
    (
        "items-element",
        ["ok", 3],
        {"type": "array", "items": {"type": "string"}},
        "$[1]",
        "expected type string",
    ),
]


@pytest.mark.parametrize(
    ("instance", "schema", "path", "fragment"),
    [(c[1], c[2], c[3], c[4]) for c in REJECT_CASES],
    ids=[c[0] for c in REJECT_CASES],
)
def test_keyword_rejects_with_path(instance, schema, path, fragment):
    error = expect_error(instance, schema)
    assert error.path == path
    assert fragment in str(error)


# ---------------------------------------------------------------------------
# 3. Validator semantics that the schemas rely on.
# ---------------------------------------------------------------------------


class TestValidatorSemantics:
    def test_booleans_are_not_integers_or_numbers(self):
        assert expect_error(True, {"type": "integer"}).path == "$"
        assert expect_error(True, {"type": "number"}).path == "$"

    def test_floats_are_not_integers(self):
        assert "expected type integer" in str(expect_error(2.0, {"type": "integer"}))

    def test_enum_and_const_keep_bool_and_int_apart(self):
        expect_error(True, {"enum": [0, 1]})
        expect_error(1, {"const": True})
        assert validate(True, {"const": True}) is None

    def test_keywords_only_constrain_matching_kinds(self):
        # Per JSON Schema, a keyword ignores instances of other kinds.
        assert validate("abc", {"minimum": 99}) is None
        assert validate(7, {"minLength": 99}) is None
        assert validate({}, {"items": {"type": "null"}, "minItems": 3}) is None

    def test_annotation_keywords_are_inert(self):
        schema = {
            "$schema": "x",
            "$id": "y",
            "title": "t",
            "description": "d",
            "type": "integer",
        }
        assert validate(7, schema) is None

    def test_unsupported_keyword_rejected_loudly(self):
        error = expect_error({}, {"type": "object", "minProperties": 1})
        assert "unsupported schema keyword" in str(error)
        assert "minProperties" in str(error)

    def test_unknown_type_name_rejected(self):
        assert "unknown type name" in str(expect_error("x", {"type": "text"}))

    def test_schema_node_must_be_object(self):
        error = expect_error("x", ["not a schema"])
        assert "schema node must be an object" in str(error)

    def test_additional_properties_must_be_boolean(self):
        error = expect_error({}, {"type": "object", "additionalProperties": {}})
        assert "must be a boolean" in str(error)

    def test_error_path_threads_objects_and_arrays(self):
        schema = {
            "type": "object",
            "properties": {
                "rows": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {"n": {"type": "integer", "minimum": 1}},
                    },
                }
            },
        }
        error = expect_error({"rows": [{"n": 3}, {"n": 0}]}, schema)
        assert error.path == "$.rows[1].n"

    def test_required_is_checked_before_additional_properties(self):
        schema = {
            "type": "object",
            "required": ["a"],
            "properties": {"a": {}},
            "additionalProperties": False,
        }
        error = expect_error({"z": 1}, schema)
        assert error.path == "$"
        assert "'a'" in str(error)

    def test_additional_properties_default_allows_undeclared(self):
        schema = {"type": "object", "properties": {"a": {"type": "integer"}}}
        assert validate({"a": 1, "b": "free"}, schema) is None

    def test_type_list_accepts_any_member_and_names_all_in_error(self):
        schema = {"type": ["string", "null"]}
        assert validate(None, schema) is None
        assert validate("x", schema) is None
        assert "string or null" in str(expect_error(3, schema))

    def test_schema_error_exposes_path_and_reason(self):
        error = expect_error(0, {"type": "integer", "minimum": 1})
        assert error.path == "$"
        assert error.reason
        assert str(error).startswith("$: ")

    def test_validate_returns_none_on_success(self):
        schema = {
            "type": "object",
            "required": ["a"],
            "additionalProperties": False,
            "properties": {"a": {"type": "array", "items": {"type": "integer"}}},
        }
        assert validate({"a": [1, 2]}, schema) is None


# ---------------------------------------------------------------------------
# 4. The checked-in schemas: parse, stay in-subset, accept minimal instances.
# ---------------------------------------------------------------------------


def _assert_supported_keywords_only(node: dict, where: str) -> None:
    assert isinstance(node, dict), f"{where}: schema node must be an object"
    for keyword in node:
        assert keyword in schema_lite._SUPPORTED_KEYWORDS, (
            f"{where}: unsupported keyword {keyword!r}"
        )
    for name, sub in node.get("properties", {}).items():
        _assert_supported_keywords_only(sub, f"{where}.properties[{name!r}]")
    if "items" in node:
        _assert_supported_keywords_only(node["items"], f"{where}.items")


def test_exactly_the_six_expected_schema_files_exist():
    found = sorted(p.name for p in SCHEMAS_DIR.glob("*.schema.json"))
    assert found == sorted(SCHEMA_FILES)


@pytest.mark.parametrize("schema_name", SCHEMA_FILES)
def test_schema_file_parses_and_stays_in_subset(schema_name):
    _assert_supported_keywords_only(load_schema(schema_name), schema_name)


MINIMAL_INSTANCES = [
    ("company.schema.json", minimal_company),
    ("documents.schema.json", minimal_documents),
    ("brm.schema.json", minimal_brm),
    ("traps.schema.json", minimal_traps),
    ("ground_truth_row.schema.json", minimal_ground_truth_row),
    ("oracle_stats.schema.json", minimal_oracle_stats),
]


@pytest.mark.parametrize(
    ("schema_name", "builder"),
    MINIMAL_INSTANCES,
    ids=[name for name, _ in MINIMAL_INSTANCES],
)
def test_schema_accepts_minimal_instance(schema_name, builder):
    assert validate(builder(), load_schema(schema_name)) is None


# ---------------------------------------------------------------------------
# 5. Targeted corruptions against each checked-in schema.
# ---------------------------------------------------------------------------


class TestCompanySchema:
    def test_rejects_band_below_one(self):
        instance = minimal_company()
        instance["people"][0]["employment_band"] = 0
        error = expect_error(instance, load_schema("company.schema.json"))
        assert error.path == "$.people[0].employment_band"

    def test_rejects_band_above_five(self):
        instance = minimal_company()
        instance["people"][1]["employment_band"] = 6
        error = expect_error(instance, load_schema("company.schema.json"))
        assert error.path == "$.people[1].employment_band"

    def test_manager_id_key_required_even_though_nullable(self):
        instance = minimal_company()
        del instance["people"][0]["manager_id"]
        error = expect_error(instance, load_schema("company.schema.json"))
        assert error.path == "$.people[0]"
        assert "'manager_id'" in str(error)

    def test_rejects_fictional_false(self):
        instance = minimal_company()
        instance["company"]["fictional"] = False
        error = expect_error(instance, load_schema("company.schema.json"))
        assert error.path == "$.company.fictional"

    def test_rejects_wrong_department_count(self):
        instance = minimal_company()
        instance["departments"] = instance["departments"][:7]
        error = expect_error(instance, load_schema("company.schema.json"))
        assert error.path == "$.departments"

    def test_rejects_synthetic_false_on_person(self):
        instance = minimal_company()
        instance["people"][0]["synthetic"] = False
        error = expect_error(instance, load_schema("company.schema.json"))
        assert error.path == "$.people[0].synthetic"

    def test_rejects_extra_person_property(self):
        instance = minimal_company()
        instance["people"][0]["nickname"] = "extra"
        error = expect_error(instance, load_schema("company.schema.json"))
        assert error.path == "$.people[0].nickname"

    def test_rejects_bad_start_date(self):
        instance = minimal_company()
        instance["people"][0]["start_date"] = "11/02/2019"
        error = expect_error(instance, load_schema("company.schema.json"))
        assert error.path == "$.people[0].start_date"

    def test_rejects_extra_key_in_agent_grant(self):
        instance = minimal_company()
        instance["agents"][0]["grant"]["scope"] = "everything"
        error = expect_error(instance, load_schema("company.schema.json"))
        assert error.path == "$.agents[0].grant.scope"


class TestDocumentsSchema:
    def test_rejects_unknown_sensitivity(self):
        instance = minimal_documents()
        instance["documents"][0]["sensitivity"] = "secret"
        error = expect_error(instance, load_schema("documents.schema.json"))
        assert error.path == "$.documents[0].sensitivity"
        assert "enum" in str(error)

    def test_rejects_unknown_acl_kind(self):
        instance = minimal_documents()
        instance["documents"][0]["acl_refs"][0]["kind"] = "owner"
        error = expect_error(instance, load_schema("documents.schema.json"))
        assert error.path == "$.documents[0].acl_refs[0].kind"

    def test_rejects_version_zero(self):
        instance = minimal_documents()
        instance["documents"][0]["version"] = 0
        error = expect_error(instance, load_schema("documents.schema.json"))
        assert error.path == "$.documents[0].version"

    def test_rejects_timestamp_without_z(self):
        instance = minimal_documents()
        instance["documents"][0]["created_at"] = "2026-01-05T09:00:00+00:00"
        error = expect_error(instance, load_schema("documents.schema.json"))
        assert error.path == "$.documents[0].created_at"

    def test_rejects_unknown_doc_type(self):
        instance = minimal_documents()
        instance["documents"][1]["doc_type"] = "memo"
        error = expect_error(instance, load_schema("documents.schema.json"))
        assert error.path == "$.documents[1].doc_type"

    def test_subject_id_key_required_even_though_nullable(self):
        instance = minimal_documents()
        del instance["documents"][0]["subject_id"]
        error = expect_error(instance, load_schema("documents.schema.json"))
        assert error.path == "$.documents[0]"
        assert "'subject_id'" in str(error)


class TestBrmSchema:
    def test_rejects_missing_layer(self):
        instance = minimal_brm()
        del instance["workflows"]
        error = expect_error(instance, load_schema("brm.schema.json"))
        assert error.path == "$"
        assert "'workflows'" in str(error)

    def test_rejects_workflow_without_parent_pointer(self):
        instance = minimal_brm()
        del instance["workflows"][0]["initiative_id"]
        error = expect_error(instance, load_schema("brm.schema.json"))
        assert error.path == "$.workflows[0]"

    def test_rejects_non_string_document_ids(self):
        instance = minimal_brm()
        instance["capabilities"][0]["document_ids"] = [17]
        error = expect_error(instance, load_schema("brm.schema.json"))
        assert error.path == "$.capabilities[0].document_ids[0]"


class TestTrapsSchema:
    def test_rejects_missing_trap_family(self):
        instance = minimal_traps()
        del instance["cross_site"]
        error = expect_error(instance, load_schema("traps.schema.json"))
        assert error.path == "$"
        assert "'cross_site'" in str(error)

    def test_rejects_empty_trap_family(self):
        instance = minimal_traps()
        instance["mosaic"] = []
        error = expect_error(instance, load_schema("traps.schema.json"))
        assert error.path == "$.mosaic"
        assert "minItems" in str(error)

    def test_rejects_extra_key_in_trap_record(self):
        instance = minimal_traps()
        instance["confused_deputy"][0]["why"] = "because"
        error = expect_error(instance, load_schema("traps.schema.json"))
        assert error.path == "$.confused_deputy[0].why"


class TestOracleStatsSchema:
    def test_rejects_missing_sensitivity_bucket(self):
        stats = minimal_oracle_stats()
        del stats["by_sensitivity"]["restricted"]
        error = expect_error(stats, load_schema("oracle_stats.schema.json"))
        assert error.path == "$.by_sensitivity"
        assert "'restricted'" in str(error)

    def test_rejects_unknown_sensitivity_bucket(self):
        stats = minimal_oracle_stats()
        stats["by_sensitivity"]["top_tier"] = {
            "pairs": 1,
            "allows": 0,
            "allow_rate": 0.0,
        }
        error = expect_error(stats, load_schema("oracle_stats.schema.json"))
        assert error.path == "$.by_sensitivity.top_tier"

    def test_rejects_out_of_range_rate(self):
        stats = minimal_oracle_stats()
        stats["allow_rate"] = 1.5
        error = expect_error(stats, load_schema("oracle_stats.schema.json"))
        assert error.path == "$.allow_rate"
        assert "maximum" in str(error)

    def test_rejects_non_integer_counts(self):
        stats = minimal_oracle_stats()
        stats["total_pairs"] = 74400.0
        error = expect_error(stats, load_schema("oracle_stats.schema.json"))
        assert error.path == "$.total_pairs"
        assert "expected type integer" in str(error)


class TestGroundTruthRowSchema:
    """Spec-mandated checks for the per-line ground-truth schema."""

    def test_accepts_valid_row(self):
        row = minimal_ground_truth_row()
        assert validate(row, load_schema("ground_truth_row.schema.json")) is None

    def test_accepts_deny_row(self):
        row = minimal_ground_truth_row()
        row["decision"] = "DENY"
        row["reasons"] = ["no matching grant rule (deny-by-default)"]
        assert validate(row, load_schema("ground_truth_row.schema.json")) is None

    def test_rejects_empty_reasons(self):
        row = minimal_ground_truth_row()
        row["reasons"] = []
        error = expect_error(row, load_schema("ground_truth_row.schema.json"))
        assert error.path == "$.reasons"
        assert "minItems" in str(error)

    def test_rejects_bad_decision(self):
        row = minimal_ground_truth_row()
        row["decision"] = "PERMIT"
        error = expect_error(row, load_schema("ground_truth_row.schema.json"))
        assert error.path == "$.decision"
        assert "enum" in str(error)

    def test_rejects_blank_reason_string(self):
        row = minimal_ground_truth_row()
        row["reasons"] = ["fine", ""]
        error = expect_error(row, load_schema("ground_truth_row.schema.json"))
        assert error.path == "$.reasons[1]"

    def test_rejects_extra_property(self):
        row = minimal_ground_truth_row()
        row["note"] = "stray"
        error = expect_error(row, load_schema("ground_truth_row.schema.json"))
        assert error.path == "$.note"

    def test_rejects_missing_resource_id(self):
        row = minimal_ground_truth_row()
        del row["resource_id"]
        error = expect_error(row, load_schema("ground_truth_row.schema.json"))
        assert error.path == "$"
        assert "'resource_id'" in str(error)


# ---------------------------------------------------------------------------
# 6. The harness's own files must not trip the denylist.
# ---------------------------------------------------------------------------


def test_harness_files_do_not_trip_denylist():
    files = [TEST_DIR / "schema_lite.py", TEST_DIR / "test_schema_lite.py"]
    files += sorted(SCHEMAS_DIR.glob("*.schema.json"))
    for path in files:
        text = path.read_text(encoding="utf-8").lower()
        for term in constants.DENYLIST:
            assert term.lower() not in text, (
                f"{path.name} contains denylisted string {term!r}"
            )
