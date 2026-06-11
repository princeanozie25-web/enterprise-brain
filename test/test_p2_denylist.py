"""P-2: no real-company/person string appears in any output file."""

from pathlib import Path

from conftest import FIXTURES_DIR
from synth.constants import DENYLIST


def test_denylist_absent_from_every_output_file() -> None:
    assert len(DENYLIST) >= 24  # the 4 required seeds + 20 recalled distributors
    files = sorted(FIXTURES_DIR.glob("*"))
    assert files, "no fixtures generated"
    hits: list[str] = []
    for path in files:
        text = path.read_text(encoding="utf-8").lower()
        for term in DENYLIST:
            if term.lower() in text:
                hits.append(f"{term!r} in {path.name}")
    assert hits == [], f"denylist strings leaked into fixtures: {hits}"
