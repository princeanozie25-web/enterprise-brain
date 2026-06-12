//! THE NETWORK CARVE-OUT. This module owns ALL network use in Enterprise
//! Brain. Exactly two operations ride through it: embedding texts and judge
//! chat completion — both against a local model server (default Ollama).
//!
//! The client REFUSES, at construction, any endpoint whose host does not
//! resolve to a loopback address (127.0.0.0/8 or ::1). Hostnames are
//! resolved once at construction and the connection only ever goes to the
//! verified loopback addresses — a host that later re-resolves elsewhere
//! cannot redirect traffic. `https` is refused (no TLS stack, no cloud
//! path). Zero retries; every call carries an explicit deadline.
//!
//! The HTTP client is ~100 lines of std-only HTTP/1.0 over `TcpStream`: no
//! HTTP dependency means the entire network surface of the workspace is
//! auditable on one page.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// Default local endpoint (Ollama).
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11434";

/// Exact token counts reported by the local API, when it reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenCounts {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// One metering row for the Bursar sidecar. Never carries content — model id
/// and numbers only. `cost_usd` is always null for local models; the spend
/// ledger's own fail-closed pricing decides what to do with that.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UsageEvent {
    pub cost_usd: Option<f64>,
    /// True when token counts are byte-length estimates (bytes/4), not API
    /// numbers.
    pub estimated: bool,
    pub input_tokens: u64,
    pub model: String,
    pub output_tokens: u64,
    /// FIXED_EPOCH-relative milliseconds. The workspace allows no wall clock,
    /// so this is a deterministic monotone ordinal assigned at write time.
    pub ts: u64,
}

// ---------------------------------------------------------------------------
// Runtime configuration
// ---------------------------------------------------------------------------

/// Loaded from `--config <path>`. Endpoint and the NUMBERS-mandated knobs
/// carry spec defaults; MODEL IDS NEVER DEFAULT — a missing model id refuses
/// at the call site rather than silently picking one.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    #[serde(default)]
    pub embed_model: Option<String>,
    #[serde(default)]
    pub embed_dim: Option<u32>,
    #[serde(default)]
    pub judge_model: Option<String>,
    #[serde(default)]
    pub timeouts_ms: TimeoutsMs,
    #[serde(default)]
    pub judge_elision: ElisionConfig,
}

fn default_endpoint() -> String {
    DEFAULT_ENDPOINT.to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeoutsMs {
    #[serde(default = "default_index_embed_ms")]
    pub index_embed_per_batch: u64,
    #[serde(default = "default_query_embed_ms")]
    pub query_embed: u64,
    #[serde(default = "default_judge_ms")]
    pub judge: u64,
}

fn default_index_embed_ms() -> u64 {
    5000
}
fn default_query_embed_ms() -> u64 {
    1500
}
fn default_judge_ms() -> u64 {
    2000
}

impl Default for TimeoutsMs {
    fn default() -> TimeoutsMs {
        TimeoutsMs {
            index_embed_per_batch: default_index_embed_ms(),
            query_embed: default_query_embed_ms(),
            judge: default_judge_ms(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElisionConfig {
    #[serde(default = "default_min_candidates")]
    pub min_candidates: usize,
    #[serde(default = "default_max_ratio")]
    pub max_top1_top2_ratio: f64,
    #[serde(default = "default_judge_top_k")]
    pub top_k: usize,
    #[serde(default = "default_snippet_chars")]
    pub snippet_chars: usize,
}

fn default_min_candidates() -> usize {
    4
}
fn default_max_ratio() -> f64 {
    1.3
}
fn default_judge_top_k() -> usize {
    12
}
fn default_snippet_chars() -> usize {
    240
}

impl Default for ElisionConfig {
    fn default() -> ElisionConfig {
        ElisionConfig {
            min_candidates: default_min_candidates(),
            max_top1_top2_ratio: default_max_ratio(),
            top_k: default_judge_top_k(),
            snippet_chars: default_snippet_chars(),
        }
    }
}

impl RuntimeConfig {
    pub fn load(path: &Path) -> Result<RuntimeConfig> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("cannot read config {}", path.display()))?;
        serde_json::from_slice(&bytes)
            .with_context(|| format!("config {} fails schema/parse", path.display()))
    }

    /// The embed model id, refused when missing (never defaulted).
    pub fn require_embed_model(&self) -> Result<(&str, u32)> {
        let model = self
            .embed_model
            .as_deref()
            .context("config has no embed_model; refusing to default a model id silently")?;
        let dim = self
            .embed_dim
            .context("config has no embed_dim; refusing to guess vector dimensions")?;
        Ok((model, dim))
    }

    /// The judge model id, refused when missing (never defaulted).
    pub fn require_judge_model(&self) -> Result<&str> {
        self.judge_model
            .as_deref()
            .context("config has no judge_model; refusing to default a model id silently")
    }
}

// ---------------------------------------------------------------------------
// Loopback-only HTTP client
// ---------------------------------------------------------------------------

/// A verified-loopback endpoint: scheme http, host resolved at construction,
/// every resolved address loopback.
#[derive(Debug)]
pub struct LocalLlmClient {
    host_header: String,
    addrs: Vec<SocketAddr>,
}

fn is_loopback(ip: &IpAddr) -> bool {
    match ip {
        // 127.0.0.0/8
        IpAddr::V4(v4) => v4.octets()[0] == 127,
        // ::1 only
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

/// Splits `http://host[:port]` (path must be absent or `/`). Refuses any
/// other scheme — there is no TLS stack here on purpose.
fn parse_endpoint(endpoint: &str) -> Result<(String, u16)> {
    let rest = endpoint
        .strip_prefix("http://")
        .with_context(|| format!("endpoint {endpoint:?} must use plain http:// to loopback"))?;
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    if rest.is_empty() || rest.contains('/') || rest.contains('?') || rest.contains('#') {
        bail!("endpoint {endpoint:?} must be http://host[:port] with no path");
    }
    // [v6]:port | [v6] | host:port | host
    let (host, port) = if let Some(after) = rest.strip_prefix('[') {
        let (v6, tail) = after
            .split_once(']')
            .with_context(|| format!("endpoint {endpoint:?} has an unterminated IPv6 literal"))?;
        let port = match tail.strip_prefix(':') {
            Some(p) => p.parse::<u16>().context("invalid endpoint port")?,
            None if tail.is_empty() => 80,
            _ => bail!("endpoint {endpoint:?} is malformed after the IPv6 literal"),
        };
        (v6.to_string(), port)
    } else if let Some((host, p)) = rest.rsplit_once(':') {
        (
            host.to_string(),
            p.parse::<u16>().context("invalid endpoint port")?,
        )
    } else {
        (rest.to_string(), 80)
    };
    if host.is_empty() {
        bail!("endpoint {endpoint:?} has an empty host");
    }
    Ok((host, port))
}

impl LocalLlmClient {
    /// Constructs the client, refusing any endpoint whose host does not
    /// resolve to loopback. Literal IPs are checked without any resolution;
    /// hostnames are resolved ONCE and every resolved address must be
    /// loopback (fail closed). Construction opens no connection.
    pub fn new(endpoint: &str) -> Result<LocalLlmClient> {
        let (host, port) = parse_endpoint(endpoint)?;
        let addrs: Vec<SocketAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
            if !is_loopback(&ip) {
                bail!(
                    "endpoint host {host} is not a loopback address; \
                     the local-LLM carve-out refuses non-loopback endpoints"
                );
            }
            vec![SocketAddr::new(ip, port)]
        } else {
            let resolved: Vec<SocketAddr> = (host.as_str(), port)
                .to_socket_addrs()
                .with_context(|| format!("cannot resolve endpoint host {host}"))?
                .collect();
            if resolved.is_empty() {
                bail!("endpoint host {host} resolved to no addresses; refusing");
            }
            if let Some(bad) = resolved.iter().find(|a| !is_loopback(&a.ip())) {
                bail!(
                    "endpoint host {host} resolves to non-loopback {}; \
                     the local-LLM carve-out refuses non-loopback endpoints",
                    bad.ip()
                );
            }
            resolved
        };
        let host_header = if host.contains(':') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        Ok(LocalLlmClient { host_header, addrs })
    }

    /// POSTs a JSON body and returns the parsed JSON response. One attempt,
    /// no retries; the deadline covers connect + write + read.
    pub fn post_json(
        &self,
        path: &str,
        body: &serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value> {
        let deadline = Instant::now() + timeout;
        let remaining = |what: &str| -> Result<Duration> {
            let now = Instant::now();
            if now >= deadline {
                bail!("local LLM call timed out before {what}");
            }
            Ok(deadline - now)
        };

        let mut stream = TcpStream::connect_timeout(&self.addrs[0], remaining("connect")?)
            .with_context(|| format!("cannot connect to local LLM at {}", self.addrs[0]))?;
        stream.set_write_timeout(Some(remaining("write")?))?;

        let payload = serde_json::to_vec(body).context("encoding request body")?;
        // HTTP/1.0 + Connection: close keeps the response framing trivial:
        // the body is everything until EOF, never chunked.
        let request = format!(
            "POST {path} HTTP/1.0\r\nHost: {}\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n",
            self.host_header,
            payload.len()
        );
        stream
            .write_all(request.as_bytes())
            .and_then(|()| stream.write_all(&payload))
            .context("writing request to local LLM")?;

        let mut response = Vec::new();
        let mut buf = [0u8; 16 * 1024];
        loop {
            stream.set_read_timeout(Some(remaining("read")?))?;
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => response.extend_from_slice(&buf[..n]),
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    bail!("local LLM call timed out while reading the response");
                }
                Err(e) => return Err(e).context("reading response from local LLM"),
            }
        }

        let header_end = response
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .context("local LLM response has no header terminator")?;
        let head = std::str::from_utf8(&response[..header_end])
            .context("local LLM response headers are not UTF-8")?;
        let status_line = head.lines().next().unwrap_or("");
        let status = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok())
            .context("local LLM response has no status code")?;
        if head
            .lines()
            .any(|l| l.to_ascii_lowercase().starts_with("transfer-encoding:"))
        {
            bail!("local LLM responded with a transfer encoding this client does not speak");
        }
        let body_bytes = &response[header_end + 4..];
        if status != 200 {
            let preview: String = String::from_utf8_lossy(body_bytes)
                .chars()
                .take(200)
                .collect();
            bail!("local LLM returned HTTP {status}: {preview}");
        }
        serde_json::from_slice(body_bytes).context("local LLM response is not valid JSON")
    }
}

/// Appends usage events to a JSONL sidecar, assigning deterministic monotone
/// `ts` ordinals that continue from the rows already present in the file.
/// Rows carry model + numbers only — no content, ever.
pub fn append_usage_sidecar(path: &Path, events: &[UsageEvent]) -> Result<()> {
    if events.is_empty() {
        return Ok(());
    }
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => text.lines().filter(|l| !l.trim().is_empty()).count() as u64,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
    };
    let mut out = String::new();
    for (i, event) in events.iter().enumerate() {
        let mut row = event.clone();
        row.ts = existing + i as u64;
        // Canonical row: sorted keys via the Value round-trip.
        let value = serde_json::to_value(&row).context("encoding usage row")?;
        out.push_str(&serde_json::to_string(&value).context("encoding usage row")?);
        out.push('\n');
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("cannot open usage sidecar {}", path.display()))?;
    file.write_all(out.as_bytes())
        .with_context(|| format!("cannot append to usage sidecar {}", path.display()))?;
    Ok(())
}

/// bytes/4 token estimate for APIs that do not report counts.
pub fn estimate_tokens(byte_len: usize) -> u64 {
    (byte_len as u64).div_ceil(4)
}
