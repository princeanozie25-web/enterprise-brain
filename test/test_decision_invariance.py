"""AR-1b: THE decision-invariance proof at the M0 layer.

Regenerating the corpus with humanized names changed NO governance. The oracle
output (ground_truth.jsonl) and the structural fixtures (oracle_stats, traps,
brm) are BYTE-IDENTICAL to the pre-regeneration corpus — recorded below as the
sha256 they carried before AR-1b. Only company.json and documents.json (the
name-bearing files) legitimately differ. If any of these four moves, the
regeneration altered authorization, not just strings.
"""

import hashlib

from conftest import FIXTURES_DIR

# sha256 recorded from the FROZEN (pre-AR-1b) corpus, before regeneration.
PRIOR_INVARIANT_SHAS = {
    "ground_truth.jsonl": "c4661ee0e129b1d3bd2bcf531132a8e62a5bc8c47ebf600e23d61103be376f08",
    "oracle_stats.json": "8049ca0332024e4709ae73e4bbcf573dc7df85e91c29e1de8535413053a01d55",
    "traps.json": "6a7b82757bc2431922894e4dd307a033f966b76a67e5ebd4df92cf3810f87869",
    "brm.json": "1b6a36cd61e10a7410c44be69aaf0b4fbd5db71c63ed04356003c780efd11dc8",
}


def test_oracle_and_structure_byte_identical_to_prior() -> None:
    for name, prior in PRIOR_INVARIANT_SHAS.items():
        got = hashlib.sha256((FIXTURES_DIR / name).read_bytes()).hexdigest()
        assert got == prior, (
            f"{name} changed vs the pre-AR-1b corpus — regeneration altered "
            "governance, not just names; HALT"
        )


def test_trap_battery_unchanged(traps) -> None:
    # The trap count + semantics are unchanged (traps.json is byte-identical
    # above); assert the battery total explicitly for the record.
    total = sum(len(traps[k]) for k in traps)
    assert total == 51, f"trap battery total changed: {total} != 51"
