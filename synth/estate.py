"""S3 estate fixture generator (deterministic, seeded).

Emits the SECOND source of the multi-source estate and its SEPARATE
authority file, plus an independently-computed ground truth. Run:

    python -m synth.estate            # writes under fixtures/estate/

The architectural sentence this fixture proves: PERMISSIONS DO NOT LIVE
WITH THE DOCUMENT. The 150 objects carry ZERO permission metadata — they
are bytes in buckets. Authority lives entirely in `s3-access.json`
(per-object sensitivity labels + the two estate agents' tier grants).
The oracle here recomputes expected decisions from FIRST PRINCIPLES,
independent of the Rust engine that is the system under test.

The primary Bryremead corpus (600 docs, 124 principals) is source 1 and
is left BYTE-IDENTICAL — this generator never touches it. The estate is
additive: two estate agents whose authority spans both sources by
sensitivity tier, and a second source of 150 filesystem objects.
"""

from __future__ import annotations

import hashlib
import json
import random
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
FIXTURES = REPO / "fixtures"
ESTATE = FIXTURES / "estate"
STORE = ESTATE / "s3-store"

# Sensitivity tiers, ordered. An estate agent with tier T may read a
# resource of sensitivity S iff level(S) <= level(T). This is the WHOLE
# authority rule for the estate agents, over BOTH sources.
TIER_LEVEL = {
    "public": 0,
    "internal": 1,
    "confidential": 2,
    "restricted": 3,
    "special_category": 4,
}

# The two new estate agents (agent A / agent B), spanning both sources.
ESTATE_AGENTS = {
    "agent_estate_confidential": "confidential",  # A — up to confidential
    "agent_estate_internal": "internal",          # B — internal only
}

# The three buckets: (name, count, label). 150 objects total.
BUCKETS = [
    ("ops-shared", 60, "internal"),
    ("finance-restricted", 50, "confidential"),
    ("public-notices", 40, "public"),
]

# Bryremead-flavoured content banks (the synthetic pharma-distribution
# world of the primary corpus). Deterministic assembly, no PII.
SUBJECTS = [
    "cold-chain transit", "supplier audit", "warehouse rota", "goods-in check",
    "returns reconciliation", "picking accuracy", "despatch cutoff",
    "temperature excursion", "stock rotation", "quarantine hold",
    "credit terms", "invoice matching", "payment run", "margin review",
    "budget variance", "quarterly forecast", "supplier rebate", "cost centre",
    "site notice", "opening hours", "car park", "fire drill", "visitor policy",
    "recycling", "canteen menu", "charity week", "town hall", "wellbeing",
]
SITES = ["Keldonbury", "Marrowfen", "Ashcombe", "Draymoor"]
QUARTERS = ["2026/q1", "2026/q2", "2026/q3"]


def slugify(text: str) -> str:
    return "".join(c if c.isalnum() else "-" for c in text.lower()).strip("-")


def body_for(rng: random.Random, bucket: str, label: str, subject: str, site: str) -> str:
    """A deterministic, realistic plain-text/markdown body. Carries a
    distinctive coined token so probe anchors can target it, and NEVER any
    permission metadata."""
    tag = f"EST-{bucket[:3].upper()}-{rng.randint(1000, 9999)}"
    lines = [
        f"# {subject.title()} — {site}",
        "",
        f"Reference: {tag}",
        f"Site: {site} distribution centre.",
        "",
        f"This note concerns {subject} handled through the {site} operation. "
        f"It forms part of the {bucket.replace('-', ' ')} record set and is "
        f"reviewed on the standard cycle.",
        "",
        "## Detail",
        "",
        f"The {subject} process is carried out by the responsible team and "
        f"logged in the operational system. Any exceptions are escalated "
        f"under the site procedure and closed within the agreed window.",
    ]
    if label == "confidential":
        lines += [
            "",
            "## Commercial (restricted circulation)",
            "",
            f"Commercial terms for {subject} are held here for finance review: "
            f"the negotiated position and counterparty figures are recorded in "
            f"the schedule below and are not for general distribution.",
        ]
    elif label == "internal":
        lines += [
            "",
            "## Operational notes",
            "",
            f"Staff handling {subject} should follow the current working "
            f"instruction; questions go to the shift lead.",
        ]
    else:  # public
        lines += [
            "",
            "This notice is for general information and may be shared freely.",
        ]
    return "\n".join(lines) + "\n"


def generate(seed: int = 20260711) -> dict:
    rng = random.Random(seed)
    objects: list[dict] = []
    used_keys: set[str] = set()

    for bucket, count, label in BUCKETS:
        for _ in range(count):
            subject = rng.choice(SUBJECTS)
            site = rng.choice(SITES)
            quarter = rng.choice(QUARTERS)
            # Path-like key, made unique within the bucket by an ordinal
            # suffix when the natural key already exists (guarantees exactly
            # `count` objects per bucket).
            base = f"{quarter}/{slugify(subject)}-{slugify(site)}"
            key = f"{base}.md"
            suffix = 1
            while f"{bucket}/{key}" in used_keys:
                suffix += 1
                key = f"{base}-{suffix}.md"
            used_keys.add(f"{bucket}/{key}")
            body = body_for(rng, bucket, label, subject, site)
            objects.append(
                {
                    "bucket": bucket,
                    "key": key,
                    "doc_id": f"s3/{bucket}/{key}",
                    "title": f"{subject.title()} — {site}",
                    "sensitivity": label,
                    "body": body,
                }
            )

    return {"objects": objects}


def write_store(estate: dict) -> None:
    for obj in estate["objects"]:
        path = STORE / obj["bucket"] / obj["key"]
        path.parent.mkdir(parents=True, exist_ok=True)
        # The object is BYTES ONLY — no sidecar, no permission metadata.
        path.write_text(obj["body"], encoding="utf-8", newline="\n")


def content_sha256(estate: dict) -> str:
    """The pinned content hash: sha256 over `doc_id\\0body` for every
    object, sorted by doc_id. Verified at load, same integrity law as the
    primary corpus — a tampered object body fails startup."""
    hasher = hashlib.sha256()
    for obj in sorted(estate["objects"], key=lambda o: o["doc_id"]):
        hasher.update(obj["doc_id"].encode("utf-8"))
        hasher.update(b"\x00")
        hasher.update(obj["body"].encode("utf-8"))
        hasher.update(b"\x00")
    return hasher.hexdigest()


def write_access(estate: dict) -> None:
    """s3-access.json: the SEPARATE authority file — the second oracle
    input. Object labels + the estate agents' tier grants + the pinned
    content hash. The objects themselves carry none of this."""
    access = {
        "_note": (
            "Authority for the S3 estate. Objects carry ZERO permission "
            "metadata; all authority is here. Estate agents read a resource "
            "(either source) iff its sensitivity level <= the agent's tier "
            "level. Principals absent from `agent_tiers` have no estate "
            "grant and are denied every object (fail-closed seam default)."
        ),
        "content_sha256": content_sha256(estate),
        "tier_levels": TIER_LEVEL,
        "agent_tiers": dict(ESTATE_AGENTS),
        "objects": [
            {"doc_id": obj["doc_id"], "sensitivity": obj["sensitivity"]}
            for obj in sorted(estate["objects"], key=lambda o: o["doc_id"])
        ],
    }
    (ESTATE / "s3-access.json").write_text(
        json.dumps(access, indent=2, sort_keys=False) + "\n", encoding="utf-8", newline="\n"
    )


def primary_sensitivities() -> dict[str, str]:
    docs = json.loads((FIXTURES / "documents.json").read_text(encoding="utf-8"))
    return {d["id"]: d["sensitivity"] for d in docs["documents"]}


def existing_principals() -> list[str]:
    principals: set[str] = set()
    with open(FIXTURES / "ground_truth.jsonl", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                principals.add(json.loads(line)["principal_id"])
    return sorted(principals)


def tier_allows(agent_tier: str, sensitivity: str) -> bool:
    """THE estate authority rule, from first principles."""
    return TIER_LEVEL[sensitivity] <= TIER_LEVEL[agent_tier]


def write_ground_truth(estate: dict) -> dict:
    """The INDEPENDENT oracle: expected decisions for every estate-relevant
    (principal, resource) pair, computed from first principles. Emits the
    20,100 NEW pairs — the existing 74,400 (existing principals x primary)
    stay in the primary ground_truth.jsonl and are reused verbatim.

    New pairs:
      - estate agents x 600 primary docs      (tier over primary sensitivity)
      - estate agents x 150 s3 objects         (tier over object label)
      - existing principals x 150 s3 objects   (all DENY — no estate grant)
    """
    primary_sens = primary_sensitivities()
    existing = existing_principals()
    s3_sens = {o["doc_id"]: o["sensitivity"] for o in estate["objects"]}
    rows: list[dict] = []

    # Estate agents over BOTH sources.
    for agent, tier in sorted(ESTATE_AGENTS.items()):
        for doc_id, sens in sorted(primary_sens.items()):
            rows.append(_row(agent, doc_id, "primary", tier_allows(tier, sens)))
        for doc_id, sens in sorted(s3_sens.items()):
            rows.append(_row(agent, doc_id, "s3", tier_allows(tier, sens)))

    # Existing principals over the second source: all DENY (fail-closed
    # seam default — no estate grant reaches across the seam).
    for principal in existing:
        for doc_id in sorted(s3_sens):
            rows.append(_row(principal, doc_id, "s3", False))

    rows.sort(key=lambda r: (r["principal_id"], r["resource_id"]))
    text = "\n".join(json.dumps(r, sort_keys=True, ensure_ascii=False) for r in rows)
    (ESTATE / "estate_ground_truth.jsonl").write_text(text + "\n", encoding="utf-8", newline="\n")

    return {
        "estate_pairs": len(rows),
        "estate_agents": len(ESTATE_AGENTS),
        "s3_objects": len(estate["objects"]),
        "primary_docs": len(primary_sens),
        "existing_principals": len(existing),
        "full_estate_matrix": (len(existing) + len(ESTATE_AGENTS))
        * (len(primary_sens) + len(estate["objects"])),
    }


def _row(principal: str, resource: str, source: str, allow: bool) -> dict:
    return {
        "principal_id": principal,
        "resource_id": resource,
        "source": source,
        "decision": "ALLOW" if allow else "DENY",
    }


def main() -> int:
    estate = generate()
    ESTATE.mkdir(parents=True, exist_ok=True)
    write_store(estate)
    write_access(estate)
    stat = write_ground_truth(estate)
    print(json.dumps(stat, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
