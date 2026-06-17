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
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};

use wiki::{generate, generate_scoped};

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
    /// Slice 2: derive per-scope CONTENT knowledge with the LOCAL model, reading
    /// only each scope's authorized documents (via governed retrieval).
    DeriveScoped {
        /// Directory holding people.json, documents.json, company.json, brm.json.
        #[arg(long)]
        fixtures: PathBuf,
        /// Compiled M1 artifact directory. READ-ONLY.
        #[arg(long)]
        artifacts: PathBuf,
        /// Governed retrieval index directory (produced by `retrieval index`). READ-ONLY.
        #[arg(long)]
        idx: PathBuf,
        /// Output directory for the scoped layers + drift report.
        #[arg(long)]
        out: PathBuf,
        /// Comma-separated principal ids to derive scopes for.
        #[arg(long, value_delimiter = ',')]
        scopes: Vec<String>,
        /// Local generation model id (a CONFIG value; never defaulted silently).
        #[arg(long)]
        model: String,
        /// Local model endpoint (loopback only).
        #[arg(long, default_value = "http://127.0.0.1:11434")]
        endpoint: String,
        /// Per-call deadline in milliseconds.
        #[arg(long, default_value_t = 120_000)]
        timeout_ms: u64,
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
        Command::DeriveScoped {
            fixtures,
            artifacts,
            idx,
            out,
            scopes,
            model,
            endpoint,
            timeout_ms,
        } => {
            let report = generate_scoped(
                &fixtures,
                &artifacts,
                &idx,
                &out,
                &scopes,
                &endpoint,
                &model,
                Duration::from_millis(timeout_ms),
            )?;
            println!(
                "scoped derivation with `{}` over {} scope(s):",
                report.model,
                report.scopes.len()
            );
            for s in &report.scopes {
                println!(
                    "  {} — allowed {} docs, {} sourced, {} claims, {} fail-closed flags, {} refused",
                    s.principal_id,
                    s.allowed_count,
                    s.sourced_docs,
                    s.claims,
                    s.fail_closed_flags,
                    s.refused
                );
            }
            println!(
                "structural (slice-1) flags: {}; pages written: {}",
                report.structural_flags, report.pages_written
            );
            println!("scoped layers written to {}", out.display());
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
