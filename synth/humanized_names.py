"""AR-1b: the humanized name source — the SAME seeded pool + assignment that
service/src/humanize.rs uses, ported to Python so the M0 generator bakes the
exact same id -> display_name at the SOURCE.

This is a pure function of the principal id set + the fixed SEED: no rng, no
clock, no I/O. It MUST stay byte-for-byte in step with humanize.rs (the Rust
FIRST_NAMES/SURNAMES/SEED/hash are identical); test/test_humanized_names.py
proves it against the committed fixtures/people.json.

Why a port and not a read of people.json: keeps the generator self-contained
and deterministic (P-1 regenerates to temp dirs), so names are truly baked at
source rather than lifted from a downstream artifact.
"""

from __future__ import annotations

import hashlib

# Fixed seed pinning every choice (identical to humanize.rs SEED).
SEED = "aperture-ar-1"

# Diverse, fictional, mixed-gender pools — identical to humanize.rs.
FIRST_NAMES = [
    "Amara", "Wei", "Priya", "Diego", "Fatima", "Kenji", "Aisha", "Mateo", "Ling", "Omar",
    "Sofia", "Raj", "Nadia", "Hiroshi", "Zara", "Carlos", "Mei", "Ahmed", "Elena", "Kwame",
    "Yuki", "Ana", "Tariq", "Ingrid", "Jin", "Leila", "Pablo", "Sanjay", "Noor", "Hassan",
    "Camila", "Bao", "Ravi", "Yara", "Andrei", "Mariam", "Tao", "Lucia", "Idris", "Keiko",
    "Marco", "Anaya", "Dmitri", "Rania", "Hana", "Felix", "Selina", "Arjun", "Lin", "Oskar",
    "Nia", "Viktor", "Imani", "Tomas", "Chiara", "Rohan", "Asha", "Niko", "Freya", "Samir",
]

SURNAMES = [
    "Chen", "Patel", "Okafor", "Nguyen", "Garcia", "Khan", "Kim", "Rossi", "Andersson", "Tanaka",
    "Mwangi", "Silva", "Cohen", "O'Brien", "Haddad", "Reyes", "Novak", "Adeyemi", "Petrov",
    "Santos", "Yusuf", "Lindqvist", "Park", "Dubois", "Romano", "Mensah", "Sharma", "Ali",
    "Nakamura", "Costa", "Ibrahim", "Walsh", "Kowalski", "Mendoza", "Bauer", "Haq", "Singh",
    "Moreau", "Diallo", "Vargas", "Schmidt", "Lee", "Fernandes", "Abebe", "Hoffmann", "Castillo",
    "Ortega", "Banerjee", "Kaur", "Larsen", "Marino", "Osei", "Volkov", "Nair", "Bianchi",
    "Eriksson", "Suzuki", "Flores", "Tetteh", "Rahman",
]

_SEP = "\x1f"  # unit separator (matches Rust's '\u{1f}')


def _hash_u64(*parts: str) -> int:
    """First 16 hex chars of sha256(SEED + sep + parts...) as a u64 (big-endian).
    Identical to humanize.rs::hash_u64."""
    preimage = SEED + _SEP + _SEP.join(parts)
    return int(hashlib.sha256(preimage.encode("utf-8")).hexdigest()[:16], 16)


def assign(ids: list[str]) -> dict[str, str]:
    """principal id -> unique full name. Deterministic over the sorted id set;
    surnames rotate to break full-name collisions (humanize.rs::assign_names)."""
    used: set[str] = set()
    out: dict[str, str] = {}
    for pid in sorted(ids):
        first = FIRST_NAMES[_hash_u64("first", pid) % len(FIRST_NAMES)]
        base = _hash_u64("surname", pid)
        full = ""
        for offset in range(len(SURNAMES)):
            candidate = f"{first} {SURNAMES[(base + offset) % len(SURNAMES)]}"
            if candidate not in used:
                full = candidate
                break
        if not full:  # unreachable for <=120 ids; never emit a duplicate
            full = f"{first} {SURNAMES[base % len(SURNAMES)]}"
        used.add(full)
        out[pid] = full
    return out
