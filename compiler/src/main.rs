//! CLI for the M1 scope compiler.
//!
//! ```text
//! scope-compiler compile --fixtures ../fixtures --out artifacts/ [--principal ID]...
//! scope-compiler verify  --fixtures ../fixtures --artifacts artifacts/
//! ```
//!
//! Exit code 0 only when every requested step succeeded; any refusal
//! (schema/parse failure, duplicate or dangling reference, snapshot mismatch)
//! exits nonzero. An unknown principal id is NOT a refusal: it compiles to an
//! empty allowlist (deny-by-default), is logged, and exits 0.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use scope_compiler::{compile, load_world, snapshot};

#[derive(Parser)]
#[command(
    name = "scope-compiler",
    about = "Enterprise Brain M1: compiles principals' permission snapshots into pinned, reason-traced allowlists"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile per-principal allowlist artifacts from fixtures.
    Compile {
        /// Directory holding company.json, documents.json, traps.json.
        #[arg(long)]
        fixtures: PathBuf,
        /// Output directory for <principal_id>.json artifacts + index.json.
        #[arg(long)]
        out: PathBuf,
        /// Compile only these principal ids (repeatable). Default: all.
        #[arg(long = "principal", value_name = "ID")]
        principals: Vec<String>,
    },
    /// Verify a compiled artifact directory against a fixture directory.
    Verify {
        /// Directory holding the fixtures the artifacts claim to pin.
        #[arg(long)]
        fixtures: PathBuf,
        /// Directory holding index.json and the artifacts.
        #[arg(long)]
        artifacts: PathBuf,
    },
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Compile {
            fixtures,
            out,
            principals,
        } => {
            // Pin the input bytes first; everything compiled below is bound
            // to exactly this snapshot.
            let snap = snapshot::take(&fixtures)?;
            let world = load_world(&fixtures)?;
            let requested = if principals.is_empty() {
                None
            } else {
                Some(principals.as_slice())
            };
            let (set, unknown) = compile::compile_set(&world, &snap, requested)?;
            for id in &unknown {
                eprintln!("unknown principal {id:?}: compiled empty allowlist (deny-by-default)");
            }
            // Re-verify before persisting: if the fixture bytes moved while
            // we compiled, the artifacts describe nothing real — refuse.
            snapshot::verify_unchanged(&fixtures, &snap)?;
            compile::write_artifacts(&out, &set)?;
            println!(
                "compiled {} principal(s) x {} document(s): {} allow entries, {} denials",
                set.index.totals.principals,
                set.index.totals.documents,
                set.index.totals.allow_entries,
                set.index
                    .principals
                    .iter()
                    .map(|r| r.denied_count)
                    .sum::<usize>(),
            );
            println!("snapshot_version {}", set.snapshot.snapshot_version);
            println!("artifacts written to {}", out.display());
            Ok(())
        }
        Command::Verify {
            fixtures,
            artifacts,
        } => {
            let index = compile::verify_artifacts(&artifacts, &fixtures)?;
            println!(
                "VERIFIED: {} artifact(s) match snapshot_version {}",
                index.principals.len(),
                index.snapshot_version
            );
            Ok(())
        }
    }
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
