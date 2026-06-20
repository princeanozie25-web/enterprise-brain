//! Markdown rendering of the derived layer.
//!
//! Rendering consumes [`Claim`]s, and a `Claim` always carries a `Provenance`,
//! so every fact line emitted here ends in an inline `src:` cite. The governed
//! access block is printed verbatim from the model; the fail-closed flags block
//! is clearly labelled as "not reconciled, access NOT widened". Output is
//! deterministic — no clock, no ordering by anything but stable keys.

use std::collections::BTreeSet;

use crate::derive::{DerivedLayer, GovernedAccess, Page};

/// One rendered file, addressed relative to `out/`.
#[derive(Debug, Clone)]
pub struct RenderedPage {
    pub relpath: String,
    pub markdown: String,
}

/// Renders every page of the layer (index + all entity pages).
pub fn render_layer(layer: &DerivedLayer) -> Vec<RenderedPage> {
    let mut out = Vec::new();
    out.push(RenderedPage {
        relpath: "index.md".to_string(),
        markdown: render_index_page(layer),
    });
    for page in layer.entity_pages() {
        out.push(RenderedPage {
            relpath: format!("{}/{}.md", page.kind.dir(), page.id),
            markdown: render_entity_page(page),
        });
    }
    out
}

const BANNER: &str =
    "> Bryremead Distribution Ltd — synthetic org knowledge layer (slice 1). Deterministically derived; every claim cites its source.";

fn render_entity_page(page: &Page) -> String {
    let mut s = String::new();
    s.push_str(&format!("# {}\n\n", page.title));
    s.push_str(BANNER);
    s.push_str("\n\n");

    // Facts — one provenance-anchored line per claim.
    s.push_str("## Facts\n\n");
    for claim in &page.claims {
        s.push_str(&format!(
            "- {} — `src: {}`\n",
            claim.text(),
            claim.provenance().cite()
        ));
    }
    s.push('\n');

    // Cross-links (deduplicated, stable order).
    let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
    let mut link_lines: Vec<String> = Vec::new();
    for link in &page.links {
        if seen.insert((link.kind.dir(), &link.id)) {
            // All entity pages live one level under out/, so `../<dir>/<id>.md`
            // resolves from any of them.
            link_lines.push(format!(
                "- [{}](../{}/{}.md)\n",
                link.label,
                link.kind.dir(),
                link.id
            ));
        }
    }
    if !link_lines.is_empty() {
        s.push_str("## Links\n\n");
        for l in link_lines {
            s.push_str(&l);
        }
        s.push_str("- [↑ index](../index.md)\n\n");
    } else {
        s.push_str("## Links\n\n- [↑ index](../index.md)\n\n");
    }

    // Governed access (person pages) — verbatim from the compiled model.
    if let Some(ga) = &page.governed_access {
        render_governed_access(&mut s, ga);
    }

    // Fail-closed flags.
    if !page.discrepancies.is_empty() {
        s.push_str(&format!(
            "## ⚠ Fail-closed flags ({})\n\n",
            page.discrepancies.len()
        ));
        s.push_str("> Derivation implied access the authorization model does **not** grant. Flagged, not reconciled — access **NOT** widened.\n\n");
        for d in page.discrepancies.iter().take(crate::derive::SAMPLE_LIMIT) {
            s.push_str(&format!(
                "- `{}` via {} — `src: {}`\n",
                d.document_id,
                d.bases.join(", "),
                d.provenance.cite()
            ));
        }
        if page.discrepancies.len() > crate::derive::SAMPLE_LIMIT {
            s.push_str(&format!(
                "- …and {} more flagged document(s).\n",
                page.discrepancies.len() - crate::derive::SAMPLE_LIMIT
            ));
        }
        s.push('\n');
    }

    s
}

fn render_governed_access(s: &mut String, ga: &GovernedAccess) {
    // The governed-access facts are sourced to the compiled authorization model
    // itself — keyed by principal, pinned to its snapshot — not to a fixture
    // span. They carry a `src:` cite to that model, just as fixture-derived
    // claims cite their file: no fact on a page is left unattributed.
    let snap = &ga.snapshot_version[..ga.snapshot_version.len().min(12)];
    s.push_str(&format!(
        "## Governed access (read-only · compiled model `{}`)\n\n",
        ga.snapshot_version
    ));
    let denied = ga
        .denied_count
        .map(|c| c.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    s.push_str(&format!(
        "Granted documents: **{}** · denied (deny-by-default): **{}** — `src: compiled-model#{} (snapshot {})`\n\n",
        ga.allowed_total, denied, ga.principal_id, snap
    ));
    if ga.sample.is_empty() {
        s.push_str("_No documents granted by the model._\n\n");
        return;
    }
    s.push_str(&format!(
        "First {} granted (of {}), with the model's own reason trace:\n\n",
        ga.sample.len(),
        ga.allowed_total
    ));
    for d in &ga.sample {
        let sup = if d.superseded { " _(superseded)_" } else { "" };
        let reasons = if d.reasons.is_empty() {
            "(no reason recorded)".to_string()
        } else {
            d.reasons.join(", ")
        };
        s.push_str(&format!(
            "- `{}` {}{} — why: {} — `src: compiled-model#{} /entries/{} (snapshot {})`\n",
            d.document_id, d.title, sup, reasons, ga.principal_id, d.document_id, snap
        ));
    }
    s.push('\n');
}

fn render_index_page(layer: &DerivedLayer) -> String {
    let mut s = String::new();
    s.push_str(&format!("# {}\n\n", layer.index.title));
    s.push_str(BANNER);
    s.push_str("\n\n## Summary\n\n");
    for claim in &layer.index.claims {
        s.push_str(&format!(
            "- {} — `src: {}`\n",
            claim.text(),
            claim.provenance().cite()
        ));
    }
    s.push('\n');

    // Department entry points (only 8 — a usable front door into the layer).
    s.push_str("## Departments\n\n");
    for d in &layer.departments {
        s.push_str(&format!("- [{}]({}/{}.md)\n", d.title, d.kind.dir(), d.id));
    }
    s.push('\n');

    // Fail-closed roll-up across the whole layer.
    let total_flags = layer.all_discrepancies().len();
    s.push_str("## Fail-closed roll-up\n\n");
    s.push_str(&format!(
        "Across all pages, derivation raised **{total_flags}** flag(s) where a derived association implied access the authorization model does not grant. Every one is surfaced on its page; none widened access.\n\n"
    ));

    s.push_str("## Sections\n\n");
    s.push_str(&format!("- `people/` — {} pages\n", layer.people.len()));
    s.push_str(&format!(
        "- `departments/` — {} pages\n",
        layer.departments.len()
    ));
    s.push_str(&format!("- `projects/` — {} pages\n", layer.projects.len()));
    s.push_str(&format!("- `tools/` — {} pages\n", layer.tools.len()));
    s
}
