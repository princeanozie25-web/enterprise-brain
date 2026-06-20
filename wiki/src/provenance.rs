//! Provenance is the spine of the knowledge layer, not decoration.
//!
//! INVARIANT (by construction): a [`Claim`] cannot be authored without a
//! [`Provenance`]. `Claim` owns a `Provenance` by value and exposes no
//! constructor that omits it, so "every factual claim cites a source span" is
//! enforced by the type system — code that tries to emit an unsourced claim
//! does not compile. `Provenance::new` additionally refuses an empty source,
//! record key, or locator at run time (fail-closed), so a claim whose
//! provenance is blank is rejected rather than written.

use std::fmt;

use serde::Serialize;

/// A pointer to exactly where a claim was derived from in a raw synthetic
/// source. `source` names the file (e.g. `fixtures/people.json`), `record` is
/// the record key within it (e.g. `p001`, `d0001`, `cap01`), and `locator` is
/// a precise span into the structured record — a JSON pointer such as
/// `/people/0/title`, optionally carrying the 1-based line of the record
/// anchor in the raw file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Provenance {
    pub source: String,
    pub record: String,
    pub locator: String,
    /// 1-based line of the record's `"id"` anchor in the raw source, when it
    /// could be resolved. `None` never weakens the cite — `source` + `record`
    /// + `locator` already pin it — it only adds a human-friendly anchor.
    pub line: Option<usize>,
}

impl Provenance {
    /// Builds a provenance pointer, refusing (fail-closed) any blank component.
    /// A claim whose source, record, or locator is empty has, in effect, no
    /// cite — so it is rejected here rather than allowed to reach a page.
    pub fn new(
        source: impl Into<String>,
        record: impl Into<String>,
        locator: impl Into<String>,
        line: Option<usize>,
    ) -> Result<Self, ProvenanceError> {
        let source = source.into();
        let record = record.into();
        let locator = locator.into();
        if source.trim().is_empty() {
            return Err(ProvenanceError::EmptySource);
        }
        if record.trim().is_empty() {
            return Err(ProvenanceError::EmptyRecord);
        }
        if locator.trim().is_empty() {
            return Err(ProvenanceError::EmptyLocator);
        }
        Ok(Self {
            source,
            record,
            locator,
            line,
        })
    }

    /// A compact human-readable cite, e.g. `fixtures/people.json#p001 /people/0/title (L77)`.
    pub fn cite(&self) -> String {
        match self.line {
            Some(line) => format!(
                "{}#{} {} (L{})",
                self.source, self.record, self.locator, line
            ),
            None => format!("{}#{} {}", self.source, self.record, self.locator),
        }
    }
}

impl fmt::Display for Provenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.cite())
    }
}

/// Why a provenance pointer was refused. Each variant means the same thing in
/// the end: there is no usable source span, so no claim may be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceError {
    EmptySource,
    EmptyRecord,
    EmptyLocator,
}

impl fmt::Display for ProvenanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self {
            ProvenanceError::EmptySource => "source",
            ProvenanceError::EmptyRecord => "record key",
            ProvenanceError::EmptyLocator => "locator/span",
        };
        write!(
            f,
            "provenance refused: empty {what} — a claim without a source span is not written"
        )
    }
}

impl std::error::Error for ProvenanceError {}

/// One factual statement on a page, bound to the source it was derived from.
///
/// `Claim` has no field-omitting constructor and its fields are private: the
/// only way to make one is [`Claim::new`], which demands a `Provenance`. There
/// is therefore no representable "claim without provenance" anywhere in the
/// crate. Renderers consume `Claim`s, so every rendered line is sourced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Claim {
    text: String,
    provenance: Provenance,
}

impl Claim {
    /// Authors a claim. The `Provenance` argument is mandatory — this is the
    /// single choke point that makes unsourced claims unrepresentable.
    pub fn new(text: impl Into<String>, provenance: Provenance) -> Self {
        Self {
            text: text.into(),
            provenance,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_refuses_blank_components() {
        assert_eq!(
            Provenance::new("", "p001", "/people/0", None),
            Err(ProvenanceError::EmptySource)
        );
        assert_eq!(
            Provenance::new("fixtures/people.json", "  ", "/people/0", None),
            Err(ProvenanceError::EmptyRecord)
        );
        assert_eq!(
            Provenance::new("fixtures/people.json", "p001", "\t", None),
            Err(ProvenanceError::EmptyLocator)
        );
    }

    #[test]
    fn good_provenance_round_trips_into_a_claim() {
        let p = Provenance::new("fixtures/people.json", "p001", "/people/0/title", Some(77))
            .expect("valid provenance");
        let c = Claim::new("Title: Head of Quality & Compliance", p);
        assert!(!c.text().is_empty());
        assert_eq!(c.provenance().record, "p001");
        assert_eq!(c.provenance().line, Some(77));
        assert_eq!(
            c.provenance().cite(),
            "fixtures/people.json#p001 /people/0/title (L77)"
        );
    }
}
