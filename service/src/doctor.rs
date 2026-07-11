//! S5a: `gateway doctor` — the anti-F1 preflight. It turns
//! misconfiguration-class failures (the ones that otherwise surface as
//! silent runtime 401s or all-deny surfaces) into named, fixable findings
//! BEFORE the server starts.
//!
//! TWO LAWS:
//!   1. READ-ONLY. Doctor makes no network calls, mutates no state, and
//!      never runs on the request path. It inspects the deployment and
//!      reports; it changes nothing.
//!   2. THE WIRE STAYS MUTE; THE OPERATOR IS TOLD LOUDLY. Doctor's findings
//!      go to stdout — this is the operator surface, the counterpart to the
//!      gateway's deliberately-generic wire. A misconfiguration a running
//!      gateway would hide behind a 401 is here a named ✗ with the exact
//!      fix. Secrets, tokens, and key material are NEVER printed.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::agent::proposals::{verify_ledger, LedgerVerification};
use crate::agent_bridge::AgentBridgeConfig;
use crate::estate::EstateModel;
use crate::{AppState, ServiceConfig};

/// One preflight check result.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    /// A human line: on ✗, the exact fix; on ✓, what was verified. Never a
    /// secret.
    pub detail: String,
}

impl Check {
    fn ok(name: &str, detail: impl Into<String>) -> Check {
        Check {
            name: name.to_string(),
            ok: true,
            detail: detail.into(),
        }
    }
    fn fail(name: &str, detail: impl Into<String>) -> Check {
        Check {
            name: name.to_string(),
            ok: false,
            detail: detail.into(),
        }
    }
}

/// The full preflight report.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub checks: Vec<Check>,
}

impl DoctorReport {
    pub fn all_ok(&self) -> bool {
        self.checks.iter().all(|c| c.ok)
    }

    /// Human-readable ticks — the default operator surface.
    pub fn to_human(&self) -> String {
        let mut out = String::from("gateway doctor — preflight\n");
        for check in &self.checks {
            let mark = if check.ok { "\u{2713}" } else { "\u{2717}" };
            out.push_str(&format!("  {mark} {}: {}\n", check.name, check.detail));
        }
        out.push_str(if self.all_ok() {
            "\nall checks passed.\n"
        } else {
            "\nsome checks failed — fix the ✗ items above.\n"
        });
        out
    }

    /// Machine-consumable form.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "ok": self.all_ok(),
            "checks": self.checks,
        }))
        .unwrap_or_else(|_| "{\"ok\":false}".to_string())
    }
}

/// What the doctor inspects — mirrors the server's launch inputs so the
/// preflight sees exactly what the server would.
pub struct DoctorInputs {
    pub fixtures: PathBuf,
    pub artifacts: PathBuf,
    pub idx: PathBuf,
    pub config: Option<PathBuf>,
    pub state_dir: Option<PathBuf>,
}

/// Run every preflight check and return the report. Never panics; a failing
/// check is a ✗, not an error. (The only hard error is inputs so broken the
/// corpus itself cannot load — reported as a ✗ too.)
pub fn run(inputs: &DoctorInputs) -> DoctorReport {
    let mut checks = Vec::new();

    // 1. Config parses + every present section schema-valid.
    let config = match &inputs.config {
        None => {
            checks.push(Check::ok(
                "config",
                "no --config given; running with defaults",
            ));
            None
        }
        Some(path) => match ServiceConfig::load(path) {
            Ok(config) => {
                checks.push(Check::ok(
                    "config",
                    format!("{} parses; all sections schema-valid", path.display()),
                ));
                Some(config)
            }
            Err(err) => {
                checks.push(Check::fail(
                    "config",
                    format!(
                        "{} is invalid: {err:#} — fix the named field",
                        path.display()
                    ),
                ));
                None
            }
        },
    };

    // The corpus/identity model (loading it also proves fixtures+artifacts).
    let state = match AppState::build(&inputs.fixtures, &inputs.artifacts, &inputs.idx) {
        Ok(state) => Some(state),
        Err(err) => {
            checks.push(Check::fail(
                "corpus",
                format!("cannot load the compiled corpus: {err:#}"),
            ));
            None
        }
    };

    // 2. Ledger dir writable + (if a file exists) chain-verifies.
    if let Some(config) = &config {
        if let Some(ledger) = &config.ledger {
            checks.push(check_ledger("ledger", &ledger.dir, "audit.jsonl"));
        }
    }

    // 3. Agent bridge (if enabled): JWKS loadable, registry non-empty,
    //    every registered principal resolvable (no S0 ghost).
    if let Some(config) = &config {
        if let Some(bridge) = &config.agent_bridge {
            if bridge.enabled {
                checks.extend(check_bridge(bridge, state.as_ref(), &inputs.fixtures));
            } else {
                checks.push(Check::ok(
                    "bridge",
                    "present but disabled; /v1 will not serve",
                ));
            }
        }
    }

    // 4. Estate: sources present, hashes verify, index buildable.
    let estate_dir = config
        .as_ref()
        .and_then(|c| c.estate_dir.clone())
        .unwrap_or_else(|| inputs.fixtures.join("estate"));
    if estate_dir.join("s3-access.json").exists() {
        checks.push(check_estate(&estate_dir, state.as_ref()));
    }

    // 5. Alerting (if enabled): sink path writable, webhook URL well-formed.
    if let Some(config) = &config {
        if let Some(alerting) = &config.alerting {
            if alerting.enabled {
                checks.push(check_alert_sink(&alerting.alerts_path));
                if let Some(url) = &alerting.webhook_url {
                    checks.push(check_url("alerting.webhook_url", url));
                }
            } else {
                checks.push(Check::ok("alerting", "present but disabled"));
            }
        }
    }

    // 6. Workflow store (if a state dir is wired): dir writable, chain OK.
    if let Some(state_dir) = &inputs.state_dir {
        if state_dir.join("wf_proposals.jsonl").exists() {
            checks.push(check_ledger(
                "workflow_store",
                state_dir,
                "wf_proposals.jsonl",
            ));
        }
    }

    DoctorReport { checks }
}

fn check_ledger(name: &str, dir: &Path, filename: &str) -> Check {
    if !dir.exists() {
        return Check::fail(
            name,
            format!("{} does not exist — create the ledger dir", dir.display()),
        );
    }
    if !writable(dir) {
        return Check::fail(
            name,
            format!("{} is not writable — fix its permissions", dir.display()),
        );
    }
    let path = dir.join(filename);
    if !path.exists() {
        return Check::ok(
            name,
            format!("{} writable; no {filename} yet (fresh)", dir.display()),
        );
    }
    match verify_ledger(&path) {
        Ok(LedgerVerification::Clean { rows, chained_rows }) => Check::ok(
            name,
            format!(
                "{} writable; {filename} verifies CLEAN ({rows} rows, {chained_rows} chained)",
                dir.display()
            ),
        ),
        Ok(LedgerVerification::Broken { ordinal, detail }) => Check::fail(
            name,
            format!(
                "{filename} chain BREAKS at ordinal {ordinal} ({detail}) — investigate tampering"
            ),
        ),
        Err(err) => Check::fail(name, format!("cannot verify {filename}: {err:#}")),
    }
}

fn check_bridge(
    bridge: &AgentBridgeConfig,
    state: Option<&AppState>,
    fixtures: &Path,
) -> Vec<Check> {
    let mut checks = Vec::new();

    // JWKS source: file exists+parses, or URL well-formed (no live fetch).
    match (&bridge.jwks.file, &bridge.jwks.url) {
        (Some(file), None) => {
            if !file.exists() {
                checks.push(Check::fail(
                    "bridge.jwks",
                    format!("JWKS file {} not found — check the path", file.display()),
                ));
            } else if crate::agent_bridge::jwks::FileJwks::load(file).is_err() {
                checks.push(Check::fail(
                    "bridge.jwks",
                    format!(
                        "JWKS file {} does not parse — check its JSON",
                        file.display()
                    ),
                ));
            } else {
                checks.push(Check::ok(
                    "bridge.jwks",
                    format!("{} loads", file.display()),
                ));
            }
        }
        (None, Some(url)) => checks.push(check_url("bridge.jwks.url", url)),
        _ => checks.push(Check::fail(
            "bridge.jwks",
            "needs exactly one of `file` / `url`",
        )),
    }

    // Registry non-empty + every principal resolvable (no S0 ghost).
    if bridge.agents.is_empty() {
        checks.push(Check::fail(
            "bridge.registry",
            "the agent registry is empty — no agent can authenticate",
        ));
    } else if let Some(state) = state {
        // A principal is resolvable iff the identity model knows it OR it is
        // an estate agent (loaded from fixtures/estate here for the check).
        let estate = EstateModel::load(&fixtures.join("estate")).ok();
        let mut ghosts = Vec::new();
        for agent in &bridge.agents {
            let known = state.identity.is_known(&agent.principal)
                || estate
                    .as_ref()
                    .is_some_and(|e| e.is_estate_agent(&agent.principal));
            if !known {
                ghosts.push(agent.principal.clone());
            }
        }
        if ghosts.is_empty() {
            checks.push(Check::ok(
                "bridge.registry",
                format!(
                    "{} agents registered; all principals resolvable",
                    bridge.agents.len()
                ),
            ));
        } else {
            checks.push(Check::fail(
                "bridge.registry",
                format!(
                    "registration(s) to unknown principal(s) {ghosts:?} — a ghost registration \
                     is a runtime all-deny; register a real principal or remove it"
                ),
            ));
        }
    } else {
        checks.push(Check::fail(
            "bridge.registry",
            "cannot check registry principals — the corpus failed to load",
        ));
    }
    checks
}

fn check_estate(estate_dir: &Path, state: Option<&AppState>) -> Check {
    match EstateModel::load(estate_dir) {
        Err(err) => Check::fail(
            "estate",
            format!(
                "estate at {} failed to load/verify: {err:#}",
                estate_dir.display()
            ),
        ),
        Ok(model) => {
            // The index must build over both sources (needs the corpus).
            let Some(state) = state else {
                return Check::fail(
                    "estate",
                    "estate loads but the corpus did not — cannot build the index",
                );
            };
            let index = state.build_estate_index(&model);
            Check::ok(
                "estate",
                format!(
                    "{} objects verify against the pinned hash; retrieval index builds ({} docs)",
                    model.object_count(),
                    index.doc_count()
                ),
            )
        }
    }
}

fn check_alert_sink(path: &Path) -> Check {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    match parent {
        Some(dir) if !dir.exists() => Check::fail(
            "alerting.sink",
            format!(
                "alert sink dir {} does not exist — create it",
                dir.display()
            ),
        ),
        Some(dir) if !writable(dir) => Check::fail(
            "alerting.sink",
            format!(
                "alert sink dir {} is not writable — fix its permissions",
                dir.display()
            ),
        ),
        _ => Check::ok("alerting.sink", format!("{} is writable", path.display())),
    }
}

fn check_url(name: &str, url: &str) -> Check {
    if url.starts_with("http://") || url.starts_with("https://") {
        // Well-formed enough to POST to; NO live call is made.
        if url.len() > "https://".len() {
            return Check::ok(name, format!("{url} is a well-formed URL (not fetched)"));
        }
    }
    Check::fail(name, format!("{url:?} is not a well-formed http(s) URL"))
}

/// A directory is writable iff we can create + remove a probe file. Read-only
/// discipline: the probe is created and immediately removed; nothing else
/// touches the directory.
fn writable(dir: &Path) -> bool {
    let probe = dir.join(".doctor-write-probe");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}
