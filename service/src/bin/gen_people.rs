//! AR-1 generator — writes `fixtures/people.json` deterministically from the
//! FROZEN skeleton plus the service's own startup Lane derivation. Re-runnable
//! and byte-identical every time (the AR-suite proves it). It never touches
//! company.json or any other M1 input; the humanization layer is a new,
//! separate file.
//!
//! ```text
//! cargo run --bin gen_people \
//!     [--fixtures fixtures] [--artifacts compiler/artifacts] [--idx retrieval/idx]
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};

use service::{humanize, AppState};

fn run() -> Result<()> {
    let mut fixtures = PathBuf::from("fixtures");
    let mut artifacts = PathBuf::from("compiler/artifacts");
    let mut idx = PathBuf::from("retrieval/idx");

    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = |name: &str| -> Result<PathBuf> {
            args.next()
                .map(PathBuf::from)
                .with_context(|| format!("{name} requires a value"))
        };
        match flag.as_str() {
            "--fixtures" => fixtures = value("--fixtures")?,
            "--artifacts" => artifacts = value("--artifacts")?,
            "--idx" => idx = value("--idx")?,
            other => bail!("unknown flag {other:?}"),
        }
    }

    // Build state WITHOUT `with_people` — no circular dependency: the layer is
    // derived FROM this state's frozen inputs and lane seeds, then written.
    let state = AppState::build(&fixtures, &artifacts, &idx)?;
    let inputs = humanize::read_person_inputs(&fixtures, &state.company_sha256)?;
    let file = humanize::generate(&inputs, &state.lane_seeds);
    let bytes = humanize::to_pretty_bytes(&file)?;

    let out = fixtures.join("people.json");
    std::fs::write(&out, &bytes).with_context(|| format!("cannot write {}", out.display()))?;

    let with_projects = file.people.iter().filter(|p| !p.projects.is_empty()).count();
    eprintln!(
        "gen_people: wrote {} — {} principals, {} with projects, {} with none, {} bytes",
        out.display(),
        file.people.len(),
        with_projects,
        file.people.len() - with_projects,
        bytes.len(),
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("REFUSED: {err:#}");
            ExitCode::FAILURE
        }
    }
}
