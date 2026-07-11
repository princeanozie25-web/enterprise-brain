//! S0: the JWKS key source behind ONE trait, so validation code is identical
//! whether keys come from a local file (tests, offline) or the tenant's
//! discovery endpoint (live).
//!
//! Fail-closed by construction: any failure to produce a key — missing file,
//! unparseable JWKS, unknown `kid`, unreachable endpoint — yields `None`,
//! which the ladder turns into a DENY (EB-4). An unreachable key source is
//! a deny, never a bypass.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use jsonwebtoken::DecodingKey;
use serde::Deserialize;

/// JWKS cache lifetime for the HTTP provider: 24 hours.
pub const JWKS_CACHE_TTL_SECS: u64 = 86_400;

/// The one seam the validator sees. `kid` is the token header's key id;
/// `None` when the header carries none (resolvable only if the set holds
/// exactly one key — anything ambiguous is a miss).
pub trait JwksProvider: Send + Sync {
    fn key_for(&self, kid: Option<&str>) -> Option<DecodingKey>;
}

/// One RSA entry of a JWKS document. Non-RSA entries are skipped at parse
/// (defensive: their presence must not fail the set).
#[derive(Debug, Clone, Deserialize)]
struct Jwk {
    #[serde(default)]
    kty: String,
    #[serde(default)]
    kid: Option<String>,
    #[serde(default)]
    n: Option<String>,
    #[serde(default)]
    e: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JwksDocument {
    keys: Vec<Jwk>,
}

fn parse_jwks(text: &str) -> Result<Vec<Jwk>> {
    let document: JwksDocument = serde_json::from_str(text).context("JWKS fails parse")?;
    Ok(document
        .keys
        .into_iter()
        .filter(|k| k.kty == "RSA" && k.n.is_some() && k.e.is_some())
        .collect())
}

/// Select the entry for `kid` and build its decoding key. A `kid`-less
/// header resolves only against a single-key set; a `kid` that matches
/// nothing is a miss. Component decode failure is a miss too (fail closed).
fn key_from(keys: &[Jwk], kid: Option<&str>) -> Option<DecodingKey> {
    let entry = match kid {
        Some(kid) => keys.iter().find(|k| k.kid.as_deref() == Some(kid))?,
        None => {
            if keys.len() == 1 {
                &keys[0]
            } else {
                return None;
            }
        }
    };
    DecodingKey::from_rsa_components(entry.n.as_deref()?, entry.e.as_deref()?).ok()
}

/// Offline provider: a JWKS JSON loaded once from disk. The test fixture
/// path — and byte-for-byte the SAME lookup/build code the live path uses.
pub struct FileJwks {
    keys: Vec<Jwk>,
}

impl FileJwks {
    pub fn load(path: &std::path::Path) -> Result<FileJwks> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read JWKS file {}", path.display()))?;
        Ok(FileJwks {
            keys: parse_jwks(&text)?,
        })
    }
}

impl JwksProvider for FileJwks {
    fn key_for(&self, kid: Option<&str>) -> Option<DecodingKey> {
        key_from(&self.keys, kid)
    }
}

/// The fetch seam: URL in, JWKS body out. Production wiring uses [`ureq`];
/// tests inject counting / failing closures. The signature is sync — the
/// bridge runs inside `spawn_blocking`, matching the crate's discipline
/// that only the HTTP edge is async.
pub type Fetcher = Box<dyn Fn(&str) -> Result<String> + Send + Sync>;

/// Live provider: the tenant JWKS endpoint, cached for
/// [`JWKS_CACHE_TTL_SECS`]. Refresh policy: at most ONE fetch per request —
/// on an empty/expired cache, or on a `kid` the fresh cache does not hold
/// (key rollover). A failed fetch empties nothing and produces no key: the
/// request denies rather than trusting anything stale past its TTL.
pub struct HttpJwks {
    url: String,
    ttl: Duration,
    fetch: Fetcher,
    cache: Mutex<Option<(Instant, Vec<Jwk>)>>,
}

impl HttpJwks {
    pub fn new(url: &str) -> HttpJwks {
        HttpJwks::with_fetcher(
            url,
            Duration::from_secs(JWKS_CACHE_TTL_SECS),
            Box::new(|url: &str| {
                let mut response = ureq::get(url)
                    .call()
                    .with_context(|| format!("JWKS fetch {url} failed"))?;
                response
                    .body_mut()
                    .read_to_string()
                    .with_context(|| format!("JWKS body from {url} unreadable"))
            }),
        )
    }

    /// Injectable constructor (tests: counting fetchers, failure paths,
    /// short TTLs). Production uses [`HttpJwks::new`].
    pub fn with_fetcher(url: &str, ttl: Duration, fetch: Fetcher) -> HttpJwks {
        HttpJwks {
            url: url.to_string(),
            ttl,
            fetch,
            cache: Mutex::new(None),
        }
    }

    fn refreshed_keys(&self) -> Option<Vec<Jwk>> {
        let text = (self.fetch)(&self.url).ok()?;
        parse_jwks(&text).ok()
    }
}

impl JwksProvider for HttpJwks {
    fn key_for(&self, kid: Option<&str>) -> Option<DecodingKey> {
        let mut cache = self.cache.lock().expect("jwks cache mutex");
        // Serve from a fresh cache when it can answer.
        if let Some((fetched_at, keys)) = cache.as_ref() {
            if fetched_at.elapsed() < self.ttl {
                if let Some(key) = key_from(keys, kid) {
                    return Some(key);
                }
                // Fresh cache, unknown kid: fall through to ONE refresh
                // (key rollover), below.
            }
        }
        // The one permitted fetch for this request. Failure -> no key ->
        // the ladder denies (never a bypass, never stale-beyond-TTL trust).
        let keys = self.refreshed_keys()?;
        let key = key_from(&keys, kid);
        *cache = Some((Instant::now(), keys));
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // A minimal syntactically-valid RSA JWKS (values are unpadded base64url;
    // decodability of n/e is all key_from needs to construct a DecodingKey).
    fn jwks_json(kid: &str) -> String {
        format!(
            "{{\"keys\":[{{\"kty\":\"RSA\",\"kid\":\"{kid}\",\"n\":\"u1SU1LfVLPHCozMxH2Mo4lgOEePzNm0tRgeLezV6ffAt0gunVTLw7onLRnrq0_IzW7yWR7QkrmBL7jTKEn5u-qKhbwKfBstIs-bMY2Zkp18gnTxKLxoS2tFczGkPLPgizskuemMghRniWaoLcyehkd3qqGElvW_VDL5AaWTg0nLVkjRo9z-40RQzuVaE8AkAFmxZzow3x-VJYKdjykkJ0iT9wCS0DRTXu269V264Vf_3jvredZiKRkgwlL9xNAwxXFg0x_XFw005UWVRIkdgcKWTjpBP2dPwVZ4WWC-9aGVd-Gyn1o0CLelf4rEjGoXbAAEgAqeGUxrcIlbjXfbcmw\",\"e\":\"AQAB\"}}]}}"
        )
    }

    #[test]
    fn file_jwks_resolves_by_kid_and_misses_unknowns() {
        let dir = std::env::temp_dir().join(format!("eb-jwks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("jwks.json");
        std::fs::write(&path, jwks_json("kid-a")).unwrap();
        let jwks = FileJwks::load(&path).unwrap();
        assert!(jwks.key_for(Some("kid-a")).is_some());
        assert!(jwks.key_for(Some("kid-b")).is_none(), "unknown kid misses");
        // A single-key set answers a kid-less header; nothing else would.
        assert!(jwks.key_for(None).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn http_jwks_fetch_failure_yields_no_key_never_a_bypass() {
        let jwks = HttpJwks::with_fetcher(
            "https://unreachable.invalid/keys",
            Duration::from_secs(60),
            Box::new(|_| anyhow::bail!("simulated outage")),
        );
        assert!(jwks.key_for(Some("kid-a")).is_none());
    }

    #[test]
    fn http_jwks_caches_and_refreshes_at_most_once_per_request() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let jwks = HttpJwks::with_fetcher(
            "https://tenant.example/keys",
            Duration::from_secs(3600),
            Box::new(move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(jwks_json("kid-a"))
            }),
        );
        // First request fetches once; second serves from cache.
        assert!(jwks.key_for(Some("kid-a")).is_some());
        assert!(jwks.key_for(Some("kid-a")).is_some());
        assert_eq!(calls.load(Ordering::SeqCst), 1, "fresh cache serves");
        // Unknown kid on a fresh cache triggers EXACTLY one refresh for
        // that request — and still misses if the rollover has no such key.
        assert!(jwks.key_for(Some("kid-rolled")).is_none());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "one refresh, no retry loop"
        );
    }
}
