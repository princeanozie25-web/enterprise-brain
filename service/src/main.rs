//! The Ask Brain service binary. Binds 127.0.0.1:8787 ONLY.
//!
//! ```text
//! service --fixtures fixtures --artifacts compiler/artifacts --idx retrieval/idx
//!         [--config service/config.example.json] [--usage-out usage.jsonl]
//!         [--no-cache]
//! ```
//!
//! Flags are parsed by hand: the service's dependency surface stays exactly
//! the async stack the milestone allows.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{bail, Context, Result};

use service::{app, loopback_listener, AppState, ServiceConfig, BIND_ADDR};

struct Args {
    fixtures: PathBuf,
    artifacts: PathBuf,
    idx: PathBuf,
    config: Option<PathBuf>,
    usage_out: Option<PathBuf>,
    no_cache: bool,
    agents_config: Option<PathBuf>,
    state_dir: Option<PathBuf>,
}

fn parse_args() -> Result<Args> {
    let mut fixtures = None;
    let mut artifacts = None;
    let mut idx = None;
    let mut config = None;
    let mut usage_out = None;
    let mut no_cache = false;
    let mut agents_config = None;
    let mut state_dir = None;

    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut path_value = |name: &str| -> Result<PathBuf> {
            args.next()
                .map(PathBuf::from)
                .with_context(|| format!("{name} requires a value"))
        };
        match flag.as_str() {
            "--fixtures" => fixtures = Some(path_value("--fixtures")?),
            "--artifacts" => artifacts = Some(path_value("--artifacts")?),
            "--idx" => idx = Some(path_value("--idx")?),
            "--config" => config = Some(path_value("--config")?),
            "--usage-out" => usage_out = Some(path_value("--usage-out")?),
            "--no-cache" => no_cache = true,
            "--agents-config" => agents_config = Some(path_value("--agents-config")?),
            "--state-dir" => state_dir = Some(path_value("--state-dir")?),
            other => bail!("unknown flag {other:?}"),
        }
    }
    if agents_config.is_some() != state_dir.is_some() {
        bail!("--agents-config and --state-dir must be given together");
    }
    Ok(Args {
        fixtures: fixtures.context("--fixtures is required")?,
        artifacts: artifacts.context("--artifacts is required")?,
        idx: idx.context("--idx is required")?,
        config,
        usage_out,
        no_cache,
        agents_config,
        state_dir,
    })
}

fn build_state(args: &Args) -> Result<AppState> {
    let mut state = AppState::build(&args.fixtures, &args.artifacts, &args.idx)?;
    if let Some(config_path) = &args.config {
        let config = ServiceConfig::load(config_path)?;
        state = config.apply(state)?;
    }
    if args.no_cache {
        state = state.with_cache(None);
    }
    state = state.with_usage_out(args.usage_out.clone());
    if let (Some(agents_config), Some(state_dir)) = (&args.agents_config, &args.state_dir) {
        let registry = service::agent::standing::AgentRegistry::load(
            agents_config,
            &args.fixtures,
            &state.company_sha256,
        )?;
        let store = service::agent::proposals::ProposalStore::open(state_dir)?;
        // AP-6: the lane's box store shares the state dir.
        let boxes = service::lane::BoxStore::open(state_dir)?;
        state = state
            .with_agents(registry)
            .with_proposals(std::sync::Arc::new(store))
            .with_lane_boxes(std::sync::Arc::new(boxes));
    }
    Ok(state)
}

async fn run() -> Result<()> {
    let args = parse_args()?;
    let state = Arc::new(build_state(&args)?);
    eprintln!(
        "ask-brain: {} principals, {} docs, cache={}, embedder={}, judge={}, generator={}",
        state.identity_count(),
        state.docs.len(),
        state.cache.is_some(),
        state.embedder.is_some(),
        state.judge.is_some(),
        state.generator.is_some(),
    );

    let listener = loopback_listener(BIND_ADDR)?;
    let listener = tokio::net::TcpListener::from_std(listener).context("tokio listener")?;
    eprintln!("ask-brain: serving on http://{BIND_ADDR} (demo identity mode)");
    axum::serve(listener, app(state))
        .await
        .context("server error")?;
    Ok(())
}

fn main() -> ExitCode {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("REFUSED: cannot start runtime: {err}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("REFUSED: {err:#}");
            ExitCode::FAILURE
        }
    }
}
