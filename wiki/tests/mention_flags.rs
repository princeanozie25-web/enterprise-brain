//! Slice 5 — free-text principal-mention flagging, fail-closed and additive.
//!
//! The structured `ABOUT:` flag (slices 1–2) catches a structured association
//! the model does not grant. It does NOT catch a principal NAMED only in
//! free-text prose. These tests prove slice 5 closes that limit: a derived
//! claim whose prose names a principal the deriving scope is not granted ABOUT
//! is flagged and surfaced — the same fail-closed treatment, never silently
//! kept, never widening access — and that it composes across the compounding
//! path, stays deterministic, and leaves the firewall and the authz model
//! untouched.
//!
//! Scopes (real compiled grants): p060 (Felix Osei) is granted ZERO HR/subject
//! documents -> granted about NOBODY but itself; p088 (Tomas Reyes) is granted
//! all 30 HR records -> granted about their subjects (incl. p008 Hassan Walsh).
//! Detection runs with the slice-2 fakes (no Ollama, no tantivy), so the GATE,
//! not a live model, is under test.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use common::{
    compile_artifacts, fixtures_dir, hash_tree, scratch, verbatim_prefix, FakeVerifier,
    FixedSelector, RecordingSynthesizer,
};

use wiki::authz::{AuthzView, GrantOracle};
use wiki::compound::{compound_answer, CompoundStore};
use wiki::scope::{ScopeContext, ScopeGate};
use wiki::scoped::{derive_scope, ScopedLayer, Topic};
use wiki::synth::RawClaim;
use wiki::Sources;

const BOARD: &str = "p060"; // granted 0 HR docs -> granted about nobody.
const HRADMIN: &str = "p088"; // granted all 30 HR records -> granted about their subjects.
const HASSAN: &str = "p008"; // Hassan Walsh — subject of HR record d0091.
const AISHA_OBRIEN: &str = "p105"; // "Aisha O'Brien" — unique full name; no HR record, so no scope is granted-about her.

/// A synthesizer that emits ONE claim whose prose is `prose`, citing the first
/// in-scope source with a verbatim quote (so grounding admits it). The prose is
/// model output — independent of the cited body — which is exactly how a free-
/// text principal mention slips in.
fn prose_synth(prose: &'static str) -> RecordingSynthesizer {
    RecordingSynthesizer::new("fake-model", move |s| {
        if s.is_empty() {
            return vec![];
        }
        vec![RawClaim {
            text: prose.to_string(),
            cited_doc_id: s[0].doc_id.clone(),
            quote: verbatim_prefix(&s[0].text, 48),
            about_principal: None,
        }]
    })
}

/// Derives one scope over its first few in-scope docs with `synth`.
fn derive_with(
    artifacts: &Path,
    sources: &Sources,
    authz: &AuthzView,
    pid: &str,
    synth: &RecordingSynthesizer,
) -> ScopedLayer {
    let gate = ScopeGate::load(artifacts, pid).expect("load scope");
    let head: Vec<String> = gate.allowed().iter().take(4).cloned().collect();
    let ctx = ScopeContext::build(gate, sources);
    let selector = FixedSelector { ids: head };
    let verifier = FakeVerifier::always();
    let topics = vec![Topic {
        label: "t".into(),
        query: "t".into(),
    }];
    derive_scope(sources, &ctx, &topics, &selector, synth, &verifier, authz).expect("derive scope")
}

// Each test gets its OWN scratch dir (by `name`); a shared name would race
// under parallel execution, since `scratch` resets the directory on entry.
fn setup(name: &str) -> (std::path::PathBuf, Sources, AuthzView) {
    let artifacts = scratch(name);
    compile_artifacts(&artifacts);
    let sources = Sources::load(&fixtures_dir()).unwrap();
    let authz = AuthzView::load(&artifacts).unwrap();
    (artifacts, sources, authz)
}

/// DoD #1 (CENTERPIECE): prose that names a principal the deriving scope is not
/// granted about is flagged and surfaced — the claim is still admitted (not
/// silently kept), with a free-text mention flag beside it.
#[test]
fn free_text_mention_of_ungranted_principal_is_flagged() {
    let (artifacts, sources, authz) = setup("mf_centerpiece");
    // p060 is granted about nobody; naming Hassan Walsh in its prose is a leak.
    let synth = prose_synth("The board minutes thank Hassan Walsh for the report.");
    let layer = derive_with(&artifacts, &sources, &authz, BOARD, &synth);

    // The claim is still admitted — slice 5 flags, it does not suppress.
    assert_eq!(layer.claims.len(), 1, "the prose claim is still admitted");
    // …and a free-text mention flag is surfaced, resolved to Hassan Walsh.
    let flag = layer
        .mention_flags
        .iter()
        .find(|m| m.mentioned_id.as_deref() == Some(HASSAN))
        .expect("Hassan Walsh mention is flagged");
    assert!(!flag.ambiguous, "a unique full name resolves");
    assert_eq!(flag.deriving_scope, BOARD);
    assert!(
        flag.detail.contains("not granted about"),
        "the flag explains the fail-closed reason: {}",
        flag.detail
    );
}

/// DoD #2: a mention of a principal the scope IS granted about is NOT flagged on
/// that basis (no false positive on authorized mentions).
#[test]
fn authorized_mention_is_not_flagged() {
    let (artifacts, sources, authz) = setup("mf_authorized");
    // p088 holds the HR record about Hassan Walsh -> granted about p008.
    let synth = prose_synth("Hassan Walsh completed the scheduled salary review.");
    let layer = derive_with(&artifacts, &sources, &authz, HRADMIN, &synth);

    assert_eq!(layer.claims.len(), 1, "the prose claim is admitted");
    assert!(
        layer
            .mention_flags
            .iter()
            .all(|m| m.mentioned_id.as_deref() != Some(HASSAN)),
        "an authorized principal mention is not flagged (no false positive)"
    );
    // The full name resolved cleanly, so there is no flag at all here.
    assert!(
        layer.mention_flags.is_empty(),
        "no spurious flags on an authorized, unambiguous mention: {:?}",
        layer.mention_flags
    );
}

/// DoD #3: a genuinely ambiguous name match is flagged, not passed by a guess.
#[test]
fn ambiguous_mention_fails_closed() {
    let (artifacts, sources, authz) = setup("mf_ambiguous");
    // "Samir" is borne by several principals — identity cannot be established.
    let synth = prose_synth("Samir confirmed the figures during the call.");
    let layer = derive_with(&artifacts, &sources, &authz, BOARD, &synth);

    let amb = layer
        .mention_flags
        .iter()
        .find(|m| m.ambiguous && m.surface == "samir")
        .expect("the ambiguous token is flagged");
    assert!(amb.mentioned_id.is_none(), "no identity is guessed");
    assert!(
        amb.candidates.len() >= 2,
        "an ambiguous token lists its candidates: {:?}",
        amb.candidates
    );
}

/// DoD #4: detection is deterministic — identical results across reruns, and no
/// model is in the detection path (the fakes prove that structurally).
#[test]
fn detection_is_deterministic() {
    let (artifacts, sources, authz) = setup("mf_deterministic");
    let a = derive_with(
        &artifacts,
        &sources,
        &authz,
        BOARD,
        &prose_synth("Hassan Walsh and Samir reviewed it with Zara Lee."),
    );
    let b = derive_with(
        &artifacts,
        &sources,
        &authz,
        BOARD,
        &prose_synth("Hassan Walsh and Samir reviewed it with Zara Lee."),
    );
    assert_eq!(
        a.mention_flags, b.mention_flags,
        "the same prose yields byte-identical mention flags across runs"
    );
}

/// DoD #5: additive, not relaxing. The structured `ABOUT:` flag still fires
/// exactly as before, INDEPENDENTLY of the new free-text mention flag, and the
/// admitted (granted/displayed) claim is kept either way.
#[test]
fn structured_flag_still_fires_and_is_independent() {
    let (artifacts, sources, authz) = setup("mf_structured");
    // A claim that BOTH names an ungranted principal in prose AND carries a
    // structured ABOUT for an ungranted principal: both coverages must fire,
    // separately, and the claim is still admitted.
    let synth = RecordingSynthesizer::new("fake-model", |s| {
        if s.is_empty() {
            return vec![];
        }
        vec![RawClaim {
            text: "Hassan Walsh is referenced in the board pack.".into(),
            cited_doc_id: s[0].doc_id.clone(),
            quote: verbatim_prefix(&s[0].text, 48),
            // structured association implicating Hassan Walsh on this doc.
            about_principal: Some(HASSAN.to_string()),
        }]
    });
    let layer = derive_with(&artifacts, &sources, &authz, BOARD, &synth);

    assert_eq!(
        layer.claims.len(),
        1,
        "the claim is admitted (kept), not dropped"
    );
    assert!(
        !layer.discrepancies.is_empty(),
        "the structured ABOUT flag still fires (slice 1/2 path intact)"
    );
    assert!(
        layer
            .mention_flags
            .iter()
            .any(|m| m.mentioned_id.as_deref() == Some(HASSAN)),
        "the free-text mention flag fires too (slice 5), separately"
    );
}

/// DoD #1/#5 (NO WIDENING, pinned): the admitted-claim / displayed set is
/// UNCHANGED by slice 5. The same scope and cited source admit the same claim
/// whether or not the prose names an ungranted principal — ONLY `mention_flags`
/// differs. The flag adds coverage; it never adds, drops, or alters a claim, and
/// never widens access.
#[test]
fn mention_flagging_does_not_change_the_admitted_claim_set() {
    let (artifacts, sources, authz) = setup("mf_no_widening");
    // Identical cited source + verbatim quote; the ONLY difference between the
    // two runs is whether the prose names an ungranted principal.
    let neutral = derive_with(
        &artifacts,
        &sources,
        &authz,
        BOARD,
        &prose_synth("The account operates on standard wholesale terms."),
    );
    let named = derive_with(
        &artifacts,
        &sources,
        &authz,
        BOARD,
        &prose_synth("Hassan Walsh recorded the standard wholesale terms."),
    );

    // The admitted (displayed) claim set is identical — same count, same cites.
    assert_eq!(
        neutral.claims.len(),
        named.claims.len(),
        "naming a principal in prose adds or drops NO admitted claim"
    );
    assert_eq!(
        neutral.cited_docs(),
        named.cited_docs(),
        "the cited (displayed) source set is byte-identical with or without a name"
    );
    // Only the mention-flag coverage differs: the additive property, made visible.
    assert!(
        neutral.mention_flags.is_empty(),
        "no principal named -> no mention flag"
    );
    assert!(
        named
            .mention_flags
            .iter()
            .any(|m| m.mentioned_id.as_deref() == Some(HASSAN)),
        "an ungranted principal named -> a flag, ADDED beside the unchanged claim"
    );
}

/// DoD #6: composes across the compounding path — a free-text mention in a
/// compounded answer is flagged the same way, and the grounding/closure gates
/// still admit the claim.
#[test]
fn mention_flag_composes_across_compounding() {
    let (artifacts, sources, authz) = setup("mf_compound");
    let allowed: BTreeSet<String> = authz.allowed_documents(BOARD).into_iter().collect();
    let gate = ScopeGate::load(&artifacts, BOARD).unwrap();
    let head: Vec<String> = gate.allowed().iter().take(3).cloned().collect();
    let ctx = ScopeContext::build(gate, &sources);
    let selector = FixedSelector { ids: head };
    let verifier = FakeVerifier::always();
    let synth = prose_synth("Hassan Walsh was noted in the compounded summary.");

    let mut allowed_of: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    allowed_of.insert(BOARD.to_string(), allowed);

    let page = compound_answer(
        &sources,
        &ctx,
        "q",
        "q",
        &selector,
        &synth,
        &verifier,
        &[],
        &allowed_of,
        0,
    )
    .unwrap();

    assert!(!page.claims.is_empty(), "the compounded claim is admitted");
    assert!(
        page.mention_flags
            .iter()
            .any(|m| m.mentioned_id.as_deref() == Some(HASSAN)),
        "the free-text mention is flagged on the compounded page too"
    );

    // The page still stores cleanly (closure/no-laundering gate intact).
    let mut store = CompoundStore::new(authz.snapshot_version());
    store
        .add(page, &allowed_of)
        .expect("page stores; closure gate intact");
    assert_eq!(store.len(), 1);
}

/// DoD #7: a mention-flagging run leaves the compiled authz model byte-identical
/// (the slice-5 detection has no authz write path; it only reads `allowed()`).
#[test]
fn mention_flagging_leaves_authz_artifacts_byte_identical() {
    let (artifacts, sources, authz) = setup("mf_byte_identical");
    let before = hash_tree(&artifacts);

    let synth = prose_synth("Samir and Hassan Walsh appear in the minutes.");
    let layer = derive_with(&artifacts, &sources, &authz, BOARD, &synth);
    assert!(
        !layer.mention_flags.is_empty(),
        "the run actually exercised mention flagging"
    );

    let after = hash_tree(&artifacts);
    assert_eq!(
        before, after,
        "free-text mention flagging must leave the compiled authz model byte-identical"
    );
}

/// P1-e (CENTERPIECE): an apostrophe-elided surname in admitted prose ("Aisha
/// OBrien") is now FLAGGED against the roster's "Aisha O'Brien" principal the
/// deriving scope is not granted about — and the ASCII `'` and unicode `’`
/// apostrophe forms flag identically. Before the fix the elided form produced NO
/// flag (the fail-open). The claim is still admitted alongside the flag (additive:
/// the flag-set grows, the granted/displayed set does not).
#[test]
fn apostrophe_elided_surname_is_flagged() {
    let (artifacts, sources, authz) = setup("mf_apostrophe");
    // p060 is granted about nobody, so naming Aisha O'Brien is a leak it must flag.
    let flags_aisha = |prose: &'static str| -> bool {
        let synth = prose_synth(prose);
        let layer = derive_with(&artifacts, &sources, &authz, BOARD, &synth);
        assert_eq!(layer.claims.len(), 1, "claim still admitted for {prose:?}");
        layer
            .mention_flags
            .iter()
            .any(|m| m.mentioned_id.as_deref() == Some(AISHA_OBRIEN))
    };
    assert!(
        flags_aisha("Aisha OBrien approved the figures."),
        "the apostrophe-ELIDED surname now flags (was a miss before the fix)"
    );
    assert!(
        flags_aisha("Aisha O'Brien approved the figures."),
        "the ASCII-apostrophe form flags identically"
    );
    assert!(
        flags_aisha("Aisha O\u{2019}Brien approved the figures."),
        "the unicode right-single-quote form flags identically"
    );
}
