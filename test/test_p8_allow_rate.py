"""P-8: overall allow rate < 35%; restricted + special_category < 5%.
The stats file is also cross-checked against the raw ground truth."""

from synth.constants import ALLOW_RATE_CEILING, RESTRICTED_SPECIAL_ALLOW_CEILING


def test_allow_rate_ceilings(documents, ground_truth, oracle_stats) -> None:
    sensitivity = {d["id"]: d["sensitivity"] for d in documents}

    total = len(ground_truth)
    allows = sum(1 for r in ground_truth if r["decision"] == "ALLOW")
    rate = allows / total
    assert rate < ALLOW_RATE_CEILING, f"corpus too open: allow rate {rate:.4f}"

    rs_rows = [r for r in ground_truth if sensitivity[r["resource_id"]] in ("restricted", "special_category")]
    rs_allows = sum(1 for r in rs_rows if r["decision"] == "ALLOW")
    rs_rate = rs_allows / len(rs_rows)
    assert rs_rate < RESTRICTED_SPECIAL_ALLOW_CEILING, f"sensitive tiers too open: {rs_rate:.4f}"

    # The published stats file must agree with the raw matrix.
    assert oracle_stats["total_pairs"] == total
    assert oracle_stats["allow_total"] == allows
    assert abs(oracle_stats["allow_rate"] - rate) < 1e-6
    assert abs(oracle_stats["restricted_special_allow_rate"] - rs_rate) < 1e-6
