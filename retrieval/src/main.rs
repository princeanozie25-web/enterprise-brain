//! CLI for M2 governed retrieval.
//!
//! ```text
//! retrieval index --fixtures fixtures --out idx/ [--hybrid --config cfg.json]
//! retrieval query --principal <id> --artifacts <m1-artifacts> --idx idx/
//!                 --q "<text>" [--include-superseded] [--k N]
//!                 [--hybrid] [--judge] [--config cfg.json] [--usage-out u.jsonl]
//! ```
//!
//! `query` prints exactly one canonical envelope JSON line to stdout and
//! nothing else. Instrumentation traces stay in memory; no out-of-scope
//! document id is ever emitted, logged, or printed — including in errors.
//! `--hybrid`/`--judge` require `--config` with explicit model ids: a
//! missing model id refuses rather than defaulting silently.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use retrieval::embed::OllamaEmbeddings;
use retrieval::index::{build_index, TOP_K_DEFAULT};
use retrieval::judge::OllamaJudge;
use retrieval::local_llm::{append_usage_sidecar, LocalLlmClient, RuntimeConfig};
use retrieval::search::{Engine, HybridParams, JudgeParams, PrincipalScope, SearchOptions};
use retrieval::vector::build_vectors;

#[derive(Parser)]
#[command(
    name = "retrieval",
    about = "Enterprise Brain M2: BM25 + vector retrieval strictly inside M1 compiled allowlists"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build the five per-sensitivity-class partitions from fixtures.
    Index {
        /// Directory holding documents.json.
        #[arg(long)]
        fixtures: PathBuf,
        /// Output directory for the partitions + manifest.json (must be empty).
        #[arg(long)]
        out: PathBuf,
        /// Also embed the corpus and store per-partition vectors. An
        /// embedder failure FAILS the build (an index silently missing
        /// vectors is a lie).
        #[arg(long)]
        hybrid: bool,
        /// Runtime config (endpoint, model ids, timeouts). Required by
        /// --hybrid.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Run one governed query for one principal.
    Query {
        /// Principal id whose compiled allowlist governs the query.
        #[arg(long)]
        principal: String,
        /// M1 artifacts directory (index.json + per-principal artifacts).
        #[arg(long)]
        artifacts: PathBuf,
        /// Index directory produced by `retrieval index`.
        #[arg(long)]
        idx: PathBuf,
        /// Query text (treated as plain terms; no query syntax).
        #[arg(long)]
        q: String,
        /// Serve superseded documents too (always marked as such).
        #[arg(long)]
        include_superseded: bool,
        /// Top-k results (1..=50).
        #[arg(long, default_value_t = TOP_K_DEFAULT)]
        k: usize,
        /// Add the vector RankSource (degrades to lexical_only if the local
        /// embedder is unreachable).
        #[arg(long)]
        hybrid: bool,
        /// Let a local judge reorder the fused top-K (elided/failed judges
        /// leave the fused order standing).
        #[arg(long)]
        judge: bool,
        /// Runtime config (endpoint, model ids, timeouts). Required by
        /// --hybrid and --judge.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Append one JSONL metering row per judge/embedder call (model +
        /// token numbers only, never content).
        #[arg(long)]
        usage_out: Option<PathBuf>,
    },
}

fn load_config(config: &Option<PathBuf>, needed_by: &str) -> Result<RuntimeConfig> {
    let path = config
        .as_ref()
        .with_context(|| format!("{needed_by} requires --config"))?;
    RuntimeConfig::load(path)
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Index {
            fixtures,
            out,
            hybrid,
            config,
        } => {
            let manifest = build_index(&fixtures, &out)?;
            let manifest = if hybrid {
                let config = load_config(&config, "--hybrid")?;
                let (model, dim) = config.require_embed_model()?;
                let client = LocalLlmClient::new(&config.endpoint)?;
                let embedder = OllamaEmbeddings::new(client, model, dim);
                build_vectors(
                    &fixtures,
                    &out,
                    &embedder,
                    Duration::from_millis(config.timeouts_ms.index_embed_per_batch),
                    config.judge_elision.snippet_chars,
                )?
            } else {
                manifest
            };
            println!("index_version {}", manifest.index_version);
            println!(
                "partitions written to {} ({})",
                out.display(),
                if manifest.vectors.is_some() {
                    "with vectors"
                } else {
                    "lexical only"
                }
            );
            Ok(())
        }
        Command::Query {
            principal,
            artifacts,
            idx,
            q,
            include_superseded,
            k,
            hybrid,
            judge,
            config,
            usage_out,
        } => {
            if usage_out.is_some() && !hybrid && !judge {
                bail!("--usage-out meters embedder/judge calls; nothing would be metered");
            }
            let engine = Engine::open(&idx)?;
            let scope = PrincipalScope::load(&artifacts, &principal)?;

            // Constructed up front so they outlive the search options.
            let mut embedder = None;
            let mut judge_impl = None;
            let mut runtime = None;
            if hybrid || judge {
                let config = load_config(&config, "--hybrid/--judge")?;
                if hybrid {
                    let (model, dim) = config.require_embed_model()?;
                    let client = LocalLlmClient::new(&config.endpoint)?;
                    embedder = Some(OllamaEmbeddings::new(client, model, dim));
                }
                if judge {
                    let model = config.require_judge_model()?.to_string();
                    let client = LocalLlmClient::new(&config.endpoint)?;
                    judge_impl = Some(OllamaJudge::new(client, &model));
                }
                runtime = Some(config);
            }

            let options = SearchOptions {
                k,
                include_superseded,
                hybrid: embedder.as_ref().map(|e| HybridParams {
                    embedder: e,
                    query_embed_timeout: Duration::from_millis(
                        runtime
                            .as_ref()
                            .expect("config loaded")
                            .timeouts_ms
                            .query_embed,
                    ),
                }),
                judge: judge_impl.as_ref().map(|j| {
                    let config = runtime.as_ref().expect("config loaded");
                    JudgeParams {
                        judge: j,
                        timeout: Duration::from_millis(config.timeouts_ms.judge),
                        top_k: config.judge_elision.top_k,
                        min_candidates: config.judge_elision.min_candidates,
                        max_ratio: config.judge_elision.max_top1_top2_ratio,
                    }
                }),
            };

            let (envelope, trace) = engine.search(&scope, &q, &options)?;
            if let Some(usage_path) = &usage_out {
                append_usage_sidecar(usage_path, &trace.usage_events)?;
            }
            let bytes = envelope.to_canonical_bytes()?;
            print!(
                "{}",
                String::from_utf8(bytes).expect("canonical JSON is UTF-8")
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
