//! Slice 3 — query-compounding WITHOUT laundering.
//!
//! An answer derived within a scope is written back as a SCOPE-STAMPED page that
//! may later source a future question — but only within a scope authorized for
//! everything the page was derived from. The load-bearing invariant:
//!
//!   NO LAUNDERING — for every compounded page stamped scope S, every raw
//!   document in its transitive provenance closure is within allowed(S).
//!
//! Eligibility is FAIL-CLOSED: a page stamped {S, snap_S} is usable for a
//! question in scope T (snapshot snap_T) ONLY if `snap_S == snap_T` AND
//! `allowed(S) ⊆ allowed(T)`. If the subset cannot be proven or the snapshot
//! differs, the page is excluded. There is NO toggle that relaxes this — the
//! legitimate cross-domain need is served later by an audited human grant that
//! ADDS a real authorization, never by relaxing the model's reach.
//!
//! WHERE EACH HALF IS ENFORCED (honest): the durable WRITE — `CompoundStore::add`
//! — enforces the closure ⊆ stamped-scope (no-laundering) and acyclicity; it does
//! NOT pin or check a snapshot. The snapshot-equality conjunct above is enforced
//! on the READ side, in eligibility (`is_eligible`/`eligible_for`): it gates
//! whether a stored page may SOURCE a later question, not whether it may be
//! stored. A single run uses one snapshot, so the two coincide there; across a
//! snapshot rotation the snapshot guarantee is the read-side eligibility filter,
//! not a write-side property of the store. (Pinning the store to one snapshot is
//! a tracked follow-up.)
//!
//! This module consults the authorization model only read-only (allowed sets
//! are computed from the compiled allowlists upstream and passed in); it has no
//! authz write path, exactly like slices 1-2.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};

use crate::ground::{ground_claim, Anchor, Grounded, SupportVerdict, Verifier};
use crate::mentions::{MentionFlag, Roster};
use crate::scope::{DocSelector, ScopeContext};
use crate::scoped::TOPIC_K;
use crate::sources::Sources;
use crate::synth::{SourceDoc, Synthesizer};

/// The asking scope plus the compiled-model snapshot a page was derived under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeStamp {
    pub principal_id: String,
    pub snapshot_hash: String,
}

/// Typed provenance for a compounded claim: a raw corpus document (with a span)
/// or a prior compounded page. A `CompoundClaim` cannot exist without one — the
/// slice-1 "claim requires provenance" discipline, typed for compounding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRef {
    RawDoc { doc_id: String, span: String },
    CompoundedPage { page_id: String },
}

/// One claim on a compounded page: bound to a typed source (for the closure)
/// AND grounded — anchored to a verbatim in-scope span + judge-confirmed support
/// (slice 4). For a CompoundedPage source the anchor is verbatim in the CITED
/// PAGE's own text (not a re-anchor at the raw layer); that page's claims were
/// themselves grounded to raw spans when it was created, so the chain bottoms
/// out at raw documents by the slice-3 no-laundering closure — a transitive
/// argument, not a fresh raw-doc re-anchor here.
#[derive(Debug, Clone)]
pub struct CompoundClaim {
    text: String,
    source: SourceRef,
    anchor: Anchor,
    support: SupportVerdict,
}

impl CompoundClaim {
    pub fn new(
        text: impl Into<String>,
        source: SourceRef,
        anchor: Anchor,
        support: SupportVerdict,
    ) -> Self {
        CompoundClaim {
            text: text.into(),
            source,
            anchor,
            support,
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn source(&self) -> &SourceRef {
        &self.source
    }
    pub fn anchor(&self) -> &Anchor {
        &self.anchor
    }
    pub fn support(&self) -> &SupportVerdict {
        &self.support
    }
}

/// A compounded page: a scope-stamped answer that may serve as a future source.
#[derive(Debug, Clone)]
pub struct CompoundedPage {
    pub page_id: String,
    /// Monotone ordinal — a page may cite ONLY strictly-earlier pages, so the
    /// citation graph is acyclic by construction (no wall clock is used).
    pub ordinal: u64,
    pub stamp: ScopeStamp,
    pub question: String,
    pub model: String,
    pub claims: Vec<CompoundClaim>,
    /// Cites the gate refused (out-of-scope / unknown source). Audit only.
    pub rejected: Vec<String>,
    /// Claims with no verbatim in-scope span — unfounded (slice-4 anchoring).
    pub refused_unfounded: Vec<String>,
    /// Claims anchored but judge-unconfirmed — withheld (slice-4, fail-closed).
    pub withheld: Vec<String>,
    /// Free-text principal mentions in admitted prose the scope is not granted
    /// about (slice 5). Additive, fail-closed, access never widened.
    pub mention_flags: Vec<MentionFlag>,
}

impl CompoundedPage {
    /// Page ids this page cites directly.
    pub fn cited_pages(&self) -> impl Iterator<Item = &str> {
        self.claims.iter().filter_map(|c| match c.source() {
            SourceRef::CompoundedPage { page_id } => Some(page_id.as_str()),
            SourceRef::RawDoc { .. } => None,
        })
    }
}

/// An append-only, acyclic store of compounded pages.
#[derive(Debug, Clone, Default)]
pub struct CompoundStore {
    pages: BTreeMap<String, CompoundedPage>,
    order: Vec<String>,
}

impl CompoundStore {
    pub fn new() -> CompoundStore {
        CompoundStore::default()
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// The ordinal the next added page should carry.
    pub fn next_ordinal(&self) -> u64 {
        self.order.len() as u64
    }

    pub fn get(&self, page_id: &str) -> Option<&CompoundedPage> {
        self.pages.get(page_id)
    }

    /// Pages in insertion (ordinal) order.
    pub fn pages(&self) -> impl Iterator<Item = &CompoundedPage> {
        self.order.iter().filter_map(|id| self.pages.get(id))
    }

    /// Adds a page, FAIL-CLOSED. The scope the no-laundering closure is checked
    /// against is derived from the page's OWN stamp (`allowed_of[stamp]`), not a
    /// free parameter — a caller cannot check against a mismatched set. Rejects
    /// (storing nothing) if:
    ///   * the stamped scope is unresolvable in `allowed_of`, or
    ///   * a cited page is unknown or not strictly earlier (acyclicity), or
    ///   * the page's transitive raw-doc closure includes any document outside
    ///     the stamped scope's allowed set (no laundering).
    pub fn add(
        &mut self,
        page: CompoundedPage,
        allowed_of: &BTreeMap<String, BTreeSet<String>>,
    ) -> Result<String> {
        if self.pages.contains_key(&page.page_id) {
            bail!("duplicate compounded page id {}", page.page_id);
        }
        // Bind the check to the page's OWN stamped scope (fail-closed if absent).
        let allowed_scope = allowed_of.get(&page.stamp.principal_id).with_context(|| {
            format!(
                "no allowed set for page {}'s stamped scope {}; refusing",
                page.page_id, page.stamp.principal_id
            )
        })?;
        // Acyclicity: every cited page must exist AND be strictly earlier.
        for cited in page.cited_pages() {
            let prior = self
                .pages
                .get(cited)
                .with_context(|| format!("page {} cites unknown page {cited}", page.page_id))?;
            if prior.ordinal >= page.ordinal {
                bail!(
                    "page {} cites non-earlier page {cited} (ordinal {} >= {}); refusing to break acyclicity",
                    page.page_id,
                    prior.ordinal,
                    page.ordinal
                );
            }
        }
        // No-laundering, computed BEFORE inserting (cited pages are already
        // present and strictly earlier, so their closures resolve): the new
        // page's transitive raw-doc closure must be entirely within
        // `allowed_scope`. Nothing is stored until every check passes — there is
        // no partially-inserted state and no rollback path to get wrong.
        let mut closure = BTreeSet::new();
        for claim in &page.claims {
            match claim.source() {
                SourceRef::RawDoc { doc_id, .. } => {
                    closure.insert(doc_id.clone());
                }
                SourceRef::CompoundedPage { page_id } => {
                    closure.extend(self.transitive_raw_docs(page_id)?);
                }
            }
        }
        let outside: Vec<String> = closure
            .iter()
            .filter(|d| !allowed_scope.contains(*d))
            .cloned()
            .collect();
        if !outside.is_empty() {
            bail!(
                "no-laundering violation: page {} transitive closure includes {} document(s) outside its scope: {}",
                page.page_id,
                outside.len(),
                outside.join(", ")
            );
        }

        let id = page.page_id.clone();
        self.pages.insert(id.clone(), page);
        self.order.push(id.clone());
        Ok(id)
    }

    /// The raw documents underlying `page_id`, following CompoundedPage refs to
    /// the bottom. Terminates (the graph is acyclic); a cycle — impossible via
    /// `add` — is detected and refused rather than looping.
    pub fn transitive_raw_docs(&self, page_id: &str) -> Result<BTreeSet<String>> {
        let mut raw = BTreeSet::new();
        let mut visiting = BTreeSet::new();
        self.collect(page_id, &mut raw, &mut visiting)?;
        Ok(raw)
    }

    fn collect(
        &self,
        page_id: &str,
        raw: &mut BTreeSet<String>,
        visiting: &mut BTreeSet<String>,
    ) -> Result<()> {
        if !visiting.insert(page_id.to_string()) {
            bail!("cycle detected in compounded provenance at page {page_id}");
        }
        let page = self
            .pages
            .get(page_id)
            .with_context(|| format!("provenance closure hit unknown page {page_id}"))?;
        for claim in &page.claims {
            match claim.source() {
                SourceRef::RawDoc { doc_id, .. } => {
                    raw.insert(doc_id.clone());
                }
                SourceRef::CompoundedPage { page_id: child } => {
                    self.collect(child, raw, visiting)?;
                }
            }
        }
        visiting.remove(page_id);
        Ok(())
    }

    /// Pages eligible as sources for a question in scope T. FAIL-CLOSED: a page
    /// is eligible ONLY if its snapshot matches `snap_t` AND its scope's allowed
    /// set is a subset of `allowed_t`. A scope whose allowed set cannot be
    /// resolved in `allowed_of`, or a differing snapshot, is excluded. There is
    /// deliberately NO parameter that relaxes this.
    pub fn eligible_for<'a>(
        &'a self,
        snap_t: &str,
        allowed_t: &BTreeSet<String>,
        allowed_of: &BTreeMap<String, BTreeSet<String>>,
    ) -> Vec<&'a CompoundedPage> {
        self.pages()
            .filter(|p| is_eligible(p, snap_t, allowed_t, allowed_of))
            .collect()
    }
}

/// The fail-closed eligibility predicate, shared by the store's `eligible_for`
/// and `compound_answer`'s defensive self-gate: a page is eligible for scope T
/// ONLY if its snapshot matches AND `allowed(page.scope) ⊆ allowed(T)`. An
/// unresolvable scope or a differing snapshot is excluded. There is no relaxing
/// path.
pub fn is_eligible(
    page: &CompoundedPage,
    snap_t: &str,
    allowed_t: &BTreeSet<String>,
    allowed_of: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    page.stamp.snapshot_hash == snap_t
        && allowed_of
            .get(&page.stamp.principal_id)
            .is_some_and(|allowed_s| allowed_s.is_subset(allowed_t))
}

/// A compact text rendering of a page's claims, for feeding it as a source.
fn page_summary(p: &CompoundedPage) -> String {
    p.claims
        .iter()
        .map(|c| format!("- {}", c.text()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Answers one question within scope T and returns a compounded page. The model
/// receives ONLY in-scope raw documents (the slice-2 governed gate) plus
/// `eligible` compounded pages — and EACH candidate page is RE-CHECKED here via
/// [`is_eligible`] (same snapshot AND `allowed(S) ⊆ allowed(T)`), so an
/// ineligible page's summary text can never reach the model even if the caller
/// mis-filters. A claim citing anything outside the provided set is refused. The
/// page's closure is therefore within `allowed(T)` — `CompoundStore::add`
/// re-checks this fail-closed on write.
#[allow(clippy::too_many_arguments)]
pub fn compound_answer(
    sources: &Sources,
    ctx: &ScopeContext,
    question: &str,
    query: &str,
    selector: &dyn DocSelector,
    synth: &dyn Synthesizer,
    verifier: &dyn Verifier,
    eligible: &[&CompoundedPage],
    allowed_of: &BTreeMap<String, BTreeSet<String>>,
    ordinal: u64,
) -> Result<CompoundedPage> {
    let gate = &ctx.gate;
    let snap_t = gate.snapshot_version.as_str();
    let allowed_t = gate.allowed();
    let doc_index: BTreeMap<&str, usize> = sources
        .documents
        .documents
        .iter()
        .enumerate()
        .map(|(i, d)| (d.id.as_str(), i))
        .collect();

    // 1. Raw in-scope sources via the governed gate (nothing outside the scope).
    let selected = selector.select(query, TOPIC_K)?;
    let mut model_sources: Vec<SourceDoc> = Vec::new();
    let mut raw_ids: BTreeSet<String> = BTreeSet::new();
    for id in selected.iter().filter(|id| gate.permits(id)) {
        if let Some(sd) = ctx.source_doc(id) {
            raw_ids.insert(sd.doc_id.clone());
            model_sources.push(sd);
        }
    }

    // 2. Eligible compounded pages — DEFENSIVELY re-checked here (not trusting
    //    the caller's filtering), so an ineligible page's summary text can never
    //    reach the model. Each surviving page is already ⊆ allowed(T).
    let mut page_ids: BTreeSet<String> = BTreeSet::new();
    for p in eligible {
        if !is_eligible(p, snap_t, allowed_t, allowed_of) {
            continue;
        }
        page_ids.insert(p.page_id.clone());
        model_sources.push(SourceDoc {
            doc_id: p.page_id.clone(),
            title: format!("compounded page (scope {})", p.stamp.principal_id),
            text: page_summary(p),
        });
    }

    // The in-scope source bodies (raw docs + eligible-page summaries) grounding
    // anchors verbatim spans against.
    let body_of: BTreeMap<&str, &str> = model_sources
        .iter()
        .map(|s| (s.doc_id.as_str(), s.text.as_str()))
        .collect();

    // 3. Synthesize over raw + compounded — all within allowed(T).
    let raws = synth.synthesize(question, &model_sources)?;

    // 4. Per claim: (a) typed-source/scope gate, then (b) GROUNDING — a verbatim
    //    in-scope span + judge support, fail-closed. For a CompoundedPage source
    //    the span is verbatim in that PAGE's text (not a re-anchor at the raw
    //    layer); the page's own claims were grounded to raw spans when created, so
    //    the chain bottoms out at raw docs via the no-laundering closure.
    //    Refused-unfounded / withheld are recorded, not kept.
    let mut claims = Vec::new();
    let mut rejected = Vec::new();
    let mut refused_unfounded = Vec::new();
    let mut withheld = Vec::new();
    // Slice 5: deterministic roster for free-text mention flagging on admitted prose.
    let roster = Roster::from_sources(sources);
    let mut mention_flags: Vec<MentionFlag> = Vec::new();
    for raw in raws {
        let source = if raw_ids.contains(&raw.cited_doc_id) {
            let span = match doc_index.get(raw.cited_doc_id.as_str()) {
                Some(i) => format!("/documents/{i}/body"),
                None => "/documents/body".to_string(),
            };
            SourceRef::RawDoc {
                doc_id: raw.cited_doc_id.clone(),
                span,
            }
        } else if page_ids.contains(&raw.cited_doc_id) {
            SourceRef::CompoundedPage {
                page_id: raw.cited_doc_id.clone(),
            }
        } else {
            rejected.push(raw.cited_doc_id.clone());
            continue;
        };

        let fact = raw.text.trim();
        let body = body_of
            .get(raw.cited_doc_id.as_str())
            .copied()
            .unwrap_or("");
        match ground_claim(fact, &raw.cited_doc_id, &raw.quote, body, verifier) {
            Grounded::Admitted { anchor, support } => {
                // Slice 5: flag (fail-closed) any principal the admitted prose
                // names that this scope is not granted about, before the source
                // is moved into the claim.
                let cited = match &source {
                    SourceRef::RawDoc { doc_id, span } => format!("RawDoc {doc_id} {span}"),
                    SourceRef::CompoundedPage { page_id } => format!("CompoundedPage {page_id}"),
                };
                mention_flags.extend(roster.flag_prose(
                    &gate.principal_id,
                    allowed_t,
                    fact,
                    &cited,
                ));
                let text = format!(
                    "{} [scope {} via {}]",
                    fact,
                    gate.principal_id,
                    synth.model_id()
                );
                claims.push(CompoundClaim::new(text, source, anchor, support));
            }
            Grounded::RefusedUnfounded { .. } => refused_unfounded.push(raw.cited_doc_id.clone()),
            Grounded::Withheld { .. } => withheld.push(raw.cited_doc_id.clone()),
        }
    }

    Ok(CompoundedPage {
        page_id: format!("cp{ordinal:04}-{}", gate.principal_id),
        ordinal,
        stamp: ScopeStamp {
            principal_id: gate.principal_id.clone(),
            snapshot_hash: gate.snapshot_version.clone(),
        },
        question: question.to_string(),
        model: synth.model_id().to_string(),
        claims,
        rejected,
        refused_unfounded,
        withheld,
        mention_flags,
    })
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Renders one compounded page, including its transitive raw-doc closure (the
/// no-laundering proof, shown inline).
pub fn render_compounded_page(store: &CompoundStore, p: &CompoundedPage) -> String {
    let mut s = String::new();
    s.push_str(&format!("# Compounded page `{}`\n\n", p.page_id));
    s.push_str(&format!(
        "> Scope stamp: principal `{}` · snapshot `{}` · ordinal {}. Derived by `{}`.\n\n",
        p.stamp.principal_id, p.stamp.snapshot_hash, p.ordinal, p.model
    ));
    s.push_str(&format!("**Question:** {}\n\n", p.question));

    s.push_str(&format!(
        "Grounding: **{}** admitted (verbatim-anchored + support-checked, fail-closed) · \
         **{}** refused-unfounded · **{}** withheld-unsupported. \
         Support is a judge's assessment — anchored + support-checked, NOT proven faithful; on \
         current local judges the live admit path over-refuses and admits ~zero.\n\n",
        p.claims.len(),
        p.refused_unfounded.len(),
        p.withheld.len()
    ));

    s.push_str(&format!("## Claims ({})\n\n", p.claims.len()));
    for c in &p.claims {
        let cite = match c.source() {
            SourceRef::RawDoc { doc_id, span } => format!("RawDoc {doc_id} {span}"),
            SourceRef::CompoundedPage { page_id } => format!("CompoundedPage {page_id}"),
        };
        let a = c.anchor();
        s.push_str(&format!(
            "- {} — `src: {}` · `anchor: \"{}\" @{}` · support: {} (judge `{}`)\n",
            c.text(),
            cite,
            a.span_text.replace('\n', " "),
            a.locator,
            if c.support().supported {
                "confirmed"
            } else {
                "unconfirmed"
            },
            c.support().judge_model,
        ));
    }
    s.push('\n');

    match store.transitive_raw_docs(&p.page_id) {
        Ok(closure) => {
            s.push_str(&format!(
                "## Transitive raw-doc closure ({}) — all within scope `{}`\n\n",
                closure.len(),
                p.stamp.principal_id
            ));
            s.push_str(&format!(
                "{}\n\n",
                closure.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        Err(e) => s.push_str(&format!("## Closure ERROR: {e}\n\n")),
    }
    if !p.mention_flags.is_empty() {
        s.push_str(&format!(
            "## ⚠ Free-text mention flags ({}) — slice 5\n\n",
            p.mention_flags.len()
        ));
        s.push_str("> Admitted prose NAMED a principal this scope is **not** granted about (or an ambiguous name token). Deterministic roster match (canonical-surface forms only — non-roster entities and apostrophe-elided name variants are **not** caught) — flagged, **not** reconciled; the granted set is **unchanged**.\n\n");
        for m in &p.mention_flags {
            let who = match &m.mentioned_id {
                Some(id) => format!("`{id}`"),
                None => format!(
                    "ambiguous `{}` → {{{}}}",
                    m.surface,
                    m.candidates.join(", ")
                ),
            };
            s.push_str(&format!("- mention {who} — `src: {}`\n", m.cited_source));
        }
        s.push('\n');
    }
    if !p.rejected.is_empty() {
        s.push_str(&format!(
            "## Refused cites ({}) — outside the scope-gated source set\n\n",
            p.rejected.len()
        ));
        for r in &p.rejected {
            s.push_str(&format!("- `{r}`\n"));
        }
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(src: &str) -> Anchor {
        Anchor {
            source_ref: src.to_string(),
            span_text: "span".to_string(),
            locator: format!("{src}@0"),
        }
    }
    fn ok() -> SupportVerdict {
        SupportVerdict {
            supported: true,
            judge_model: "fake".to_string(),
        }
    }
    fn raw(text: &str, doc: &str) -> CompoundClaim {
        CompoundClaim::new(
            text,
            SourceRef::RawDoc {
                doc_id: doc.to_string(),
                span: "/documents/0/body".to_string(),
            },
            anchor(doc),
            ok(),
        )
    }
    fn page(id: &str, ord: u64, scope: &str, claims: Vec<CompoundClaim>) -> CompoundedPage {
        CompoundedPage {
            page_id: id.to_string(),
            ordinal: ord,
            stamp: ScopeStamp {
                principal_id: scope.to_string(),
                snapshot_hash: "snap".to_string(),
            },
            question: "q".to_string(),
            model: "fake".to_string(),
            claims,
            rejected: vec![],
            refused_unfounded: vec![],
            withheld: vec![],
            mention_flags: vec![],
        }
    }
    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }
    fn amap(pairs: &[(&str, &BTreeSet<String>)]) -> BTreeMap<String, BTreeSet<String>> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), (*v).clone()))
            .collect()
    }

    #[test]
    fn closure_follows_pages_to_raw_docs_and_terminates() {
        let mut store = CompoundStore::new();
        let allowed = set(&["d1", "d2", "d3"]);
        let m = amap(&[("S", &allowed)]);
        store
            .add(
                page("cp0-S", 0, "S", vec![raw("a", "d1"), raw("b", "d2")]),
                &m,
            )
            .unwrap();
        // A round-2 page cites the round-1 page plus a fresh raw doc.
        let p2 = page(
            "cp1-S",
            1,
            "S",
            vec![
                CompoundClaim::new(
                    "c",
                    SourceRef::CompoundedPage {
                        page_id: "cp0-S".into(),
                    },
                    anchor("cp0-S"),
                    ok(),
                ),
                raw("d", "d3"),
            ],
        );
        store.add(p2, &m).unwrap();
        let closure = store.transitive_raw_docs("cp1-S").unwrap();
        assert_eq!(
            closure,
            set(&["d1", "d2", "d3"]),
            "closure bottoms out at raw docs"
        );
    }

    #[test]
    fn add_refuses_a_laundering_page() {
        let mut store = CompoundStore::new();
        // Scope S allows only d1; a page citing d9 (outside) must be refused.
        let allowed = set(&["d1"]);
        let m = amap(&[("S", &allowed)]);
        let bad = page("cp0-S", 0, "S", vec![raw("a", "d1"), raw("leak", "d9")]);
        let err = store.add(bad, &m).unwrap_err().to_string();
        assert!(err.contains("no-laundering"), "rejected: {err}");
        assert_eq!(store.len(), 0, "the laundering page was not stored");
    }

    #[test]
    fn add_refuses_a_forward_cite_keeping_the_dag_acyclic() {
        let mut store = CompoundStore::new();
        let allowed = set(&["d1"]);
        let m = amap(&[("S", &allowed)]);
        store
            .add(page("cp0-S", 0, "S", vec![raw("a", "d1")]), &m)
            .unwrap();
        // A page citing a not-yet-earlier (same/later ordinal) page is refused.
        let bad = page(
            "cp1-S",
            1,
            "S",
            vec![CompoundClaim::new(
                "x",
                SourceRef::CompoundedPage {
                    page_id: "cp2-S".into(),
                },
                anchor("cp2-S"),
                ok(),
            )],
        );
        assert!(store.add(bad, &m).is_err(), "forward/unknown cite refused");
    }

    #[test]
    fn eligibility_is_fail_closed_on_subset_and_snapshot() {
        let mut store = CompoundStore::new();
        let a_sales = set(&["d1", "d2"]); // Sales
        let a_hr = set(&["d3"]); // HR (disjoint -> non-nested)
        let allowed_of = amap(&[("sales", &a_sales), ("hr", &a_hr)]);
        store
            .add(
                page("cp0-sales", 0, "sales", vec![raw("a", "d1")]),
                &allowed_of,
            )
            .unwrap();
        store
            .add(page("cp1-hr", 1, "hr", vec![raw("b", "d3")]), &allowed_of)
            .unwrap();

        // For an HR question: only the HR page is eligible (Sales ⊄ HR).
        let elig_hr = store.eligible_for("snap", &a_hr, &allowed_of);
        assert_eq!(elig_hr.len(), 1);
        assert_eq!(elig_hr[0].page_id, "cp1-hr");

        // Wrong snapshot -> excluded.
        assert!(store
            .eligible_for("other-snap", &a_hr, &allowed_of)
            .is_empty());

        // Unresolvable scope -> excluded (fail-closed).
        assert!(store
            .eligible_for("snap", &a_hr, &BTreeMap::new())
            .is_empty());
    }
}
