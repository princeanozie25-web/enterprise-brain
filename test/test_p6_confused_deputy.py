"""P-6: all planted confused-deputy scenarios resolve DENY — the agent's
grant alone would allow, but the intersection with the owner clamps it."""

from synth.constants import MIN_CONFUSED_DEPUTY


def test_all_confused_deputies_deny(company, traps, ground_truth) -> None:
    scenarios = traps["confused_deputy"]
    assert len(scenarios) >= MIN_CONFUSED_DEPUTY

    agents = {a["id"]: a for a in company["agents"]}
    index = {(r["principal_id"], r["resource_id"]): r for r in ground_truth}

    for s in scenarios:
        assert agents[s["agent_id"]]["owner_user_id"] == s["owner_id"]
        row = index[(s["agent_id"], s["resource_id"])]
        assert row["decision"] == "DENY", f"confused deputy ALLOWED: {s}"
        # The deny must come from the owner side of the intersection: the
        # grant side alone matched (that is what makes it a deputy trap).
        assert "D_AGENT_OWNER" in row["reasons"], row
        assert "D_AGENT_GRANT" not in row["reasons"], (
            "not a real deputy trap: the agent's own grant also denied", s, row,
        )
