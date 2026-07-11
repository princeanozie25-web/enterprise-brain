//! S3: the multi-source estate — a SECOND source behind a connector seam,
//! governed ACROSS the seam by an authority model that never lives with the
//! documents.
//!
//! THE ARCHITECTURAL SENTENCE this module exists to prove: PERMISSIONS DO
//! NOT LIVE WITH THE DOCUMENT. An object in a bucket is bytes; it carries no
//! authority. Connectors deliver bytes (ingest-time only, never on the
//! request path); the access model — `s3-access.json`, a file the objects
//! know nothing about — delivers authority; the oracle proves the two never
//! blur.
//!
//! The estate is ADDITIVE: the primary Bryremead corpus (source 1: 600 docs,
//! 124 principals, the M1-compiled world) is BYTE-IDENTICAL and untouched.
//! Two estate agents whose authority spans both sources by sensitivity tier,
//! and a second source of 150 filesystem objects, compose over it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use retrieval::index::sha256_hex;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// The connector seam (community-edition seed)
// ---------------------------------------------------------------------------

/// One raw object a connector delivers at INGEST TIME. `native_meta` is
/// DESCRIPTIVE only (mime, mtime, location) — a connector that emits
/// anything permission-shaped causes ingest to refuse the whole source.
pub struct RawObject {
    pub native_key: String,
    pub bytes: Vec<u8>,
    pub native_meta: BTreeMap<String, String>,
}

/// A source connector. INGEST-TIME ONLY: `enumerate` runs once at startup;
/// request-time serving comes from the verified in-memory store, never a
/// connector.
///
/// THE CERTIFIED-CONNECTOR CONTRACT (what a future connector signs up to
/// before it may join a production estate):
///   1. Permission semantics DOCUMENTED — the connector states, in prose,
///      exactly how the source's native authority maps to the estate access
///      model, or declares it carries none (this seed's stance).
///   2. ACL inheritance TESTED — nested containers/prefixes must not silently
///      widen or narrow authority; a test pins the mapping.
///   3. Revocation latency MEASURED — the worst-case delay between an
///      upstream permission change and the estate reflecting it, published.
///   4. Conformance PASSED — the source's (principal x object) matrix is
///      oracle-checked 0-false-allow / 0-false-deny before release.
///
/// The two seed connectors ([`FsBucketConnector`], and the json-corpus
/// refactor of the primary load) carry authority for NONE of their objects
/// — authority lives in the access model — the strongest form of clause 1:
/// there is nothing to map.
pub trait SourceConnector {
    fn source_id(&self) -> &str;
    fn enumerate(&self) -> Result<Vec<RawObject>>;
}

/// Native-meta keys a connector may NEVER emit: anything that would let a
/// source smuggle authority in past the access model (fail-closed —
/// [`ingest`] rejects the source if any appears).
const PERMISSION_SHAPED_META: &[&str] = &[
    "acl",
    "acls",
    "allow",
    "deny",
    "grant",
    "grants",
    "group",
    "groups",
    "label",
    "permission",
    "permissions",
    "principal",
    "principals",
    "role",
    "roles",
    "scope",
    "sensitivity",
    "visibility",
];

/// The filesystem connector: the S3-shaped store at
/// `fixtures/estate/s3-store/<bucket>/<key>`. INGEST-TIME ONLY.
pub struct FsBucketConnector {
    source_id: String,
    root: PathBuf,
}

impl FsBucketConnector {
    pub fn new(source_id: &str, root: &Path) -> FsBucketConnector {
        FsBucketConnector {
            source_id: source_id.to_string(),
            root: root.to_path_buf(),
        }
    }

    fn walk(dir: &Path, root: &Path, out: &mut Vec<RawObject>) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .with_context(|| format!("cannot read estate dir {}", dir.display()))?
            .collect::<std::result::Result<_, _>>()?;
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                Self::walk(&path, root, out)?;
            } else {
                let rel = path
                    .strip_prefix(root)
                    .expect("walk stays under root")
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes = std::fs::read(&path)
                    .with_context(|| format!("cannot read estate object {}", path.display()))?;
                let mut native_meta = BTreeMap::new();
                native_meta.insert("mime".to_string(), "text/markdown".to_string());
                // Descriptive location only — NOT authority.
                if let Some(bucket) = rel.split('/').next() {
                    native_meta.insert("bucket".to_string(), bucket.to_string());
                }
                out.push(RawObject {
                    native_key: rel,
                    bytes,
                    native_meta,
                });
            }
        }
        Ok(())
    }
}

impl SourceConnector for FsBucketConnector {
    fn source_id(&self) -> &str {
        &self.source_id
    }

    fn enumerate(&self) -> Result<Vec<RawObject>> {
        let mut out = Vec::new();
        if self.root.exists() {
            Self::walk(&self.root, &self.root, &mut out)?;
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// The access model (the SEPARATE authority — s3-access.json)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct AccessFile {
    content_sha256: String,
    tier_levels: BTreeMap<String, u8>,
    agent_tiers: BTreeMap<String, String>,
    objects: Vec<AccessObject>,
}

#[derive(Debug, Deserialize)]
struct AccessObject {
    doc_id: String,
    sensitivity: String,
}

/// One served estate object (second source), after ingest + verification.
#[derive(Debug, Clone)]
pub struct EstateObject {
    pub doc_id: String,
    pub title: String,
    pub body: String,
    pub sensitivity: String,
    pub bucket: String,
    pub key: String,
}

/// The estate authority + the verified second-source store. Loaded once at
/// startup; consulted on the request path for authorization and serving
/// (never a connector at request time).
pub struct EstateModel {
    /// Second-source objects by doc_id (`s3/<bucket>/<key>`).
    objects: BTreeMap<String, EstateObject>,
    /// Estate agent -> tier level. The ONLY principals with any estate grant.
    agent_tier_level: BTreeMap<String, u8>,
    /// Sensitivity name -> level (shared across both sources).
    tier_levels: BTreeMap<String, u8>,
}

impl EstateModel {
    /// Loads the estate: ingest the second source through the `fs_bucket`
    /// connector, refuse any permission-shaped native_meta, verify the
    /// content hash pinned in the access file, and bind the tier grants.
    /// Fail-closed at every step.
    pub fn load(estate_dir: &Path) -> Result<EstateModel> {
        let access_path = estate_dir.join("s3-access.json");
        let access_bytes = std::fs::read(&access_path)
            .with_context(|| format!("cannot read {}", access_path.display()))?;
        let access: AccessFile = serde_json::from_slice(&access_bytes)
            .with_context(|| format!("{} fails parse", access_path.display()))?;

        let connector = FsBucketConnector::new("s3", &estate_dir.join("s3-store"));
        let raw = ingest(&connector)?;

        // Bind each ingested object to its label from the access file — the
        // objects carry no label themselves.
        let label_of: BTreeMap<&str, &str> = access
            .objects
            .iter()
            .map(|o| (o.doc_id.as_str(), o.sensitivity.as_str()))
            .collect();
        let mut objects = BTreeMap::new();
        for object in raw {
            // native_key is `<bucket>/<key>`; the estate doc id prefixes s3/.
            let doc_id = format!("s3/{}", object.native_key);
            let Some(&sensitivity) = label_of.get(doc_id.as_str()) else {
                bail!("estate object {doc_id} has no label in s3-access.json; refusing");
            };
            if !access.tier_levels.contains_key(sensitivity) {
                bail!("estate object {doc_id} carries unknown sensitivity {sensitivity:?}");
            }
            let bucket = object
                .native_meta
                .get("bucket")
                .cloned()
                .unwrap_or_default();
            let key = object
                .native_key
                .strip_prefix(&format!("{bucket}/"))
                .unwrap_or(&object.native_key)
                .to_string();
            let body = String::from_utf8(object.bytes)
                .with_context(|| format!("estate object {doc_id} is not UTF-8"))?;
            let title = first_heading(&body).unwrap_or_else(|| doc_id.clone());
            objects.insert(
                doc_id.clone(),
                EstateObject {
                    doc_id,
                    title,
                    body,
                    sensitivity: sensitivity.to_string(),
                    bucket,
                    key,
                },
            );
        }

        // Integrity: the pinned content hash must match the ingested bodies
        // (same law as the primary corpus — a tampered body fails startup).
        let computed = content_hash(&objects);
        if computed != access.content_sha256 {
            bail!(
                "estate content hash mismatch (objects tampered?): pinned {}, computed {computed}",
                access.content_sha256
            );
        }
        if objects.len() != access.objects.len() {
            bail!(
                "estate object count mismatch: {} on disk, {} in access file",
                objects.len(),
                access.objects.len()
            );
        }

        // Bind tier grants: each agent's tier name -> level.
        let mut agent_tier_level = BTreeMap::new();
        for (agent, tier) in &access.agent_tiers {
            let level = *access
                .tier_levels
                .get(tier)
                .with_context(|| format!("agent {agent} has unknown tier {tier:?}"))?;
            agent_tier_level.insert(agent.clone(), level);
        }

        Ok(EstateModel {
            objects,
            agent_tier_level,
            tier_levels: access.tier_levels,
        })
    }

    /// The estate agents (the ONLY principals with estate authority).
    pub fn is_estate_agent(&self, principal: &str) -> bool {
        self.agent_tier_level.contains_key(principal)
    }

    /// THE estate authority rule: a principal may read a resource of the
    /// given sensitivity IFF it is an estate agent AND the resource's
    /// sensitivity level is at or below the agent's tier level. Everyone
    /// else — and any unknown sensitivity — is denied (fail-closed). This
    /// single rule governs BOTH sources.
    pub fn can_read(&self, principal: &str, sensitivity: &str) -> bool {
        let Some(&tier) = self.agent_tier_level.get(principal) else {
            return false;
        };
        match self.tier_levels.get(sensitivity) {
            Some(&level) => level <= tier,
            None => false,
        }
    }

    /// A second-source object by doc id (`s3/...`), if it exists.
    pub fn object(&self, doc_id: &str) -> Option<&EstateObject> {
        self.objects.get(doc_id)
    }

    /// Every second-source object, doc-id order.
    pub fn objects(&self) -> impl Iterator<Item = &EstateObject> {
        self.objects.values()
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    pub fn estate_agents(&self) -> impl Iterator<Item = &String> {
        self.agent_tier_level.keys()
    }
}

/// One estate retrieval candidate (borrowed from the index).
pub struct EstateCandidate<'a> {
    pub doc_id: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub source: &'a str,
}

// ---------------------------------------------------------------------------
// S5a: the estate retrieval index — kill the O(corpus) cliff
// ---------------------------------------------------------------------------

/// One indexed document (owned; the index is built once at ingest and read
/// on the request path). `tokens` is the body's token SET, computed ONCE at
/// build so query time never re-tokenizes a body.
struct IndexedDoc {
    title: String,
    body: String,
    sensitivity: String,
    source: String,
    tokens: std::collections::BTreeSet<String>,
}

/// The estate's inverted index, built at INGEST over BOTH sources (primary
/// docs + second-source objects). Request-time retrieval reads the index
/// only — it never re-tokenizes a corpus body, never touches disk.
///
/// EB-5 IS PRESERVED INSIDE QUERY CONSTRUCTION. The authority predicate is
/// evaluated DURING candidate formation from the posting lists — the
/// inverted-index analogue of tantivy's `TermSetQuery` MUST clause: a
/// document is a candidate iff it matches the query AND the caller is
/// authorized for it. An out-of-scope document is never admitted to the
/// candidate set — never scored, never ranked, never returned. This is NOT
/// a post-filter of a wider result set (which would be an EB-5 violation);
/// scope is a membership condition on candidacy itself.
///
/// The build is deterministic for a given estate (same docs → same
/// candidacy), so a fixed corpus yields fixed candidate sets. Ranking is a
/// deterministic query-token-overlap score (overlap desc, doc-id asc).
pub struct EstateIndex {
    docs: BTreeMap<String, IndexedDoc>,
    /// token -> the doc ids whose body contains it (sorted, deduplicated).
    postings: BTreeMap<String, Vec<String>>,
    doc_count: usize,
}

impl EstateIndex {
    /// Build the index from an iterator of
    /// `(doc_id, title, body, sensitivity, source)`. Tokenization happens
    /// exactly once per document, here.
    pub fn build<I>(entries: I) -> EstateIndex
    where
        I: IntoIterator<Item = (String, String, String, String, String)>,
    {
        let mut docs = BTreeMap::new();
        let mut postings: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (doc_id, title, body, sensitivity, source) in entries {
            let tokens: std::collections::BTreeSet<String> =
                retrieval::index::tokenize(&body).into_iter().collect();
            for token in &tokens {
                postings
                    .entry(token.clone())
                    .or_default()
                    .push(doc_id.clone());
            }
            docs.insert(
                doc_id,
                IndexedDoc {
                    title,
                    body,
                    sensitivity,
                    source,
                    tokens,
                },
            );
        }
        // Deterministic, deduplicated posting lists.
        for ids in postings.values_mut() {
            ids.sort();
            ids.dedup();
        }
        let doc_count = docs.len();
        EstateIndex {
            docs,
            postings,
            doc_count,
        }
    }

    pub fn doc_count(&self) -> usize {
        self.doc_count
    }

    /// Retrieve the top-k candidates matching `query`, for a caller whose
    /// authorization is the `authorized` predicate (`|sensitivity| -> bool`,
    /// the estate tier rule). Candidate formation walks ONLY the posting
    /// lists of the query's tokens (query-selective, not corpus-wide), and
    /// admits a document iff `authorized(sensitivity)` holds — scope inside
    /// candidate construction, never a post-filter.
    pub fn retrieve<'a>(
        &'a self,
        query: &str,
        top_k: usize,
        authorized: impl Fn(&str) -> bool,
    ) -> Vec<EstateCandidate<'a>> {
        let query_tokens: std::collections::BTreeSet<String> =
            retrieval::index::tokenize(query).into_iter().collect();
        if query_tokens.is_empty() {
            return Vec::new();
        }
        // The query-selective candidate set: the union of the posting lists
        // for the query's tokens. A document not containing any query token
        // is never considered.
        let mut candidate_ids: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for token in &query_tokens {
            if let Some(ids) = self.postings.get(token) {
                for id in ids {
                    candidate_ids.insert(id.as_str());
                }
            }
        }

        let mut scored: Vec<(usize, EstateCandidate<'a>)> = Vec::new();
        for doc_id in candidate_ids {
            let doc = &self.docs[doc_id];
            // Authority as a MUST clause on candidacy (EB-5): an
            // unauthorized document is never admitted.
            if !authorized(&doc.sensitivity) {
                continue;
            }
            let overlap = query_tokens.intersection(&doc.tokens).count();
            if overlap == 0 {
                continue;
            }
            scored.push((
                overlap,
                EstateCandidate {
                    doc_id,
                    title: &doc.title,
                    body: &doc.body,
                    source: &doc.source,
                },
            ));
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.doc_id.cmp(b.1.doc_id)));
        scored.truncate(top_k);
        scored.into_iter().map(|(_, candidate)| candidate).collect()
    }
}

/// Run a connector at INGEST TIME, refusing the whole source if any object
/// carries permission-shaped native_meta (fail-closed: a connector must
/// never be a back door for authority).
pub fn ingest(connector: &dyn SourceConnector) -> Result<Vec<RawObject>> {
    let objects = connector.enumerate()?;
    for object in &objects {
        for key in object.native_meta.keys() {
            if PERMISSION_SHAPED_META.contains(&key.to_ascii_lowercase().as_str()) {
                bail!(
                    "connector {:?} emitted permission-shaped native_meta {:?} on {:?}; \
                     refusing the source (authority lives in the access model, not the connector)",
                    connector.source_id(),
                    key,
                    object.native_key
                );
            }
        }
    }
    Ok(objects)
}

/// The content hash over the ingested bodies — MUST match the Python
/// generator: sha256 of `doc_id\0body\0` for every object, doc-id order.
fn content_hash(objects: &BTreeMap<String, EstateObject>) -> String {
    let mut preimage = Vec::new();
    for object in objects.values() {
        preimage.extend_from_slice(object.doc_id.as_bytes());
        preimage.push(0);
        preimage.extend_from_slice(object.body.as_bytes());
        preimage.push(0);
    }
    sha256_hex(&preimage)
}

/// The first markdown heading (`# ...`) as the object title.
fn first_heading(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| line.strip_prefix("# ").map(|h| h.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SmugglingConnector;
    impl SourceConnector for SmugglingConnector {
        fn source_id(&self) -> &str {
            "smuggler"
        }
        fn enumerate(&self) -> Result<Vec<RawObject>> {
            let mut native_meta = BTreeMap::new();
            native_meta.insert("mime".to_string(), "text/plain".to_string());
            // The attack: authority in native_meta.
            native_meta.insert("acl".to_string(), "everyone".to_string());
            Ok(vec![RawObject {
                native_key: "x".to_string(),
                bytes: b"hi".to_vec(),
                native_meta,
            }])
        }
    }

    #[test]
    fn ingest_refuses_permission_shaped_native_meta() {
        let result = ingest(&SmugglingConnector);
        assert!(
            result.is_err(),
            "a connector emitting authority must be refused at ingest"
        );
    }

    #[test]
    fn tier_rule_is_at_or_below_and_fail_closed() {
        let mut tier_levels = BTreeMap::new();
        for (name, level) in [
            ("public", 0u8),
            ("internal", 1),
            ("confidential", 2),
            ("restricted", 3),
        ] {
            tier_levels.insert(name.to_string(), level);
        }
        let mut agent_tier_level = BTreeMap::new();
        agent_tier_level.insert("agent_estate_confidential".to_string(), 2u8);
        agent_tier_level.insert("agent_estate_internal".to_string(), 1u8);
        let model = EstateModel {
            objects: BTreeMap::new(),
            agent_tier_level,
            tier_levels,
        };
        // Confidential agent: public/internal/confidential yes; restricted no.
        assert!(model.can_read("agent_estate_confidential", "confidential"));
        assert!(model.can_read("agent_estate_confidential", "internal"));
        assert!(!model.can_read("agent_estate_confidential", "restricted"));
        // Internal agent: confidential no.
        assert!(model.can_read("agent_estate_internal", "internal"));
        assert!(!model.can_read("agent_estate_internal", "confidential"));
        // A non-estate principal: denied everything (seam default).
        assert!(!model.can_read("p060", "public"));
        // Unknown sensitivity: fail-closed.
        assert!(!model.can_read("agent_estate_confidential", "cosmic"));
    }
}
