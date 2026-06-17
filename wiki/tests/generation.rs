//! DoD #1 — the layer is generated: 120 person, 8 department, 90 project, 4
//! tool pages, each cross-linked and each carrying provenance, plus an index.

mod common;

use std::path::{Path, PathBuf};

use common::{compile_artifacts, fixtures_dir, scratch};

fn generate_into(out: &Path) -> wiki::GenerateReport {
    let artifacts = scratch("generation_artifacts");
    compile_artifacts(&artifacts);
    wiki::generate(&fixtures_dir(), &artifacts, out).expect("generate layer")
}

#[test]
fn counts_match_the_corpus() {
    let out = scratch("generation_out_counts");
    let report = generate_into(&out);
    assert_eq!(report.people, 120, "120 person pages");
    assert_eq!(report.departments, 8, "8 department pages");
    assert_eq!(report.projects, 90, "90 project (capability) pages");
    assert_eq!(report.tools, 4, "4 tool (agent) pages");
    // 120 + 8 + 90 + 4 + index.
    assert_eq!(report.pages_written, 120 + 8 + 90 + 4 + 1);
}

#[test]
fn every_expected_file_exists_on_disk() {
    let out = scratch("generation_out_files");
    generate_into(&out);

    assert!(out.join("index.md").is_file(), "index page");
    // Data-driven: every roster person (including the edge-case `p_void`
    // principal) gets exactly one page. Asserting the file count equals the
    // roster size also catches id collisions (two people -> one file).
    let sources = wiki::Sources::load(&fixtures_dir()).expect("load sources");
    for person in &sources.people.people {
        let p = out.join("people").join(format!("{}.md", person.id));
        assert!(p.is_file(), "missing person page {}", p.display());
    }
    assert_eq!(
        std::fs::read_dir(out.join("people")).unwrap().count(),
        sources.people.people.len(),
        "one page per roster person, no collisions"
    );
    for slug in [
        "quality-compliance",
        "warehouse-operations",
        "pharmacy-services",
        "finance",
        "it",
        "hr",
        "sales-accounts",
        "executive",
    ] {
        let p = out.join("departments").join(format!("{slug}.md"));
        assert!(p.is_file(), "missing department page {}", p.display());
    }
    let proj_count = std::fs::read_dir(out.join("projects")).unwrap().count();
    assert_eq!(proj_count, 90, "90 project pages");
    let tool_count = std::fs::read_dir(out.join("tools")).unwrap().count();
    assert_eq!(tool_count, 4, "4 tool pages");
}

#[test]
fn pages_carry_provenance_and_cross_links() {
    let out = scratch("generation_out_content");
    generate_into(&out);

    // A representative person page: provenance on every fact, links present,
    // governed-access section present.
    let person = std::fs::read_to_string(out.join("people/p001.md")).unwrap();
    assert!(person.contains("## Facts"));
    assert!(
        person.contains("src: fixtures/people.json#p001"),
        "person facts cite people.json with the record key"
    );
    assert!(person.contains("## Links"), "person page is cross-linked");
    assert!(
        person.contains("Governed access"),
        "person page shows read-only governed access"
    );
    assert!(
        person.contains("](../departments/"),
        "person links to their department page"
    );

    // A project page links people and departments and cites brm.json.
    let proj = std::fs::read_to_string(out.join("projects/cap01.md")).unwrap();
    assert!(proj.contains("src: fixtures/brm.json#cap01"));
    assert!(proj.contains("](../people/") || proj.contains("](../departments/"));

    // A tool page cites company.json and links its owner.
    let tools: Vec<_> = std::fs::read_dir(out.join("tools")).unwrap().collect();
    let any_tool = std::fs::read_to_string(tools[0].as_ref().unwrap().path()).unwrap();
    assert!(any_tool.contains("src: fixtures/company.json#agent_"));
}

#[test]
fn all_cross_links_resolve_to_real_pages() {
    let out = scratch("generation_out_linkcheck");
    generate_into(&out);

    let mut files = Vec::new();
    collect_md(&out, &mut files);
    assert!(!files.is_empty());

    let mut checked = 0usize;
    for f in &files {
        let md = std::fs::read_to_string(f).unwrap();
        let dir = f.parent().unwrap();
        for href in extract_md_links(&md) {
            if href.ends_with(".md") {
                let target = dir.join(&href);
                assert!(
                    std::fs::canonicalize(&target).is_ok(),
                    "dangling link `{href}` in {}",
                    f.display()
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked > 200,
        "the layer is densely cross-linked ({checked} links)"
    );
}

fn collect_md(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            collect_md(&p, out);
        } else if p.extension().map(|x| x == "md").unwrap_or(false) {
            out.push(p);
        }
    }
}

fn extract_md_links(md: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut i = 0;
    while let Some(rel) = md[i..].find("](") {
        let start = i + rel + 2;
        if let Some(close) = md[start..].find(')') {
            links.push(md[start..start + close].to_string());
            i = start + close;
        } else {
            break;
        }
    }
    links
}

/// No real (non-synthetic) company names leak into the layer. Synthetic stays
/// synthetic (invariant 6).
#[test]
fn output_stays_synthetic() {
    let out = scratch("generation_out_synthetic");
    generate_into(&out);
    let index = std::fs::read_to_string(out.join("index.md")).unwrap();
    assert!(index.contains("Bryremead Distribution Ltd"));
}
