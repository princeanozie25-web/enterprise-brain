"""P-1: two runs with the same seed produce byte-identical fixtures, and the
committed fixtures/ ARE that output (regeneration is reproducible)."""

import hashlib
import subprocess
import sys
from pathlib import Path

from conftest import FIXTURES_DIR, REPO_ROOT

FIXTURE_FILES = [
    "company.json",
    "documents.json",
    "brm.json",
    "traps.json",
    "ground_truth.jsonl",
    "oracle_stats.json",
]


def _run_generation(out_dir: Path) -> None:
    result = subprocess.run(
        [sys.executable, "-m", "synth.generate", "--seed", "42", "--out", str(out_dir)],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=600,
    )
    assert result.returncode == 0, f"generation failed:\n{result.stdout}\n{result.stderr}"


def _digests(directory: Path) -> dict[str, str]:
    return {
        name: hashlib.sha256((directory / name).read_bytes()).hexdigest()
        for name in FIXTURE_FILES
    }


def test_two_runs_byte_identical_and_match_committed(tmp_path: Path) -> None:
    run_a = tmp_path / "a"
    run_b = tmp_path / "b"
    _run_generation(run_a)
    _run_generation(run_b)
    digests_a = _digests(run_a)
    assert digests_a == _digests(run_b), "same seed produced different bytes across runs"
    assert digests_a == _digests(FIXTURES_DIR), (
        "committed fixtures do not match regeneration with seed 42"
    )
