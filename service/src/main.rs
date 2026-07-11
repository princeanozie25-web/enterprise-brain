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
    parse_args_from(std::env::args().skip(1).collect())
}

fn parse_args_from(argv: Vec<String>) -> Result<Args> {
    let mut fixtures = None;
    let mut artifacts = None;
    let mut idx = None;
    let mut config = None;
    let mut usage_out = None;
    let mut no_cache = false;
    let mut agents_config = None;
    let mut state_dir = None;

    let mut args = argv.into_iter();
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
    if agents_config.is_some() && state_dir.is_none() {
        bail!("--agents-config requires --state-dir");
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
    // AR-1: load + prove the humanization layer (display only; fails closed
    // if people.json disagrees with what the live skeleton derives).
    state = state.with_people()?;
    // S3: load the multi-source estate when the fixtures ship one
    // (fixtures/estate/). Absent, the world stays single-source. A config
    // `estate_dir` overrides this default location.
    state = state.with_estate_from(&args.fixtures.join("estate"))?;
    if let Some(config_path) = &args.config {
        let config = ServiceConfig::load(config_path)?;
        state = config.apply(state)?;
    }
    if args.no_cache {
        state = state.with_cache(None);
    }
    state = state.with_usage_out(args.usage_out.clone());
    if let Some(state_dir) = &args.state_dir {
        let access_requests = service::access_requests::AccessRequestStore::open(state_dir)?;
        let access_grants = service::access_grants::AccessGrantStore::open(state_dir)?;
        state = state.with_access_requests(std::sync::Arc::new(access_requests));
        state = state.with_access_grants(std::sync::Arc::new(access_grants));
    }
    if let (Some(agents_config), Some(state_dir)) = (&args.agents_config, &args.state_dir) {
        let registry = service::agent::standing::AgentRegistry::load(
            agents_config,
            &args.fixtures,
            &state.company_sha256,
        )?;
        // AP-6: the lane's box store shares the state dir.
        let boxes = service::lane::BoxStore::open(state_dir)?;
        // SHOWCASE-III: the grounded-workflow proposal store shares the state dir
        // (distinct wf_proposals.jsonl, so it never collides with M4's store).
        // wf-gen S4 condition: the mutation ledger is CHAINED (tamper-evident,
        // timestamped) — approval records are the most tamper-sensitive rows,
        // so a production deployment gets tamper-evidence by construction.
        let wf_clock: std::sync::Arc<dyn service::clock::Clock> =
            std::sync::Arc::new(service::clock::WallClock);
        let wf_proposals =
            service::proposals::WorkflowProposalStore::open_chained(state_dir, wf_clock)?;
        state = state
            .with_agents(registry)
            .with_lane_boxes(std::sync::Arc::new(boxes))
            // SHOWCASE-III: the grounded-workflow proposal store is additive.
            .with_wf_proposals(std::sync::Arc::new(wf_proposals));
        // S2b (invariant wins): a config-wired ledger (config.ledger.dir) is
        // THE decision ledger when present — M4 opens its own from
        // --state-dir only when config supplied none. One ledger either way.
        if state.proposals.is_none() {
            let store = service::agent::proposals::ProposalStore::open(state_dir)?;
            state = state.with_proposals(std::sync::Arc::new(store));
        }
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

/// S4: `service verify-ledger <path>` — walk a ledger and recompute its
/// hash chain, reporting CLEAN or the first breaking ordinal. Exits 0 on
/// CLEAN, 1 on a broken chain (so it slots into CI / an operator check).
fn verify_ledger_cmd(path: &str) -> ExitCode {
    use service::agent::proposals::{verify_ledger, LedgerVerification};
    match verify_ledger(std::path::Path::new(path)) {
        Ok(LedgerVerification::Clean { rows, chained_rows }) => {
            println!("CLEAN: {rows} rows ({chained_rows} hash-chained) verify intact");
            ExitCode::SUCCESS
        }
        Ok(LedgerVerification::Broken { ordinal, detail }) => {
            eprintln!("BROKEN: chain breaks at ordinal {ordinal} ({detail})");
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!("REFUSED: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// S5a: `service doctor [--json]` + the usual launch flags — a read-only
/// preflight over the deployment. Exits 0 all-green / 1 otherwise. Never
/// mutates state, never makes a network call.
fn doctor_cmd(args: &Args, json: bool) -> ExitCode {
    let inputs = service::doctor::DoctorInputs {
        fixtures: args.fixtures.clone(),
        artifacts: args.artifacts.clone(),
        idx: args.idx.clone(),
        config: args.config.clone(),
        state_dir: args.state_dir.clone(),
    };
    let report = service::doctor::run(&inputs);
    if json {
        println!("{}", report.to_json());
    } else {
        print!("{}", report.to_human());
    }
    if report.all_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn main() -> ExitCode {
    // S4/S5a: subcommands run without the async server.
    let mut raw = std::env::args().skip(1);
    if let Some(first) = raw.next() {
        if first == "verify-ledger" {
            let Some(path) = raw.next() else {
                eprintln!("usage: service verify-ledger <ledger-path>");
                return ExitCode::FAILURE;
            };
            return verify_ledger_cmd(&path);
        }
        if first == "doctor" {
            // Remaining args are the usual launch flags (+ optional --json).
            let rest: Vec<String> = raw.collect();
            let json = rest.iter().any(|a| a == "--json");
            let flags: Vec<String> = rest.into_iter().filter(|a| a != "--json").collect();
            let args = match parse_args_from(flags) {
                Ok(args) => args,
                Err(err) => {
                    eprintln!("REFUSED: {err:#}");
                    return ExitCode::FAILURE;
                }
            };
            return doctor_cmd(&args, json);
        }
    }

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
