//! AP-5: POST /export — Evidence Export. Every major view emits a dated,
//! attested, reason-traced PDF suitable for handing to an auditor.
//!
//! THE LAW: THE SERVER DERIVES, NEVER RECEIVES. The request names a view in
//! PARAMETERS ONLY (strict serde, unknown fields refuse) — the schema has
//! no field that could carry content, so the attestation can never bless
//! client-supplied bytes. The server re-performs the named act fresh
//! through the SAME authorize seams and builders the live views use, hashes
//! the canonical JSON it derived, audits the export as its own act, and
//! renders exactly what it derived.
//!
//! THE ONE DATED RESPONSE (explicit ruling): the PDF header carries
//! "Generated: <RFC 3339>". The date participates in NOTHING attested —
//! not the content hash, not the canonical JSON — and is injectable for
//! tests (fixed-date mode renders byte-identical PDFs). Everywhere else
//! the no-wall-clock rule stands untouched.
//!
//! DETERMINISM NOTE (flagged): printpdf stamps a random document id and
//! wall-clock Info dates into every file. Both are metadata outside the
//! attested content, so after rendering they are neutralized in place
//! (same-length byte rewrites, xref offsets preserved): the document id
//! becomes the two halves of the content sha256 — derived, meaningful —
//! and the Info dates become the same injected date the header shows.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use anyhow::{bail, Context as _, Result};
use genpdf::elements::{Break, Paragraph};
use genpdf::style::Style;
use genpdf::{fonts, Margins, Mm, Position};
use retrieval::index::{canonical_json_bytes, sha256_hex};
use serde::Deserialize;
use serde_json::Value;

use crate::answer::{ask, AskError, AskOptions};
use crate::{atlas, diff, lens, sidecar, AppState};

/// The non-negotiable watermark line, verbatim.
pub const WATERMARK: &str = "DEMO IDENTITY MODE — synthetic corpus, demonstration identities";

// ---------------------------------------------------------------------------
// Request (params ONLY — content impossible by construction)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportRequest {
    pub view: String,
    #[serde(default)]
    pub lens: Option<LensParams>,
    #[serde(default)]
    pub diff: Option<DiffParams>,
    #[serde(default)]
    pub atlas_capability: Option<AtlasCapabilityParams>,
    #[serde(default)]
    pub ask: Option<AskParams>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LensParams {
    pub subject_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiffParams {
    pub left: String,
    pub right: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtlasCapabilityParams {
    pub capability_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AskParams {
    pub query: String,
    pub hybrid: bool,
    pub judge: bool,
}

// ---------------------------------------------------------------------------
// The dated line (the ONE permitted wall-clock read)
// ---------------------------------------------------------------------------

/// Days-to-civil conversion (Howard Hinnant's algorithm) — no date crate.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn rfc3339_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    format!("{year:04}-{month:02}-{day:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    rfc3339_from_unix(secs)
}

/// "2026-01-05T00:00:00Z" -> "D:20260105000000+00'00'" (the PDF form, same
/// length as printpdf's own stamps).
fn pdf_date_from_rfc3339(rfc: &str) -> String {
    let digits: String = rfc
        .chars()
        .filter(|c| c.is_ascii_digit())
        .take(14)
        .collect();
    format!("D:{digits}+00'00'")
}

// ---------------------------------------------------------------------------
// Derivation (the same seams and builders as the live views)
// ---------------------------------------------------------------------------

struct Derived {
    /// Canonical JSON — the EXACT bytes the renderer consumes and the
    /// attestation hashes.
    body: Vec<u8>,
    view_title: String,
    subjects: String,
    /// params-as-target for the evidence_export audit row.
    target: String,
    /// The ordinal of the act's own audit row, where the act writes one.
    act_ordinal: Option<u64>,
}

fn derive(
    state: &AppState,
    actor: &str,
    request: &ExportRequest,
) -> Result<Option<Derived>, AskError> {
    // The view names exactly one params object; anything else disagrees.
    let params_given = [
        request.lens.is_some(),
        request.diff.is_some(),
        request.atlas_capability.is_some(),
        request.ask.is_some(),
    ]
    .iter()
    .filter(|p| **p)
    .count();
    if params_given != 1 {
        return Err(AskError::BadRequest(
            "exactly one params object must accompany the view".to_string(),
        ));
    }
    match request.view.as_str() {
        "lens" => {
            let Some(params) = &request.lens else {
                return Err(AskError::BadRequest("view and params disagree".to_string()));
            };
            let Some((body, act_ordinal)) = lens::lens_view(state, actor, &params.subject_id)?
            else {
                return Ok(None);
            };
            Ok(Some(Derived {
                body,
                view_title: format!("Lens — {}", params.subject_id),
                subjects: params.subject_id.clone(),
                target: format!("lens:{}", params.subject_id),
                act_ordinal,
            }))
        }
        "diff" => {
            let Some(params) = &request.diff else {
                return Err(AskError::BadRequest("view and params disagree".to_string()));
            };
            let Some((body, act_ordinal)) =
                diff::diff_view(state, actor, &params.left, &params.right)?
            else {
                return Ok(None);
            };
            Ok(Some(Derived {
                body,
                view_title: format!("Lens diff — {} vs {}", params.left, params.right),
                subjects: format!("{} | {}", params.left, params.right),
                target: format!("diff:{}|{}", params.left, params.right),
                act_ordinal: Some(act_ordinal),
            }))
        }
        "atlas_capability" => {
            let Some(params) = &request.atlas_capability else {
                return Err(AskError::BadRequest("view and params disagree".to_string()));
            };
            // The actor's own atlas — same builder, same empty-allowlist
            // rule. An unknown capability and a capability outside the
            // actor's standing share THE one 404.
            let Some(bytes) = atlas::atlas_view(state, actor)? else {
                return Ok(None);
            };
            let atlas_body: Value = serde_json::from_slice(&bytes)
                .context("atlas body fails re-parse")
                .map_err(AskError::Internal)?;
            let mut found: Option<Value> = None;
            if let Some(strategies) = atlas_body["strategies"].as_array() {
                for strategy in strategies {
                    for initiative in strategy["initiatives"].as_array().into_iter().flatten() {
                        for workflow in initiative["workflows"].as_array().into_iter().flatten() {
                            for capability in
                                workflow["capabilities"].as_array().into_iter().flatten()
                            {
                                if capability["id"] == params.capability_id.as_str() {
                                    found = Some(capability.clone());
                                }
                            }
                        }
                    }
                }
            }
            let Some(capability) = found else {
                return Ok(None);
            };
            let slice = serde_json::json!({
                "actor_id": atlas_body["actor_id"],
                "capability": capability,
                "snapshot_version": atlas_body["snapshot_version"],
            });
            let body = canonical_json_bytes(&slice).map_err(AskError::Internal)?;
            Ok(Some(Derived {
                body,
                view_title: format!("Atlas capability — {}", params.capability_id),
                subjects: params.capability_id.clone(),
                target: format!("atlas_capability:{}", params.capability_id),
                act_ordinal: None,
            }))
        }
        "ask" => {
            let Some(params) = &request.ask else {
                return Err(AskError::BadRequest("view and params disagree".to_string()));
            };
            let options = AskOptions {
                hybrid: params.hybrid,
                judge: params.judge,
                bypass_cache: false,
            };
            let (body, trace) = ask(state, actor, &params.query, &options)?;
            // Metering parity with the live endpoint (failures to stderr).
            if let Some(usage_path) = &state.usage_out {
                if let Err(err) = sidecar::append_all(usage_path, &trace.usage_events) {
                    eprintln!("usage sidecar append failed: {err:#}");
                }
            }
            let envelope: Value = serde_json::from_slice(&body)
                .context("envelope fails re-parse")
                .map_err(AskError::Internal)?;
            let query_hash = envelope["query_hash"].as_str().unwrap_or("").to_string();
            let hash8: String = query_hash.chars().take(8).collect();
            Ok(Some(Derived {
                body,
                view_title: format!("Ask — {hash8}"),
                subjects: actor.to_string(),
                target: format!("ask:{query_hash}"),
                act_ordinal: None,
            }))
        }
        _ => Err(AskError::BadRequest("unknown view".to_string())),
    }
}

// ---------------------------------------------------------------------------
// The export act
// ---------------------------------------------------------------------------

/// Derive fresh, attest, audit the export, render. `Ok(None)` = THE one
/// 404; refusals propagate with the live endpoints' own semantics and
/// leave NO evidence_export row (the AD-2 precedent: refusals are rowless).
pub fn export_view(
    state: &AppState,
    actor: &str,
    request: &ExportRequest,
) -> Result<Option<Vec<u8>>, AskError> {
    let Some(derived) = derive(state, actor, request)? else {
        return Ok(None);
    };

    // ATTEST: the hash of the exact bytes the renderer consumes.
    let content_sha256 = sha256_hex(&derived.body);

    // AUDIT THE EXPORT — its own act, before any bytes leave. No audit
    // sink: the act cannot be recorded, so it cannot happen. Fail closed.
    let Some(store) = &state.proposals else {
        return Err(AskError::Internal(anyhow::anyhow!(
            "evidence export requires the audit store (--state-dir); refusing"
        )));
    };
    let export_ordinal = store
        .audit("evidence_export", actor, &derived.target, "allowed_demo")
        .map_err(AskError::Internal)?;

    let mut audit_ordinals = Vec::new();
    if let Some(act) = derived.act_ordinal {
        audit_ordinals.push(act);
    }
    audit_ordinals.push(export_ordinal);

    // THE ONE DATED RESPONSE: injectable for tests, real elsewhere.
    let generated = state.export_fixed_date.clone().unwrap_or_else(now_rfc3339);

    let body: Value = serde_json::from_slice(&derived.body)
        .context("derived body fails re-parse")
        .map_err(AskError::Internal)?;
    let meta = ExportMeta {
        actor: actor.to_string(),
        subjects: derived.subjects,
        view_title: derived.view_title,
        snapshot_version: state.snapshot_version.clone(),
        index_version: state.engine.manifest.index_version.clone(),
        content_sha256,
        audit_ordinals,
        generated,
    };
    render_export_pdf(&request.view, &body, &meta, &state.export_fonts_dir)
        .map(Some)
        .map_err(AskError::Internal)
}

// ---------------------------------------------------------------------------
// Rendering (genpdf; A4; 18mm margins; footer on every page)
// ---------------------------------------------------------------------------

/// Everything the header and footer state about the export. Public so the
/// AE-suite can render synthetic bodies through the same code path.
pub struct ExportMeta {
    pub actor: String,
    pub subjects: String,
    pub view_title: String,
    pub snapshot_version: String,
    pub index_version: String,
    pub content_sha256: String,
    pub audit_ordinals: Vec<u64>,
    pub generated: String,
}

/// Spec sizes 16/9.5/7.5pt; genpdf takes integral points, so print uses
/// 16/10/8 (flagged in the AP-5 closeout).
const SIZE_HEADER: u8 = 16;
const SIZE_BODY: u8 = 10;
const SIZE_SMALL: u8 = 8;
const FOOTER_LINE_MM: f64 = 3.4;

struct LoadedFonts {
    chrome: fonts::FontFamily<fonts::FontData>,
    evidence: fonts::FontFamily<fonts::FontData>,
    answer: fonts::FontFamily<fonts::FontData>,
}

fn font_data(dir: &Path, file: &str) -> Result<fonts::FontData> {
    let bytes = std::fs::read(dir.join(file))
        .with_context(|| format!("cannot read vendored font {}", dir.join(file).display()))?;
    fonts::FontData::new(bytes, None)
        .map_err(|err| anyhow::anyhow!("font {file} fails to parse: {err}"))
}

/// The vendored families. Weights stand in where the subset lacks a true
/// face: bold = the 600/500 cut, italics fall back to the uprights
/// (flagged — the latin woff2 subsets vendor no italics).
fn load_fonts(dir: &Path) -> Result<LoadedFonts> {
    let family = |regular: &str, bold: &str| -> Result<fonts::FontFamily<fonts::FontData>> {
        let regular = font_data(dir, regular)?;
        let bold = font_data(dir, bold)?;
        Ok(fonts::FontFamily {
            regular: regular.clone(),
            bold: bold.clone(),
            italic: regular,
            bold_italic: bold,
        })
    };
    Ok(LoadedFonts {
        chrome: family("Inter-Regular.ttf", "Inter-Bold.ttf")?,
        evidence: family("IBMPlexMono-Regular.ttf", "IBMPlexMono-Bold.ttf")?,
        answer: family("SourceSerif4-Regular.ttf", "SourceSerif4-Bold.ttf")?,
    })
}

/// Footer-and-margins page decorator: 18mm margins, the attestation block
/// on EVERY page, page n/N. Page count is discovered by the first render
/// pass (the reserved footer height is constant, so pagination is
/// identical across passes).
struct EvidencePages {
    footer_lines: Vec<String>,
    mono: fonts::FontFamily<fonts::Font>,
    total: Option<usize>,
    seen: Rc<Cell<usize>>,
}

impl genpdf::PageDecorator for EvidencePages {
    fn decorate_page<'a>(
        &mut self,
        context: &genpdf::Context,
        mut area: genpdf::render::Area<'a>,
        _style: Style,
    ) -> Result<genpdf::render::Area<'a>, genpdf::error::Error> {
        let page = self.seen.get() + 1;
        self.seen.set(page);
        area.add_margins(Margins::trbl(18, 18, 18, 18));
        let size = area.size();
        let style = Style::new()
            .with_font_family(self.mono)
            .with_font_size(SIZE_SMALL);

        // The reservation pads past the last baseline: a footer line that
        // does not fit is a hard error below, never a silent absence.
        let lines = self.footer_lines.len() + 1;
        let footer_height = Mm::from(FOOTER_LINE_MM * lines as f64 + 2.0);
        let mut footer = area.clone();
        footer.add_offset(Position::new(0, size.height - footer_height));
        let page_line = match self.total {
            Some(total) => format!("page {page}/{total}"),
            None => format!("page {page}/?"),
        };
        let mut y = 0.0;
        for line in self.footer_lines.iter().chain(std::iter::once(&page_line)) {
            let printed =
                footer.print_str(&context.font_cache, Position::new(0, y), style, line)?;
            if !printed {
                return Err(genpdf::error::Error::new(
                    "footer line did not fit the reserved block",
                    genpdf::error::ErrorKind::Internal,
                ));
            }
            y += FOOTER_LINE_MM;
        }

        area.set_height(size.height - footer_height - Mm::from(4.0));
        Ok(area)
    }
}

fn footer_lines(meta: &ExportMeta) -> Vec<String> {
    let ordinals: Vec<String> = meta.audit_ordinals.iter().map(u64::to_string).collect();
    vec![
        format!("actor: {}    subjects: {}", meta.actor, meta.subjects),
        format!("snapshot: {}", meta.snapshot_version),
        format!("index: {}", meta.index_version),
        format!("content sha256: {}", meta.content_sha256),
        format!("audit ordinals: {}", ordinals.join(", ")),
    ]
}

/// Renders the export once. Two passes are driven by `render_export_pdf`.
fn render_pass(
    view: &str,
    body: &Value,
    meta: &ExportMeta,
    loaded: &LoadedFonts,
    total: Option<usize>,
) -> Result<(Vec<u8>, usize)> {
    let mut doc = genpdf::Document::new(loaded.chrome.clone());
    doc.set_paper_size(genpdf::PaperSize::A4);
    doc.set_minimal_conformance();
    doc.set_title("Aperture Evidence Export");
    doc.set_font_size(SIZE_BODY);

    let mono = doc.add_font_family(loaded.evidence.clone());
    let serif = doc.add_font_family(loaded.answer.clone());

    let seen = Rc::new(Cell::new(0usize));
    doc.set_page_decorator(EvidencePages {
        footer_lines: footer_lines(meta),
        mono,
        total,
        seen: Rc::clone(&seen),
    });

    let chrome = Style::new();
    let chrome_bold = Style::new().bold();
    let small = Style::new().with_font_size(SIZE_SMALL);
    let mono_body = Style::new()
        .with_font_family(mono)
        .with_font_size(SIZE_BODY);
    let mono_small = Style::new()
        .with_font_family(mono)
        .with_font_size(SIZE_SMALL);

    // HEADER (page 1): title, view, the ONE date line, the watermark.
    doc.push(Paragraph::default().styled_string(
        "Aperture Evidence Export",
        chrome_bold.with_font_size(SIZE_HEADER),
    ));
    doc.push(Paragraph::default().styled_string(&meta.view_title, chrome_bold.with_font_size(12)));
    doc.push(Paragraph::default().styled_string(format!("Generated: {}", meta.generated), small));
    doc.push(Paragraph::default().styled_string(WATERMARK, small));
    doc.push(Break::new(1.0));

    match view {
        "lens" => push_lens(
            &mut doc,
            body,
            chrome,
            chrome_bold,
            small,
            mono_body,
            mono_small,
        ),
        "diff" => push_diff(
            &mut doc,
            body,
            chrome,
            chrome_bold,
            small,
            mono_body,
            mono_small,
        )?,
        "atlas_capability" => {
            push_atlas_capability(&mut doc, body, chrome, chrome_bold, mono_body, mono_small)
        }
        "ask" => {
            let serif_style = Style::new()
                .with_font_family(serif)
                .with_font_size(SIZE_BODY);
            push_ask(
                &mut doc,
                body,
                chrome,
                chrome_bold,
                small,
                mono_body,
                mono_small,
                serif_style,
            )
        }
        other => bail!("unknown view {other:?} reached the renderer"),
    }

    let mut bytes = Vec::new();
    doc.render(&mut bytes)
        .map_err(|err| anyhow::anyhow!("pdf render failed: {err}"))?;
    Ok((bytes, seen.get()))
}

/// Two-pass render (page count), then metadata neutralization: printpdf's
/// random document id becomes the two halves of the content sha256 and its
/// wall-clock Info dates become the header's own date — same length, xref
/// intact, nothing attested touched.
pub fn render_export_pdf(
    view: &str,
    body: &Value,
    meta: &ExportMeta,
    fonts_dir: &Path,
) -> Result<Vec<u8>> {
    let loaded = load_fonts(fonts_dir)?;
    let (_, pages) = render_pass(view, body, meta, &loaded, None)?;
    let (bytes, _) = render_pass(view, body, meta, &loaded, Some(pages))?;
    neutralize_pdf_metadata(bytes, &meta.content_sha256, &meta.generated)
}

/// Replaces the payloads of the N literal strings that follow EVERY
/// occurrence of `key` (the trailer `/ID [(…) (…)]` carries two; the Info
/// dates carry one each). Same-length only — xref offsets are sacred.
fn replace_literals_after(bytes: &mut [u8], key: &[u8], payloads: &[&[u8]]) -> Result<()> {
    let mut search_from = 0usize;
    while let Some(at) = find(bytes, key, search_from) {
        let mut cursor = at + key.len();
        for payload in payloads {
            let open = cursor
                + bytes[cursor..]
                    .iter()
                    .position(|b| *b == b'(')
                    .context("PDF literal expected after key")?;
            let close = open
                + 1
                + bytes[open + 1..]
                    .iter()
                    .position(|b| *b == b')')
                    .context("PDF literal unterminated")?;
            let slot = &mut bytes[open + 1..close];
            if slot.len() != payload.len() {
                bail!(
                    "metadata payload length drifted ({} vs {}); refusing to corrupt offsets",
                    slot.len(),
                    payload.len()
                );
            }
            slot.copy_from_slice(payload);
            cursor = close;
        }
        search_from = cursor;
    }
    Ok(())
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn neutralize_pdf_metadata(
    mut bytes: Vec<u8>,
    content_sha256: &str,
    generated: &str,
) -> Result<Vec<u8>> {
    // Trailer /ID [(32) (32)]: document id AND instance id become the two
    // halves of the content hash — derived, meaningful, deterministic.
    let sha = content_sha256.as_bytes();
    if sha.len() != 64 {
        bail!("content hash is not 64 hex chars");
    }
    replace_literals_after(&mut bytes, b"/ID", &[&sha[..32], &sha[32..]])?;
    // Info dates: the same date the header states, same byte length.
    let stamp = pdf_date_from_rfc3339(generated).into_bytes();
    replace_literals_after(&mut bytes, b"/CreationDate", &[&stamp])?;
    replace_literals_after(&mut bytes, b"/ModDate", &[&stamp])?;
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// Print anatomy per view (the same shapes, one more renderer)
// ---------------------------------------------------------------------------

fn doc_row_paragraph(row: &Value, mono_small: Style, chrome: Style, small: Style) -> Paragraph {
    let mut paragraph = Paragraph::default();
    paragraph.push_styled(
        format!("{}  ", row["document_id"].as_str().unwrap_or("?")),
        mono_small,
    );
    paragraph.push_styled(row["title"].as_str().unwrap_or("").to_string(), chrome);
    paragraph.push_styled(
        format!("  [{}]", row["sensitivity"].as_str().unwrap_or("?")),
        small,
    );
    // Supersedence in print: genpdf has no strikethrough, so the strike is
    // the literal word (flagged); the successor renders ONLY where the
    // redaction law already put it in the body.
    if row["superseded"].as_bool() == Some(true) {
        match row["effective_successor"].as_str() {
            Some(successor) => {
                paragraph.push_styled("  (superseded — effective: ".to_string(), small);
                paragraph.push_styled(successor.to_string(), mono_small);
                paragraph.push_styled(")".to_string(), small);
            }
            None => paragraph.push_styled("  (superseded)".to_string(), small),
        }
    }
    paragraph
}

fn push_sections(
    doc: &mut genpdf::Document,
    sections: &Value,
    chrome: Style,
    chrome_bold: Style,
    small: Style,
    mono_small: Style,
) {
    for section in sections.as_array().into_iter().flatten() {
        let mut header = Paragraph::default();
        header.push_styled(
            section["sentence"].as_str().unwrap_or("").to_string(),
            chrome_bold,
        );
        header.push_styled(
            format!("   [{}]", section["reason"].as_str().unwrap_or("")),
            mono_small,
        );
        doc.push(header);
        for row in section["docs"].as_array().into_iter().flatten() {
            doc.push(doc_row_paragraph(row, mono_small, chrome, small));
        }
        doc.push(Break::new(0.5));
    }
}

fn push_lens(
    doc: &mut genpdf::Document,
    body: &Value,
    chrome: Style,
    chrome_bold: Style,
    small: Style,
    _mono_body: Style,
    mono_small: Style,
) {
    let subject = &body["subject"];
    let mut masthead = Paragraph::default();
    masthead.push_styled(
        format!("{}  ", subject["name"].as_str().unwrap_or("")),
        chrome_bold,
    );
    masthead.push_styled(
        format!("{}  ", subject["id"].as_str().unwrap_or("")),
        mono_small,
    );
    masthead.push_styled(
        format!("({})", subject["kind"].as_str().unwrap_or("")),
        small,
    );
    doc.push(masthead);
    if body["cross_lens"].as_bool() == Some(true) {
        doc.push(Paragraph::default().styled_string(
            format!(
                "Viewing as {} — this view is audited.",
                body["actor_id"].as_str().unwrap_or("")
            ),
            small,
        ));
    }
    doc.push(Break::new(0.5));
    push_sections(
        doc,
        &body["holdings"],
        chrome,
        chrome_bold,
        small,
        mono_small,
    );
}

fn push_diff(
    doc: &mut genpdf::Document,
    body: &Value,
    chrome: Style,
    chrome_bold: Style,
    small: Style,
    _mono_body: Style,
    mono_small: Style,
) -> Result<()> {
    let passport = |side: &Value| -> String {
        format!(
            "{}  {}  ({})",
            side["name"].as_str().unwrap_or(""),
            side["id"].as_str().unwrap_or(""),
            side["kind"].as_str().unwrap_or("")
        )
    };
    doc.push(
        Paragraph::default()
            .styled_string(format!("LEFT   {}", passport(&body["left"])), chrome_bold),
    );
    doc.push(
        Paragraph::default()
            .styled_string(format!("RIGHT  {}", passport(&body["right"])), chrome_bold),
    );
    doc.push(Paragraph::default().styled_string(
        format!(
            "Comparing as {} — this view is audited.",
            body["actor_id"].as_str().unwrap_or("")
        ),
        small,
    ));
    doc.push(Break::new(0.5));

    let column = |doc: &mut genpdf::Document, label: String, sections: &Value| {
        // Column emptiness renders as whitespace — no placeholder prose.
        if sections.as_array().map(|s| s.is_empty()).unwrap_or(true) {
            return;
        }
        doc.push(Paragraph::default().styled_string(label, chrome_bold));
        push_sections(doc, sections, chrome, chrome_bold, small, mono_small);
    };
    column(
        doc,
        format!("ONLY {}", body["left"]["name"].as_str().unwrap_or("left")),
        &body["left_only"],
    );
    column(
        doc,
        format!("ONLY {}", body["right"]["name"].as_str().unwrap_or("right")),
        &body["right_only"],
    );

    let shared = body["shared"].as_array().cloned().unwrap_or_default();
    if !shared.is_empty() {
        doc.push(Paragraph::default().styled_string("SHARED".to_string(), chrome_bold));
        // Divergent rows lead, stable within (the service order is
        // document_id ascending).
        let ordered = shared
            .iter()
            .filter(|r| r["divergent_route"].as_bool() == Some(true))
            .chain(
                shared
                    .iter()
                    .filter(|r| r["divergent_route"].as_bool() != Some(true)),
            );
        for row in ordered {
            let reasons = |key: &str| -> Vec<String> {
                row[key]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            };
            let left_primary = diff::primary_reason(&reasons("left_reasons"))?;
            let right_primary = diff::primary_reason(&reasons("right_reasons"))?;
            let mut paragraph = doc_row_paragraph(&row["doc"], mono_small, chrome, small);
            paragraph.push_styled(format!("   [{left_primary} | {right_primary}]"), mono_small);
            doc.push(paragraph);
        }
    }
    Ok(())
}

fn push_atlas_capability(
    doc: &mut genpdf::Document,
    body: &Value,
    chrome: Style,
    chrome_bold: Style,
    mono_body: Style,
    mono_small: Style,
) {
    let capability = &body["capability"];
    let mut header = Paragraph::default();
    header.push_styled(
        format!("{}  ", capability["name"].as_str().unwrap_or("")),
        chrome_bold,
    );
    header.push_styled(
        capability["id"].as_str().unwrap_or("").to_string(),
        mono_body,
    );
    doc.push(header);
    doc.push(Break::new(0.5));
    let docs = capability["docs"].as_array().cloned().unwrap_or_default();
    if docs.is_empty() {
        // The em-dash is the entire vocabulary of absence, in print too.
        doc.push(Paragraph::default().styled_string("—".to_string(), chrome));
    } else {
        for row in &docs {
            doc.push(doc_row_paragraph(
                row,
                mono_small,
                chrome,
                Style::new().with_font_size(SIZE_SMALL),
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_ask(
    doc: &mut genpdf::Document,
    body: &Value,
    chrome: Style,
    chrome_bold: Style,
    small: Style,
    _mono_body: Style,
    mono_small: Style,
    serif: Style,
) {
    doc.push(Paragraph::default().styled_string("Answer".to_string(), chrome_bold));
    match body["answer"]["text"].as_str() {
        Some(text) => {
            // The model's voice keeps its own register in print: serif.
            doc.push(Paragraph::default().styled_string(text.to_string(), serif));
            let citations: Vec<String> = body["answer"]["citations"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            if !citations.is_empty() {
                doc.push(
                    Paragraph::default()
                        .styled_string(format!("citations: {}", citations.join(", ")), mono_small),
                );
            }
        }
        None => {
            doc.push(Paragraph::default().styled_string("—".to_string(), chrome));
        }
    }
    doc.push(Break::new(0.5));
    doc.push(Paragraph::default().styled_string("Results".to_string(), chrome_bold));
    for row in body["results"].as_array().into_iter().flatten() {
        doc.push(doc_row_paragraph(row, mono_small, chrome, small));
    }
    doc.push(Break::new(0.5));
    doc.push(Paragraph::default().styled_string(
        format!(
            "retrieval_mode: {}   judge_applied: {}   generation_applied: {}   aggregation_bounded: {}",
            body["retrieval_mode"].as_str().unwrap_or("?"),
            body["judge_applied"],
            body["generation_applied"],
            body["aggregation_bounded"],
        ),
        mono_small,
    ));
}
