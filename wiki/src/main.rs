//! CLI for the org knowledge layer.
//!
//! ```text
//! wiki generate --fixtures ../fixtures --artifacts <dir> --out out/
//! ```
//!
//! `--artifacts` points at a compiled M1 artifact directory (produced by
//! `scope-compiler compile`). The wiki reads it read-only; it never writes
//! there, only under `--out`.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use wiki::generate;

#[derive(Parser)]
#[command(
    name = "wiki",
    about = "Enterprise Brain: derive a provenance-anchored org knowledge layer, firewalled from the authz model"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Derive and write the knowledge layer.
    Generate {
        /// Directory holding people.json, documents.json, company.json, brm.json.
        #[arg(long)]
        fixtures: PathBuf,
        /// Compiled M1 artifact directory (index.json + <principal>.json). READ-ONLY.
        #[arg(long)]
        artifacts: PathBuf,
        /// Output directory for the generated markdown layer.
        #[arg(long)]
        out: PathBuf,
    },
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Generate {
            fixtures,
            artifacts,
            out,
        } => {
            let report = generate(&fixtures, &artifacts, &out)?;
            println!(
                "generated {} page(s): {} people, {} departments, {} projects, {} tools",
                report.pages_written,
                report.people,
                report.departments,
                report.projects,
                report.tools,
            );
            println!(
                "fail-closed flags surfaced (access NOT widened): {}",
                report.fail_closed_flags
            );
            println!("read authz snapshot_version {}", report.snapshot_version);
            println!("layer written to {}", out.display());
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
