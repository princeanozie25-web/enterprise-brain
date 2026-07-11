//! S5c: the CONNECTOR CONFORMANCE KIT — the executable half of the
//! certified-connector contract. A contributor writes a [`SourceConnector`],
//! points [`run_kit`] at it, and gets a ✓/✗ report over the clauses that CAN
//! be mechanically checked:
//!
//!   1. **Enumerate determinism** — two `enumerate()` runs yield identical
//!      objects (keys, bytes, metadata), in the same order.
//!   2. **Bytes-only authority** — no permission-shaped `native_meta` key;
//!      the engine's ingest refusal is additionally PROVEN live against a
//!      kit-shipped poisoned connector ([`poison_probe`]), so a passing run
//!      demonstrates the guard fires, not merely that it was dodged.
//!   3. **Hash pinning round-trip** — the content hash (the estate law:
//!      `sha256(doc_id \0 body \0 …)` in doc-id order) is stable across runs
//!      and, when a pin is supplied, matches it.
//!
//! Clause 4 of the contract — **ingest-time-only discipline** (a connector
//! must never run on the request path) — cannot be proven from outside the
//! serving process, so it is a REVIEW CRITERION: reviewers check that the
//! connector is only ever invoked from load/startup code, never from a
//! handler. The kit prints that reminder in its report.
//!
//! This module is test-harness code: nothing in the serving path calls it.

use std::collections::BTreeMap;

use crate::estate::{ingest, RawObject, SourceConnector};
use retrieval::index::sha256_hex;

/// What the contributor knows about their fixture source ahead of time.
/// Everything is optional — absent expectations skip their comparisons
/// (self-consistency is still enforced).
#[derive(Default)]
pub struct KitExpectations {
    /// Expected object count, if known.
    pub object_count: Option<usize>,
    /// Expected pinned content hash, if the source's access model pins one.
    pub content_sha256: Option<String>,
    /// The doc-id prefix the estate applies to this source's native keys
    /// (e.g. `"s3/"` for the fs_bucket store) — needed to reproduce the
    /// access model's pinned hash exactly. Defaults to no prefix.
    pub doc_id_prefix: Option<String>,
}

/// One kit check: named, pass/fail, with the detail a PR review can quote.
pub struct KitCheck {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

/// The kit's report over one connector.
pub struct KitReport {
    pub checks: Vec<KitCheck>,
}

impl KitReport {
    pub fn all_ok(&self) -> bool {
        self.checks.iter().all(|c| c.ok)
    }

    /// Human rendering — what a contributor pastes into their PR.
    pub fn to_human(&self) -> String {
        let mut out = String::from("connector conformance kit\n");
        for check in &self.checks {
            out.push_str(&format!(
                "  {} {}: {}\n",
                if check.ok { "\u{2713}" } else { "\u{2717}" },
                check.name,
                check.detail
            ));
        }
        out.push_str(
            "  \u{2139} ingest-time-only discipline is a REVIEW criterion: the connector \
             must only run at load/startup, never on the request path.\n",
        );
        out
    }
}

/// Run the mechanical clauses against a connector. Read-only with respect to
/// the source; the connector is enumerated twice (determinism needs two runs).
pub fn run_kit(connector: &dyn SourceConnector, expected: &KitExpectations) -> KitReport {
    let mut checks = Vec::new();

    // Clause 2 first: ingest — which itself refuses permission-shaped
    // metadata — is how the kit obtains the objects at all (fail-closed:
    // a smuggling connector produces a report of exactly one ✗).
    let run_a = match ingest(connector) {
        Ok(objects) => objects,
        Err(err) => {
            checks.push(KitCheck {
                name: "authority.bytes_only",
                ok: false,
                detail: format!("ingest REFUSED this connector: {err:#}"),
            });
            return KitReport { checks };
        }
    };
    checks.push(KitCheck {
        name: "authority.bytes_only",
        ok: true,
        detail: format!(
            "{} objects ingested; no permission-shaped native_meta key",
            run_a.len()
        ),
    });

    // Clause 1: determinism across a second enumerate.
    match ingest(connector) {
        Ok(run_b) => {
            let identical = runs_identical(&run_a, &run_b);
            checks.push(KitCheck {
                name: "enumerate.determinism",
                ok: identical,
                detail: if identical {
                    format!(
                        "two runs identical ({} objects, same order/bytes/meta)",
                        run_a.len()
                    )
                } else {
                    "two enumerate() runs DIFFER — a connector must be deterministic \
                     for a fixed source state"
                        .to_string()
                },
            });

            // Clause 3: the content hash is stable across runs…
            let prefix = expected.doc_id_prefix.as_deref().unwrap_or("");
            let hash_a = content_hash(&run_a, prefix);
            let hash_b = content_hash(&run_b, prefix);
            let stable = hash_a == hash_b;
            // …and matches the pin when one is supplied.
            let (ok, detail) = match &expected.content_sha256 {
                Some(_) if !stable => (false, "hash differs across runs".to_string()),
                Some(pin) if &hash_a == pin => {
                    (true, format!("stable across runs AND matches the pin {pin}"))
                }
                Some(pin) => (
                    false,
                    format!("stable across runs but does NOT match the pin: computed {hash_a}, pinned {pin}"),
                ),
                None if stable => (true, format!("stable across runs: {hash_a} (no pin supplied)")),
                None => (false, "hash differs across runs".to_string()),
            };
            checks.push(KitCheck {
                name: "hash.round_trip",
                ok,
                detail,
            });
        }
        Err(err) => checks.push(KitCheck {
            name: "enumerate.determinism",
            ok: false,
            detail: format!("second enumerate failed: {err:#}"),
        }),
    }

    if let Some(expected_count) = expected.object_count {
        let ok = run_a.len() == expected_count;
        checks.push(KitCheck {
            name: "object.count",
            ok,
            detail: format!("{} objects (expected {expected_count})", run_a.len()),
        });
    }

    KitReport { checks }
}

/// PROVE the engine's smuggling rejection fires: run ingest over the
/// kit-shipped poisoned connector and demand refusal. A kit run that skips
/// this proves nothing about the guard; certification requires it.
pub fn poison_probe() -> KitCheck {
    match ingest(&PoisonedConnector) {
        Err(err) => KitCheck {
            name: "authority.poison_probe",
            ok: true,
            detail: format!("ingest REFUSED the poisoned connector, as it must: {err:#}"),
        },
        Ok(_) => KitCheck {
            name: "authority.poison_probe",
            ok: false,
            detail: "ingest ACCEPTED a connector emitting permission-shaped metadata — \
                     the authority guard did not fire"
                .to_string(),
        },
    }
}

/// The kit's poisoned fixture: a connector that tries to smuggle authority
/// through `native_meta` (`acl`). Exists so every certification run proves
/// the refusal live.
pub struct PoisonedConnector;

impl SourceConnector for PoisonedConnector {
    fn source_id(&self) -> &str {
        "kit_poisoned"
    }
    fn enumerate(&self) -> anyhow::Result<Vec<RawObject>> {
        let mut native_meta = BTreeMap::new();
        native_meta.insert("mime".to_string(), "text/plain".to_string());
        native_meta.insert("acl".to_string(), "everyone".to_string());
        Ok(vec![RawObject {
            native_key: "poisoned/object".to_string(),
            bytes: b"authority does not live with the document".to_vec(),
            native_meta,
        }])
    }
}

fn runs_identical(a: &[RawObject], b: &[RawObject]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b.iter()).all(|(x, y)| {
            x.native_key == y.native_key && x.bytes == y.bytes && x.native_meta == y.native_meta
        })
}

/// The estate content-hash law over a connector's raw objects:
/// `sha256(doc_id \0 body \0 …)` in doc-id order, where
/// `doc_id = prefix + native_key`.
fn content_hash(objects: &[RawObject], doc_id_prefix: &str) -> String {
    let mut sorted: Vec<(String, &[u8])> = objects
        .iter()
        .map(|o| {
            (
                format!("{doc_id_prefix}{}", o.native_key),
                o.bytes.as_slice(),
            )
        })
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut preimage = Vec::new();
    for (doc_id, bytes) in sorted {
        preimage.extend_from_slice(doc_id.as_bytes());
        preimage.push(0);
        preimage.extend_from_slice(bytes);
        preimage.push(0);
    }
    sha256_hex(&preimage)
}
