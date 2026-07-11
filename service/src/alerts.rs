//! S4: policy-deny alerting. Denied access attempts surface to a security
//! team as structured alerts — "this agent attempted to retrieve something
//! outside its scope."
//!
//! TWO LAWS shape the design:
//!   1. Alerts are PROJECTIONS OF THE LEDGER, never a second decision path.
//!      The deny row is written first (before-effect, unchanged); the alert
//!      is derived from it and traceable back to it by `ledger_ordinal`. The
//!      ledger is the single source of truth.
//!   2. Alerting is NEVER on the decision path. Dispatch spawns off the
//!      request: a slow, failing, or absent sink must not delay, fail, or
//!      alter any request. The synchronous durability the request depends on
//!      is the LEDGER row; the alert file sink is durable but asynchronous.
//!
//! Scope (exactly): POLICY-class denies — a validated principal denied a
//! resource-level decision (out-of-scope / ghost-registration document
//! fetch, `body_exceeds_cap`). Auth-ladder denies (bad/expired/forged
//! tokens) do NOT alert — that is threshold/anomaly territory, fenced.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::clock::Clock;

/// Webhook attempt policy: 3 tries, 3 s per-attempt timeout, 500 ms backoff.
pub const WEBHOOK_ATTEMPTS: u32 = 3;
pub const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(3);
pub const WEBHOOK_BACKOFF: Duration = Duration::from_millis(500);

/// One structured alert — every field the done-criteria require plus the
/// ledger ordinal that makes it traceable to its source-of-truth row.
#[derive(Debug, Clone, Serialize)]
pub struct Alert {
    pub ts: String,
    pub principal_id: String,
    pub claims: AlertClaims,
    pub resource: String,
    pub source: String,
    pub decision_basis: String,
    pub ledger_ordinal: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlertClaims {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub azp: Option<String>,
}

/// The webhook delivery seam. Production posts JSON over HTTP; tests inject
/// a recorder (the "webhook receiver") or a black-hole (to prove the request
/// path is unaffected by a hanging sink).
pub trait WebhookSender: Send + Sync {
    fn send(&self, url: &str, body: &[u8]) -> Result<()>;
}

/// Production webhook: a blocking `ureq` POST with the per-attempt timeout.
/// It runs only inside the off-path dispatch task, never on a request.
pub struct UreqWebhook;

impl WebhookSender for UreqWebhook {
    fn send(&self, url: &str, body: &[u8]) -> Result<()> {
        let response = ureq::post(url)
            .config()
            .timeout_global(Some(WEBHOOK_TIMEOUT))
            .build()
            .header("content-type", "application/json")
            .send(body)
            .with_context(|| format!("webhook POST {url} failed"))?;
        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("webhook POST {url} returned {status}");
        }
        Ok(())
    }
}

/// The alert dispatcher: a file sink (always on) + an optional webhook. It
/// derives an alert's `ts` from the injected clock, so the alert timestamp
/// is deterministic under a test clock (and matches the ledger row's `ts`
/// exactly under a frozen clock).
pub struct AlertDispatcher {
    alerts_path: PathBuf,
    webhook_url: Option<String>,
    webhook: Arc<dyn WebhookSender>,
    clock: Arc<dyn Clock>,
}

/// The raw material a handler hands the dispatcher — everything but the `ts`
/// (which the dispatcher stamps from its clock).
pub struct AlertInput {
    pub principal_id: String,
    pub claims: AlertClaims,
    pub resource: String,
    pub source: String,
    pub decision_basis: String,
    pub ledger_ordinal: u64,
}

impl AlertDispatcher {
    pub fn new(
        alerts_path: PathBuf,
        webhook_url: Option<String>,
        webhook: Arc<dyn WebhookSender>,
        clock: Arc<dyn Clock>,
    ) -> AlertDispatcher {
        AlertDispatcher {
            alerts_path,
            webhook_url,
            webhook,
            clock,
        }
    }

    /// Derive an alert and dispatch it OFF THE REQUEST PATH. Returns
    /// immediately: the file write (fsync) and the webhook attempts run in a
    /// spawned blocking task. A sink that is slow, failing, or absent never
    /// touches the caller.
    pub fn dispatch(&self, input: AlertInput) {
        let alert = Alert {
            ts: self.clock.now_rfc3339_ms(),
            principal_id: input.principal_id,
            claims: input.claims,
            resource: input.resource,
            source: input.source,
            decision_basis: input.decision_basis,
            ledger_ordinal: input.ledger_ordinal,
        };
        let alerts_path = self.alerts_path.clone();
        let webhook_url = self.webhook_url.clone();
        let webhook = self.webhook.clone();
        tokio::task::spawn_blocking(move || {
            // The durable record: append + fsync, same discipline as the
            // ledger — but asynchronously, so it never gates the request.
            if let Err(err) = append_alert(&alerts_path, &alert) {
                eprintln!("alert file sink failed: {err:#}");
            }
            // Best-effort delivery: 3 attempts, 500 ms backoff, then drop
            // with an operator-visible line (the file sink is the record).
            if let Some(url) = &webhook_url {
                deliver_webhook(webhook.as_ref(), url, &alert);
            }
        });
    }
}

/// Append one alert as a JSON line + fsync (the durable sink).
fn append_alert(path: &std::path::Path, alert: &Alert) -> Result<()> {
    use std::io::Write as _;
    let mut bytes = serde_json::to_vec(alert).context("alert serialization")?;
    bytes.push(b'\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("cannot open alert sink {}", path.display()))?;
    file.write_all(&bytes)
        .with_context(|| format!("cannot append to {}", path.display()))?;
    file.sync_data()
        .with_context(|| format!("cannot sync {}", path.display()))?;
    Ok(())
}

/// 3 attempts, 500 ms backoff, then drop (operator-visible). Runs only in the
/// off-path task.
fn deliver_webhook(webhook: &dyn WebhookSender, url: &str, alert: &Alert) {
    let body = match serde_json::to_vec(alert) {
        Ok(body) => body,
        Err(err) => {
            eprintln!("alert webhook serialization failed: {err:#}");
            return;
        }
    };
    for attempt in 1..=WEBHOOK_ATTEMPTS {
        match webhook.send(url, &body) {
            Ok(()) => return,
            Err(err) => {
                eprintln!(
                    "alert webhook attempt {attempt}/{WEBHOOK_ATTEMPTS} to {url} failed: {err:#}"
                );
                if attempt < WEBHOOK_ATTEMPTS {
                    std::thread::sleep(WEBHOOK_BACKOFF);
                }
            }
        }
    }
    eprintln!(
        "alert webhook to {url} dropped after {WEBHOOK_ATTEMPTS} attempts \
         (ledger ordinal {} remains the durable record)",
        alert.ledger_ordinal
    );
}
