//! FC-A1: server-minted sessions. Identity is bound from a session the server
//! mints and validates — NEVER from a caller-asserted header.
//!
//! Design: an OPAQUE, unguessable token (256 bits of OS CSPRNG entropy) backed
//! by a server-side store. The store is authoritative, so the token cannot be
//! forged (only the server mints entries) and a client-supplied id is never
//! honoured (session fixation has no purchase). Sessions expire (8h) and are
//! revocable (logout). Tokens are held only as sha256 HASHES — the raw token
//! exists solely in the client's cookie / bearer header, never at rest here.
//!
//! This is the authentication boundary; the scope-DECISION layer (compiler,
//! oracle, conformance) sits BELOW it and is untouched by this slice.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use retrieval::index::sha256_hex;

/// Session lifetime: 8 hours.
pub const SESSION_TTL_SECS: u64 = 8 * 60 * 60;
/// The session cookie name (httpOnly, SameSite=Lax — set on login).
pub const SESSION_COOKIE: &str = "eb_session";
/// AUTH-4 (D1): the maximum number of concurrent LIVE sessions one principal
/// may hold. Generous (the console reuses one session per identity), but a
/// flood that keeps minting for a single principal is rejected past this.
pub const DEFAULT_SESSION_QUOTA: usize = 64;

#[derive(Clone)]
struct SessionRecord {
    principal_id: String,
    issued_at: u64,
    expires_at: u64,
}

/// A freshly minted session. The raw `token` is handed to the client exactly
/// once (cookie + body); the store keeps only its hash.
pub struct MintedSession {
    pub token: String,
    pub principal_id: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

/// The principal id resolved from a validated session, carried in request
/// extensions by the `require_session` middleware. The `DemoPrincipal`
/// extractor reads ONLY this — never a header.
#[derive(Clone, Debug)]
pub struct SessionPrincipal(pub String);

/// In-memory, per-process session store. Keyed by sha256(token).
pub struct SessionStore {
    by_token_hash: Mutex<HashMap<String, SessionRecord>>,
    /// AUTH-4 (D1): per-principal concurrent-session cap.
    quota: usize,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore {
    pub fn new() -> SessionStore {
        SessionStore::with_quota(DEFAULT_SESSION_QUOTA)
    }

    /// A store with an explicit per-principal session quota (AUTH-4 tests dial
    /// this down to exercise the rejection branch deterministically).
    pub fn with_quota(quota: usize) -> SessionStore {
        SessionStore {
            by_token_hash: Mutex::new(HashMap::new()),
            quota,
        }
    }

    /// The per-principal concurrent-session quota in force.
    pub fn quota(&self) -> usize {
        self.quota
    }

    /// Mint a session that expires `SESSION_TTL_SECS` from now.
    pub fn mint(&self, principal_id: &str) -> MintedSession {
        let now = now_unix();
        self.mint_with_expiry(principal_id, now, now + SESSION_TTL_SECS)
    }

    /// AUTH-4 (D1): mint subject to the per-principal concurrent-session quota.
    /// Returns `None` (rejected) if the principal already holds `quota` live
    /// sessions. The count + insert happen under one lock, so concurrent mints
    /// cannot race past the cap.
    pub fn try_mint(&self, principal_id: &str) -> Option<MintedSession> {
        let now = now_unix();
        let token = new_token();
        let record = SessionRecord {
            principal_id: principal_id.to_string(),
            issued_at: now,
            expires_at: now + SESSION_TTL_SECS,
        };
        let mut map = self.by_token_hash.lock().expect("session store mutex");
        let live = map
            .values()
            .filter(|r| r.principal_id == principal_id && r.expires_at > now)
            .count();
        if live >= self.quota {
            return None;
        }
        map.insert(sha256_hex(token.as_bytes()), record.clone());
        Some(MintedSession {
            token,
            principal_id: record.principal_id,
            issued_at: record.issued_at,
            expires_at: record.expires_at,
        })
    }

    /// Count the live (unexpired) sessions a principal currently holds.
    pub fn live_count_for(&self, principal_id: &str) -> usize {
        let now = now_unix();
        self.by_token_hash
            .lock()
            .expect("session store mutex")
            .values()
            .filter(|r| r.principal_id == principal_id && r.expires_at > now)
            .count()
    }

    /// Mint a session with explicit issue/expiry instants. Login uses `mint`;
    /// this exists so tests can mint an already-expired session deterministically
    /// (the expiry path is otherwise unreachable inside an 8h test run).
    pub fn mint_with_expiry(
        &self,
        principal_id: &str,
        issued_at: u64,
        expires_at: u64,
    ) -> MintedSession {
        let token = new_token();
        let record = SessionRecord {
            principal_id: principal_id.to_string(),
            issued_at,
            expires_at,
        };
        self.by_token_hash
            .lock()
            .expect("session store mutex")
            .insert(sha256_hex(token.as_bytes()), record.clone());
        MintedSession {
            token,
            principal_id: record.principal_id,
            issued_at: record.issued_at,
            expires_at: record.expires_at,
        }
    }

    /// Resolve a raw token to its principal IFF the session is live (known,
    /// unexpired, unrevoked). Fail-closed: anything else -> None. An expired
    /// session is evicted on read.
    pub fn resolve(&self, token: &str) -> Option<String> {
        let key = sha256_hex(token.as_bytes());
        let mut map = self.by_token_hash.lock().expect("session store mutex");
        match map.get(&key) {
            Some(record) if record.expires_at > now_unix() => Some(record.principal_id.clone()),
            Some(_) => {
                map.remove(&key);
                None
            }
            None => None,
        }
    }

    /// Revoke (logout). Returns true if a live entry was removed.
    pub fn revoke(&self, token: &str) -> bool {
        let key = sha256_hex(token.as_bytes());
        self.by_token_hash
            .lock()
            .expect("session store mutex")
            .remove(&key)
            .is_some()
    }

    /// Live session count (diagnostics/tests).
    pub fn len(&self) -> usize {
        self.by_token_hash
            .lock()
            .expect("session store mutex")
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A 256-bit opaque token: sha256 over 32 bytes of OS CSPRNG entropy, rendered
/// as 64 hex chars. (Hashing the random bytes avoids a separate hex encoder
/// while preserving full entropy; the token is unguessable either way.)
fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS CSPRNG must be available to mint a session");
    sha256_hex(&bytes)
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_then_resolve_roundtrips_the_principal() {
        let store = SessionStore::new();
        let s = store.mint("p060");
        assert_eq!(store.resolve(&s.token).as_deref(), Some("p060"));
        assert_eq!(s.expires_at - s.issued_at, SESSION_TTL_SECS);
    }

    #[test]
    fn unknown_token_and_fixation_attempt_resolve_to_none() {
        let store = SessionStore::new();
        // A token the server never minted (a client-chosen / forged id).
        assert_eq!(store.resolve("not-a-real-token"), None);
        assert_eq!(store.resolve(&sha256_hex(b"client-picked")), None);
    }

    #[test]
    fn expired_session_fails_closed_and_is_evicted() {
        let store = SessionStore::new();
        let s = store.mint_with_expiry("p060", 0, 1); // expired in 1970
        assert_eq!(store.resolve(&s.token), None);
        assert!(store.is_empty(), "expired session evicted on read");
    }

    #[test]
    fn revoked_session_no_longer_resolves() {
        let store = SessionStore::new();
        let s = store.mint("p088");
        assert!(store.revoke(&s.token));
        assert_eq!(store.resolve(&s.token), None);
        assert!(!store.revoke(&s.token), "double-revoke is a no-op");
    }

    #[test]
    fn two_mints_are_distinct_tokens() {
        let store = SessionStore::new();
        let a = store.mint("p060");
        let b = store.mint("p060");
        assert_ne!(a.token, b.token, "each session is a fresh unguessable token");
    }

    #[test]
    fn try_mint_rejects_past_the_per_principal_quota() {
        let store = SessionStore::with_quota(3);
        // Three live sessions for one principal are fine.
        for _ in 0..3 {
            assert!(store.try_mint("p060").is_some());
        }
        assert_eq!(store.live_count_for("p060"), 3);
        // The 4th is rejected (excess past the quota).
        assert!(
            store.try_mint("p060").is_none(),
            "the (quota+1)th concurrent session for a principal is rejected"
        );
        // A DIFFERENT principal is unaffected — the quota is per-principal.
        assert!(store.try_mint("p088").is_some());
    }

    #[test]
    fn revoking_a_session_frees_a_quota_slot() {
        let store = SessionStore::with_quota(1);
        let s = store.try_mint("p060").expect("first fits");
        assert!(store.try_mint("p060").is_none(), "quota of 1 is full");
        assert!(store.revoke(&s.token));
        assert!(
            store.try_mint("p060").is_some(),
            "a freed slot accepts a new session"
        );
    }
}
