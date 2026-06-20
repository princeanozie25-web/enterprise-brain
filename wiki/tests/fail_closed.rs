//! DoD #4 — fail closed.
//!
//! Inject a case where derived structure implies access the authorization model
//! does NOT grant (person `p011` authors `d0001`, but the model grants them
//! only an unrelated document). Assert that:
//!   * the discrepancy is flagged and surfaced, and
//!   * the granted set shown is EXACTLY the model's answer — the implied
//!     document is absent, so access is verifiably not widened.
//!
//! The oracle is injected in-memory, so the real fixtures and artifacts are
//! never touched (immutable inputs hold).

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::fixtures_dir;

use wiki::authz::GrantOracle;
use wiki::render;
use wiki::{derive_all, Sources};

/// A read-only oracle with a hand-set allow map. Like every `GrantOracle`, it
/// can answer questions but cannot be made to widen anything.
struct FakeOracle {
    allow: BTreeMap<String, BTreeSet<String>>,
}

impl GrantOracle for FakeOracle {
    fn why_allowed(&self, principal: &str, document: &str) -> Option<Vec<String>> {
        self.allow
            .get(principal)
            .filter(|s| s.contains(document))
            .map(|_| vec!["fake:rule".to_string()])
    }
    fn allowed_documents(&self, principal: &str) -> Vec<String> {
        self.allow
            .get(principal)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }
    fn denied_count(&self, principal: &str) -> Option<usize> {
        self.allow.get(principal).map(|_| 0)
    }
    fn known_principal(&self, principal: &str) -> bool {
        self.allow.contains_key(principal)
    }
    fn is_superseded(&self, _principal: &str, _document: &str) -> bool {
        false
    }
    fn snapshot_version(&self) -> &str {
        "fake-snapshot"
    }
}

#[test]
fn derived_implies_ungranted_is_flagged_and_access_not_widened() {
    let sources = Sources::load(&fixtures_dir()).expect("load sources");

    // p011 authors d0001 (real fixture fact). The model grants p011 ONLY d0599.
    let granted_doc = "d0599";
    let implied_but_denied = "d0001";
    let mut allow = BTreeMap::new();
    allow.insert(
        "p011".to_string(),
        BTreeSet::from([granted_doc.to_string()]),
    );
    let oracle = FakeOracle { allow };

    let layer = derive_all(&sources, &oracle);
    let p011 = layer
        .people
        .iter()
        .find(|p| p.id == "p011")
        .expect("p011 page");

    // 1. The implied-but-ungranted document is FLAGGED (fail-closed), via the
    //    authorship basis, and the flag is provenance-anchored to documents.json.
    let flag = p011
        .discrepancies
        .iter()
        .find(|d| d.document_id == implied_but_denied)
        .expect("d0001 is flagged as derived-implies-ungranted");
    assert!(
        flag.bases.iter().any(|b| b == "authorship"),
        "flag names the authorship basis"
    );
    assert_eq!(flag.provenance.source, "fixtures/documents.json");
    assert_eq!(flag.provenance.record, implied_but_denied);

    // 2. The granted set shown is EXACTLY the model's answer — no widening.
    let ga = p011
        .governed_access
        .as_ref()
        .expect("p011 has governed access");
    let granted: BTreeSet<&str> = ga.sample.iter().map(|d| d.document_id.as_str()).collect();
    assert_eq!(ga.allowed_total, 1, "exactly the model's one grant");
    assert_eq!(
        granted,
        BTreeSet::from([granted_doc]),
        "granted set is verbatim from the oracle"
    );
    assert!(
        !granted.contains(implied_but_denied),
        "the implied document was NOT folded into the granted set (not widened)"
    );

    // 3. The flag is surfaced in the rendered page.
    let pages = render::render_layer(&layer);
    let rendered = pages
        .iter()
        .find(|p| p.relpath == "people/p011.md")
        .expect("rendered p011");
    assert!(rendered.markdown.contains("Fail-closed flags"));
    assert!(rendered.markdown.contains(implied_but_denied));
    assert!(rendered.markdown.contains("NOT** widened"));
}
