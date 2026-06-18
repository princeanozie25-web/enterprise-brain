//! Slice 5 — free-text principal-mention flagging (deterministic, fail-closed).
//!
//! Slices 1–2 flag a STRUCTURED `ABOUT: <principal>` association the model does
//! not grant. They do NOT catch a principal named only in free-text PROSE. This
//! module closes that gap: every admitted LLM-derived claim's prose is scanned —
//! deterministically, never with a model — for mentions of known principals, and
//! a mention of a principal the DERIVING SCOPE is not granted ABOUT is FLAGGED
//! and surfaced. This is the same fail-closed treatment the
//! structured path already gives: never silently kept, never widening access.
//! It only ADDS coverage to the existing flag — the granted/displayed set is
//! unchanged; the flag count goes up, the access never does.
//!
//! "Granted about P" (read-only, no new authz surface): a deriving scope S is
//! authorized about principal P iff
//!   * `S == P` — a scope is trivially authorized about its own principal
//!     (naming yourself in your own derived prose is not a third-party
//!     disclosure); OR
//!   * S is granted (its allowed set contains) at least one document whose
//!     governed `subject_id` is P — the corpus's explicit "document about this
//!     person" relation (the HR records).
//!
//! Authorship, or an incidental name buried in a granted document's body, does
//! NOT count: those are exactly the situations that leak an identity into prose,
//! which is what this flag is here to catch. The allowed set is the oracle's own
//! allows (`ScopeGate::allowed()` == `GrantOracle::allowed_documents`), so this
//! reads the compiled model only — it has no write path and adds no new authz
//! mechanism.
//!
//! DETECTION is deterministic roster matching, never an LLM:
//!   * a UNIQUE full display-name appearing contiguously in the prose resolves to
//!     that one principal;
//!   * a bare name token shared by several principals (a common first name), a
//!     full name borne by more than one principal, or a token colliding with a
//!     common word, is AMBIGUOUS — flagged, never resolved by guessing which
//!     principal is meant.
//!
//! LIMITS (honest, stated not hidden): detection is ROSTER-SURFACE-BOUNDED. It
//! flags canonical-surface mentions of KNOWN-ROSTER principals; it does NOT
//! recognize non-roster entities (an external party, a customer org, a nickname),
//! and apostrophe-elided surname variants (e.g. "O'Brien" written "OBrien") are
//! currently missed. A miss is the FLAG failing open, never the access gate: the
//! granted/displayed set is unchanged regardless, so a missed mention is an
//! un-surfaced disclosure, not a widened grant. Robust entity recognition and
//! apostrophe normalization are tracked follow-ups, not claimed here.

use std::collections::{BTreeMap, BTreeSet};

use crate::sources::Sources;

/// A flagged free-text principal mention. Surfaced alongside the claim; access
/// is never widened. Kept DISTINCT from a slice-1/2 structured `Discrepancy` so
/// the two coverages (structured-association vs free-text-mention) stay
/// separately countable and honest in the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionFlag {
    /// The scope whose derived prose named the principal.
    pub deriving_scope: String,
    /// The resolved principal id, or `None` when the mention is ambiguous.
    pub mentioned_id: Option<String>,
    /// The exact (lowercased) surface token/name matched in the prose.
    pub surface: String,
    /// Candidate ids: the single resolved id, or every principal an ambiguous
    /// token could refer to (≥2). Sorted, for determinism.
    pub candidates: Vec<String>,
    /// Whether the mention could NOT be resolved to a single principal.
    pub ambiguous: bool,
    /// A human cite of the in-scope source the flagged claim was derived from.
    pub cited_source: String,
    /// Human-readable reason.
    pub detail: String,
}

/// One detected principal mention in a piece of prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mention {
    /// Resolved to exactly one known principal.
    Resolved {
        principal_id: String,
        surface: String,
    },
    /// A name token present but not resolvable to a single principal — the
    /// fail-closed default handles this (it is flagged, never guessed).
    Ambiguous {
        surface: String,
        candidates: Vec<String>,
    },
}

impl Mention {
    fn sort_key(&self) -> String {
        match self {
            Mention::Resolved { principal_id, .. } => format!("0:{principal_id}"),
            Mention::Ambiguous { surface, .. } => format!("1:{surface}"),
        }
    }
}

/// Known principals, indexed for deterministic mention detection. Built from the
/// roster (people.json) and the corpus's subject relation (documents.json),
/// read-only.
#[derive(Debug, Clone)]
pub struct Roster {
    /// principal id -> display name.
    name_of: BTreeMap<String, String>,
    /// lowercased full name -> the principal ids bearing it (≥2 ⇒ ambiguous).
    full_to_ids: BTreeMap<String, BTreeSet<String>>,
    /// each principal's full-name token sequence (lowercased), `(tokens, id)`,
    /// sorted longest-first then by id, for deterministic contiguous matching.
    full_tokens: Vec<(Vec<String>, String)>,
    /// lowercased single name token -> principal ids whose name contains it.
    token_to_ids: BTreeMap<String, BTreeSet<String>>,
    /// principal id -> sorted document ids whose `subject_id` is that principal.
    subject_docs: BTreeMap<String, Vec<String>>,
}

impl Roster {
    /// Builds the index from the read-only sources. Deterministic.
    pub fn from_sources(sources: &Sources) -> Roster {
        let mut name_of = BTreeMap::new();
        let mut full_to_ids: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut token_to_ids: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut full_tokens: Vec<(Vec<String>, String)> = Vec::new();

        for p in &sources.people.people {
            name_of.insert(p.id.clone(), p.display_name.clone());
            let toks = name_tokens(&p.display_name);
            if toks.is_empty() {
                continue;
            }
            full_to_ids
                .entry(toks.join(" "))
                .or_default()
                .insert(p.id.clone());
            for t in &toks {
                token_to_ids
                    .entry(t.clone())
                    .or_default()
                    .insert(p.id.clone());
            }
            full_tokens.push((toks, p.id.clone()));
        }
        // Longest full names first so a 3-token name is consumed before any
        // 2-token subsequence of it; ties broken by id for a stable order.
        full_tokens.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.1.cmp(&b.1)));

        let mut subject_docs: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for d in &sources.documents.documents {
            if let Some(subject) = &d.subject_id {
                if !subject.is_empty() {
                    subject_docs
                        .entry(subject.clone())
                        .or_default()
                        .push(d.id.clone());
                }
            }
        }
        for docs in subject_docs.values_mut() {
            docs.sort();
            docs.dedup();
        }

        Roster {
            name_of,
            full_to_ids,
            full_tokens,
            token_to_ids,
            subject_docs,
        }
    }

    /// The display name of a principal, if known.
    pub fn display_name(&self, id: &str) -> Option<&str> {
        self.name_of.get(id).map(String::as_str)
    }

    /// FAIL-CLOSED read-only check: is the deriving scope authorized about
    /// principal `p`? True iff the scope is `p` itself, or the scope's `allowed`
    /// set contains a document whose governed subject is `p`. Otherwise false —
    /// a mention of `p` is then a disclosure the scope has no granted basis for.
    pub fn scope_granted_about(&self, scope_id: &str, p: &str, allowed: &BTreeSet<String>) -> bool {
        if scope_id == p {
            return true;
        }
        self.subject_docs
            .get(p)
            .is_some_and(|docs| docs.iter().any(|d| allowed.contains(d)))
    }

    /// Detects mentions of known principals in `text`. Deterministic: a unique
    /// full name resolves; a shared/ambiguous/common token does not.
    pub fn detect(&self, text: &str) -> Vec<Mention> {
        let toks = name_tokens(text);
        if toks.is_empty() {
            return Vec::new();
        }
        let n = toks.len();
        let mut consumed = vec![false; n];
        // id -> the surface it was resolved by (its display name).
        let mut resolved: BTreeMap<String, String> = BTreeMap::new();

        // 1. Contiguous full-name matches, longest first. Only a UNIQUELY-borne
        //    full name resolves; a full name shared by ≥2 principals is left for
        //    the ambiguous token pass below.
        for (ntoks, id) in &self.full_tokens {
            let k = ntoks.len();
            if k == 0 || k > n {
                continue;
            }
            let full = ntoks.join(" ");
            if self.full_to_ids.get(&full).is_none_or(|b| b.len() != 1) {
                continue;
            }
            let mut i = 0;
            while i + k <= n {
                if (0..k).all(|j| !consumed[i + j] && toks[i + j] == ntoks[j]) {
                    for slot in consumed.iter_mut().skip(i).take(k) {
                        *slot = true;
                    }
                    resolved
                        .entry(id.clone())
                        .or_insert_with(|| self.name_of.get(id).cloned().unwrap_or(full.clone()));
                    i += k;
                } else {
                    i += 1;
                }
            }
        }

        // 2. Leftover single tokens that are roster name tokens. A token borne by
        //    exactly one principal and not a common word resolves to that
        //    principal; anything else (shared token, or common-word collision) is
        //    ambiguous and flagged, never guessed.
        let mut ambiguous: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (i, t) in toks.iter().enumerate() {
            if consumed[i] {
                continue;
            }
            let Some(ids) = self.token_to_ids.get(t) else {
                continue;
            };
            if ids.len() == 1 && !is_common_word(t) {
                let id = ids.iter().next().expect("len == 1").clone();
                // Surface is the literal token matched in the prose (a single
                // name token here), per the field's documented meaning.
                resolved.entry(id).or_insert_with(|| t.clone());
            } else {
                ambiguous
                    .entry(t.clone())
                    .or_default()
                    .extend(ids.iter().cloned());
            }
        }

        let mut out: Vec<Mention> = resolved
            .into_iter()
            .map(|(principal_id, surface)| Mention::Resolved {
                principal_id,
                surface,
            })
            .collect();
        for (surface, ids) in ambiguous {
            out.push(Mention::Ambiguous {
                surface,
                candidates: ids.into_iter().collect(),
            });
        }
        out.sort_by_key(|m| m.sort_key());
        out
    }

    /// Scans one admitted claim's `prose` and returns a flag for every principal
    /// mention the deriving scope is not granted about, plus every ambiguous
    /// mention (fail-closed). `cited_source` is a human cite of the in-scope
    /// source the claim was derived from. Never suppresses a claim — flags only.
    pub fn flag_prose(
        &self,
        deriving_scope: &str,
        allowed: &BTreeSet<String>,
        prose: &str,
        cited_source: &str,
    ) -> Vec<MentionFlag> {
        let mut out = Vec::new();
        for m in self.detect(prose) {
            match m {
                Mention::Resolved {
                    principal_id,
                    surface,
                } => {
                    if self.scope_granted_about(deriving_scope, &principal_id, allowed) {
                        continue; // authorized about this principal — not a leak.
                    }
                    let name = self
                        .display_name(&principal_id)
                        .map(str::to_string)
                        .unwrap_or_else(|| surface.clone());
                    let detail = format!(
                        "Derived prose in scope {deriving_scope} names principal {principal_id} \
                         ({name}), but the scope is not granted about that principal (no governed \
                         document about them is in scope). Flagged, not reconciled; access NOT widened."
                    );
                    out.push(MentionFlag {
                        deriving_scope: deriving_scope.to_string(),
                        mentioned_id: Some(principal_id.clone()),
                        surface,
                        candidates: vec![principal_id],
                        ambiguous: false,
                        cited_source: cited_source.to_string(),
                        detail,
                    });
                }
                Mention::Ambiguous {
                    surface,
                    candidates,
                } => {
                    out.push(MentionFlag {
                        deriving_scope: deriving_scope.to_string(),
                        mentioned_id: None,
                        surface: surface.clone(),
                        candidates: candidates.clone(),
                        ambiguous: true,
                        cited_source: cited_source.to_string(),
                        detail: format!(
                            "Derived prose in scope {deriving_scope} contains an ambiguous principal \
                             token \"{surface}\" (matches {} principal(s)); identity cannot be \
                             established, so it is flagged fail-closed rather than resolved by guessing.",
                            candidates.len()
                        ),
                    });
                }
            }
        }
        out
    }
}

/// Lowercased alphanumeric name tokens of length ≥ 2 (drops initials and
/// punctuation). The same tokenizer is applied to roster names and to prose, so
/// matching is symmetric and deterministic.
fn name_tokens(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 2)
        .map(str::to_lowercase)
        .collect()
}

/// A small, conservative set of name tokens that are also common English words.
/// A bare match on one of these is treated as ambiguous (flagged), never used to
/// resolve identity on its own — only a full-name match can resolve such a
/// principal. Incompleteness here is safe: it can only push a mention toward
/// flagging, never toward a silent pass.
fn is_common_word(token: &str) -> bool {
    const COMMON: &[&str] = &[
        "lee", "may", "will", "mark", "art", "grace", "rose", "ray", "dawn", "jade", "summer",
        "june", "ivy", "hope", "faith", "joy", "reed", "drew", "frank", "earl", "miles", "penny",
        "bill", "jack", "an", "the", "and", "for",
    ];
    COMMON.contains(&token)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster() -> Roster {
        // A tiny hand-built roster exercising the three detection regimes:
        // unique full name, shared first name, and a common-word surname.
        Roster {
            name_of: [
                ("p1", "Hassan Walsh"),
                ("p2", "Hiroshi Walsh"),
                ("p3", "Samir Nakamura"),
                ("p4", "Samir Mwangi"),
                ("p5", "Zara Lee"),
            ]
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect(),
            full_to_ids: BTreeMap::new(),
            full_tokens: Vec::new(),
            token_to_ids: BTreeMap::new(),
            subject_docs: BTreeMap::new(),
        }
        .rebuilt()
    }

    impl Roster {
        // Recompute the derived indices from `name_of` (test convenience).
        fn rebuilt(self) -> Roster {
            let mut full_to_ids: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            let mut token_to_ids: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            let mut full_tokens = Vec::new();
            for (id, name) in &self.name_of {
                let toks = name_tokens(name);
                full_to_ids
                    .entry(toks.join(" "))
                    .or_default()
                    .insert(id.clone());
                for t in &toks {
                    token_to_ids
                        .entry(t.clone())
                        .or_default()
                        .insert(id.clone());
                }
                full_tokens.push((toks, id.clone()));
            }
            full_tokens.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.1.cmp(&b.1)));
            Roster {
                full_to_ids,
                full_tokens,
                token_to_ids,
                ..self
            }
        }
    }

    #[test]
    fn unique_full_name_resolves() {
        let r = roster();
        let m = r.detect("the review names Hassan Walsh as subject");
        assert_eq!(
            m,
            vec![Mention::Resolved {
                principal_id: "p1".into(),
                surface: "Hassan Walsh".into()
            }]
        );
    }

    #[test]
    fn shared_first_name_is_ambiguous() {
        let r = roster();
        let m = r.detect("approved by Samir last week");
        assert_eq!(m.len(), 1);
        match &m[0] {
            Mention::Ambiguous {
                surface,
                candidates,
            } => {
                assert_eq!(surface, "samir");
                assert_eq!(candidates, &vec!["p3".to_string(), "p4".to_string()]);
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn shared_surname_alone_is_ambiguous_but_full_name_resolves() {
        let r = roster();
        // "Walsh" alone is shared (p1, p2) -> ambiguous.
        assert!(matches!(
            r.detect("signed off by Walsh").as_slice(),
            [Mention::Ambiguous { .. }]
        ));
        // The full name disambiguates and consumes the surname token.
        assert_eq!(
            r.detect("Hiroshi Walsh attended"),
            vec![Mention::Resolved {
                principal_id: "p2".into(),
                surface: "Hiroshi Walsh".into()
            }]
        );
    }

    #[test]
    fn common_word_surname_alone_does_not_resolve() {
        let r = roster();
        // "lee" is a common word: a bare occurrence is ambiguous, never resolved
        // to p5 by itself (the full name "Zara Lee" would resolve it).
        assert!(matches!(
            r.detect("on the lee side of the building").as_slice(),
            [Mention::Ambiguous { .. }]
        ));
        assert_eq!(
            r.detect("Zara Lee signed"),
            vec![Mention::Resolved {
                principal_id: "p5".into(),
                surface: "Zara Lee".into()
            }]
        );
    }

    #[test]
    fn detection_is_deterministic_across_reruns() {
        let r = roster();
        let text = "Hassan Walsh and Samir met; Walsh and Zara Lee too";
        let a = r.detect(text);
        let b = r.detect(text);
        assert_eq!(a, b);
    }

    #[test]
    fn granted_about_is_reflexive_and_subject_doc_gated() {
        let mut r = roster();
        r.subject_docs.insert("p1".into(), vec!["d0091".into()]);
        let allowed_with: BTreeSet<String> = ["d0091".to_string()].into_iter().collect();
        let allowed_without: BTreeSet<String> = ["d0500".to_string()].into_iter().collect();
        // Scope granted the HR doc about p1 -> granted about p1.
        assert!(r.scope_granted_about("p9", "p1", &allowed_with));
        // Scope without it -> not granted about p1.
        assert!(!r.scope_granted_about("p9", "p1", &allowed_without));
        // Reflexive: a scope is always granted about its own principal.
        assert!(r.scope_granted_about("p1", "p1", &allowed_without));
    }
}
