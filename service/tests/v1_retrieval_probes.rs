//! S1-2, proven over a probe corpus: every `/v1/retrieve` candidate is
//! inside the resolved principal's compiled scope, and NO out-of-scope
//! document leaks into any response — not as an id, not as a title, not as
//! snippet text — regardless of what the query asks for.
//!
//! Probe construction is deterministic and oracle-driven: for each agent,
//! 13 queries are distinctive phrases lifted from IN-scope documents and 12
//! from OUT-of-scope documents (25 per agent, 100 total). Each phrase is
//! anchored on its target document's RAREST corpus token (fewest host
//! documents; ties break lexicographically), so an in-scope phrase reliably
//! surfaces its target and an out-of-scope phrase names content the agent
//! must never see. Out-of-scope targets are additionally chosen so the rare
//! token's ENTIRE host-document set is disjoint from the agent's allowlist
//! — if that token appears anywhere in a response body, it is a leak by
//! construction, not a coincidence.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::jwt::{self, TokenSpec, TEST_AUDIENCE, TEST_TENANT};
use serde_json::{json, Value};
use service::agent::proposals::ProposalStore;
use service::agent_bridge::{AgentBridgeConfig, Bridge};
use service::{app, AppState};
use tower::ServiceExt;

const AGENTS: [(&str, &str); 4] = [
    ("agent_qa_drafter", "aaaa0001-5c1e-4a2b-9d3e-000000000a01"),
    (
        "agent_ops_concierge",
        "aaaa0002-5c1e-4a2b-9d3e-000000000a02",
    ),
    (
        "agent_finance_analyst",
        "aaaa0003-5c1e-4a2b-9d3e-000000000a03",
    ),
    ("agent_exec_brief", "aaaa0004-5c1e-4a2b-9d3e-000000000a04"),
];
const IN_SCOPE_PROBES: usize = 13;
const OUT_OF_SCOPE_PROBES: usize = 12;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("service crate sits in the repo root")
        .to_path_buf()
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// principal -> the oracle ALLOW set, straight from raw ground truth.
fn oracle_allows() -> BTreeMap<String, BTreeSet<String>> {
    let path = common::repo_fixtures_dir().join("ground_truth.jsonl");
    let text = fs::read_to_string(path).expect("ground truth");
    let wanted: BTreeSet<&str> = AGENTS.iter().map(|(agent, _)| *agent).collect();
    let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("row");
        let principal = row["principal_id"].as_str().expect("principal_id");
        if !wanted.contains(principal) {
            continue;
        }
        map.entry(principal.to_string()).or_default();
        if row["decision"] == "ALLOW" {
            map.get_mut(principal).expect("entry").insert(
                row["resource_id"]
                    .as_str()
                    .expect("resource_id")
                    .to_string(),
            );
        }
    }
    map
}

struct Corpus {
    /// doc id -> tokenized body.
    tokens_by_doc: BTreeMap<String, Vec<String>>,
    /// token -> the set of documents whose BODY contains it.
    docs_by_token: BTreeMap<String, BTreeSet<String>>,
    all_ids: BTreeSet<String>,
    /// Documents some newer version supersedes. The search excludes them
    /// from results BY DESIGN (`include_superseded: false` is the standing
    /// law), so they can never be probe targets — they can never be
    /// candidates.
    superseded: BTreeSet<String>,
}

fn corpus() -> Corpus {
    let path = common::repo_fixtures_dir().join("documents.json");
    let text = fs::read_to_string(path).expect("documents.json");
    let parsed: Value = serde_json::from_str(&text).expect("documents parse");
    let mut tokens_by_doc = BTreeMap::new();
    let mut docs_by_token: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut all_ids = BTreeSet::new();
    let mut superseded = BTreeSet::new();
    for doc in parsed["documents"].as_array().expect("documents array") {
        let id = doc["id"].as_str().expect("id").to_string();
        let body = doc["body"].as_str().expect("body");
        if let Some(older) = doc["supersedes"].as_str() {
            superseded.insert(older.to_string());
        }
        let tokens = retrieval::index::tokenize(body);
        for token in &tokens {
            docs_by_token
                .entry(token.clone())
                .or_default()
                .insert(id.clone());
        }
        all_ids.insert(id.clone());
        tokens_by_doc.insert(id, tokens);
    }
    Corpus {
        tokens_by_doc,
        docs_by_token,
        all_ids,
        superseded,
    }
}

/// The probe phrase for a target document: up to six consecutive body
/// tokens centred on the document's rarest corpus token. Returns the
/// phrase and the anchor token.
fn phrase_for(corpus: &Corpus, doc_id: &str) -> (String, String) {
    let tokens = &corpus.tokens_by_doc[doc_id];
    let anchor = tokens
        .iter()
        .min_by_key(|token| {
            (
                corpus.docs_by_token[token.as_str()].len(),
                token.as_str().to_string(),
            )
        })
        .expect("document has tokens")
        .clone();
    let position = tokens
        .iter()
        .position(|t| t == &anchor)
        .expect("anchor is from this body");
    let start = position.saturating_sub(2);
    let end = (start + 6).min(tokens.len());
    (tokens[start..end].join(" "), anchor)
}

async fn retrieve(router: &axum::Router, token: &str, query: &str) -> (StatusCode, String) {
    let body = serde_json::to_string(&json!({ "query": query, "top_k": 50 })).expect("json");
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/retrieve")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, String::from_utf8_lossy(&bytes).to_string())
}

fn candidate_ids(response_body: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(response_body).expect("response json");
    value["candidates"]
        .as_array()
        .expect("candidates array")
        .iter()
        .map(|c| c["doc_id"].as_str().expect("doc_id").to_string())
        .collect()
}

#[tokio::test]
async fn probe_corpus_proves_scope_candidacy_and_zero_leaks() {
    let allows = oracle_allows();
    let corpus = corpus();

    // The world: real fixtures, all four agents registered.
    let dir = scratch("v1-probes");
    let jwks_path = dir.join("jwks.json");
    fs::write(&jwks_path, &jwt::issuer().jwks_json).expect("write jwks");
    let agents_config: Vec<Value> = AGENTS
        .iter()
        .map(|(agent, oid)| json!({ "tid": TEST_TENANT, "oid": oid, "principal": agent }))
        .collect();
    let config: AgentBridgeConfig = serde_json::from_value(json!({
        "enabled": true,
        "tenant_id": TEST_TENANT,
        "audience": TEST_AUDIENCE,
        "jwks": { "file": jwks_path },
        "agents": agents_config,
    }))
    .expect("bridge config parses");
    let store = Arc::new(ProposalStore::open(&dir.join("state")).expect("store"));
    let state = AppState::build(
        &common::repo_fixtures_dir(),
        &repo_root().join("compiler").join("artifacts"),
        &repo_root().join("retrieval").join("idx"),
    )
    .expect("build state")
    .with_people()
    .expect("people layer")
    .with_proposals(store)
    .with_agent_bridge(Arc::new(
        Bridge::from_config(&config).expect("bridge builds"),
    ));
    let router = app(Arc::new(state));

    let mut probes_run = 0usize;
    let mut in_scope_hits = 0usize;
    let mut out_of_scope_total = 0usize;

    for (agent, oid) in AGENTS {
        let token = TokenSpec::autonomous(oid).sign();
        let allow_set = &allows[agent];
        let deny_pool: Vec<&String> = corpus
            .all_ids
            .iter()
            .filter(|id| !allow_set.contains(*id))
            .collect();

        // 13 in-scope targets, DISTINCTIVE-FIRST: documents whose anchor
        // token lives in at most 3 documents corpus-wide come first (their
        // phrase names them specifically — the strict assertion applies);
        // the remainder fill from the rest of the scope in id order.
        // Fixture fact: finance (61) and ops (19) have distinctive in-scope
        // docs; qa_drafter and exec_brief have ZERO — their entire 60-doc
        // scope is shared template text, so no query can name one specific
        // document there, and the relaxed assertion (the query surfaces
        // in-scope content its anchor names) is the strongest true claim.
        let mut in_scope_targets: Vec<&String> = allow_set
            .iter()
            .filter(|id| !corpus.superseded.contains(*id))
            .filter(|id| {
                let (_, anchor) = phrase_for(&corpus, id);
                corpus.docs_by_token[&anchor].len() <= 3
            })
            .take(IN_SCOPE_PROBES)
            .collect();
        for id in allow_set.iter() {
            if in_scope_targets.len() == IN_SCOPE_PROBES {
                break;
            }
            if !corpus.superseded.contains(id) && !in_scope_targets.contains(&id) {
                in_scope_targets.push(id);
            }
        }
        assert_eq!(in_scope_targets.len(), IN_SCOPE_PROBES);

        // 12 out-of-scope targets: distinctiveness bound, PLUS the anchor
        // token's ENTIRE host set disjoint from the allowlist, PLUS the
        // anchor must be an alphabetic word of ≥5 chars — a numeric or
        // short anchor (a "22") collides with the candidates' `rank`
        // field values ("rank":22) and JSON scaffolding in the response
        // and cannot detect anything. (Historical note: the collision was
        // first hit when this field was briefly named `score`; the anchor
        // constraint is field-name-independent and stays.) With these
        // constraints, the anchor appearing ANYWHERE in a response body
        // is a leak by construction.
        let mut out_of_scope_targets: Vec<(&String, String, String)> = Vec::new();
        for id in &deny_pool {
            if corpus.superseded.contains(*id) {
                continue;
            }
            let (phrase, anchor) = phrase_for(&corpus, id);
            let hosts = &corpus.docs_by_token[&anchor];
            let anchor_is_detector =
                anchor.chars().count() >= 5 && anchor.chars().all(|c| c.is_ascii_alphabetic());
            if anchor_is_detector && hosts.len() <= 3 && hosts.is_disjoint(allow_set) {
                out_of_scope_targets.push((id, phrase, anchor));
                if out_of_scope_targets.len() == OUT_OF_SCOPE_PROBES {
                    break;
                }
            }
        }
        assert_eq!(
            out_of_scope_targets.len(),
            OUT_OF_SCOPE_PROBES,
            "{agent}: the corpus offers enough disjoint-anchor out-of-scope targets"
        );

        // (b) every in-scope-targeted phrase surfaces the in-scope content
        // its anchor names — the TARGET ITSELF when the anchor is
        // distinctive (≤3 host docs), else at least one in-scope host of
        // the anchor (per-doc discrimination is textually impossible among
        // identical template docs). (a) all candidates stay inside the
        // compiled scope, always.
        for target in in_scope_targets {
            let (phrase, anchor) = phrase_for(&corpus, target);
            let (status, body) = retrieve(&router, &token, &phrase).await;
            assert_eq!(status, StatusCode::OK, "{agent} probe {phrase:?}");
            let ids = candidate_ids(&body);
            for id in &ids {
                assert!(
                    allow_set.contains(id),
                    "{agent}: candidate {id} is OUTSIDE the compiled scope (query {phrase:?})"
                );
            }
            let hosts = &corpus.docs_by_token[&anchor];
            if hosts.len() <= 3 {
                assert!(
                    ids.iter().any(|id| id == target),
                    "{agent}: distinctive phrase {phrase:?} must surface its target {target}"
                );
            } else {
                let expected: BTreeSet<&String> = hosts.intersection(allow_set).collect();
                assert!(
                    ids.iter().any(|id| expected.contains(id)),
                    "{agent}: phrase {phrase:?} must surface in-scope content its \
                     anchor {anchor:?} names"
                );
            }
            probes_run += 1;
            in_scope_hits += 1;
        }

        // (c) out-of-scope-targeted phrases: target absent, anchor token
        // absent from the ENTIRE response body; (a) still holds.
        for (target, phrase, anchor) in out_of_scope_targets {
            let (status, body) = retrieve(&router, &token, &phrase).await;
            assert_eq!(status, StatusCode::OK, "{agent} probe {phrase:?}");
            let ids = candidate_ids(&body);
            for id in &ids {
                assert!(
                    allow_set.contains(id),
                    "{agent}: candidate {id} is OUTSIDE the compiled scope (query {phrase:?})"
                );
                out_of_scope_total += usize::from(!allow_set.contains(id));
            }
            assert!(
                !ids.iter().any(|id| id == target),
                "{agent}: out-of-scope target {target} appeared in candidates"
            );
            assert!(
                !body.to_lowercase().contains(&anchor.to_lowercase()),
                "{agent}: out-of-scope content leaked — anchor token {anchor:?} \
                 (host docs all outside scope) appears in the response body"
            );
            probes_run += 1;
        }
    }

    // (d) the all-agents sweep: zero out-of-scope doc ids anywhere.
    assert_eq!(
        out_of_scope_total, 0,
        "zero out-of-scope candidates across the probe corpus"
    );
    assert_eq!(
        probes_run,
        AGENTS.len() * (IN_SCOPE_PROBES + OUT_OF_SCOPE_PROBES)
    );
    println!(
        "S1 probe corpus: {probes_run} probes ({} agents x {} queries), \
         {in_scope_hits} in-scope targets surfaced, 0 out-of-scope candidates, 0 anchor leaks",
        AGENTS.len(),
        IN_SCOPE_PROBES + OUT_OF_SCOPE_PROBES
    );

    // C8: a gibberish query is an EMPTY 200, not an error.
    let token = TokenSpec::autonomous(AGENTS[0].1).sign();
    let (status, body) = retrieve(&router, &token, "zzxqv wplk vrtn qqjx").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        candidate_ids(&body).len(),
        0,
        "gibberish -> empty candidates"
    );
}
