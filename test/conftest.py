"""Shared pytest plumbing: repo-root import path and fixture loaders."""

import json
import sys
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT))

FIXTURES_DIR = REPO_ROOT / "fixtures"


@pytest.fixture(scope="session")
def fixtures_dir() -> Path:
    return FIXTURES_DIR


@pytest.fixture(scope="session")
def company(fixtures_dir: Path) -> dict:
    return json.loads((fixtures_dir / "company.json").read_text(encoding="utf-8"))


@pytest.fixture(scope="session")
def documents(fixtures_dir: Path) -> list[dict]:
    data = json.loads((fixtures_dir / "documents.json").read_text(encoding="utf-8"))
    return data["documents"]


@pytest.fixture(scope="session")
def traps(fixtures_dir: Path) -> dict:
    return json.loads((fixtures_dir / "traps.json").read_text(encoding="utf-8"))


@pytest.fixture(scope="session")
def oracle_stats(fixtures_dir: Path) -> dict:
    return json.loads((fixtures_dir / "oracle_stats.json").read_text(encoding="utf-8"))


@pytest.fixture(scope="session")
def ground_truth(fixtures_dir: Path) -> list[dict]:
    rows = []
    with (fixtures_dir / "ground_truth.jsonl").open(encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows
