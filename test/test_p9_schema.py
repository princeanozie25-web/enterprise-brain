"""P-9: every fixture file validates against its checked-in JSON Schema."""

import json
from pathlib import Path

from conftest import FIXTURES_DIR
from schema_lite import validate

SCHEMAS_DIR = Path(__file__).resolve().parent / "schemas"


def _schema(name: str) -> dict:
    return json.loads((SCHEMAS_DIR / name).read_text(encoding="utf-8"))


def test_company_schema(company) -> None:
    validate(company, _schema("company.schema.json"))


def test_documents_schema(documents) -> None:
    validate({"documents": documents}, _schema("documents.schema.json"))


def test_brm_schema(fixtures_dir) -> None:
    brm = json.loads((fixtures_dir / "brm.json").read_text(encoding="utf-8"))
    validate(brm, _schema("brm.schema.json"))


def test_traps_schema(traps) -> None:
    validate(traps, _schema("traps.schema.json"))


def test_oracle_stats_schema(oracle_stats) -> None:
    validate(oracle_stats, _schema("oracle_stats.schema.json"))


def test_ground_truth_rows_schema(ground_truth) -> None:
    schema = _schema("ground_truth_row.schema.json")
    for row in ground_truth:
        validate(row, schema)
