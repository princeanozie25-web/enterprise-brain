//! AUTH-4 (threat-model D1): a minimal fixed-window rate limiter for the auth
//! endpoints. Cheap-reject login floods before any session-minting work.
//!
//! Fixed window (not a sliding/token bucket) on purpose: it is the simplest
//! thing that is correct, has no background timer, and needs no clock beyond
//! the same `now_unix` the session store already uses. For the single synthetic
//! org on loopback this is sufficient; it is the named D1 completion, not a
//! production WAF.

use std::sync::Mutex;

use crate::session::now_unix;

/// Default cap: 600 login attempts per 60s window, per process. Generous enough
/// that no legitimate flow (or test matrix) trips it, low enough that a flood
/// is cheap-rejected. Tests dial it down via `AppState::with_login_rate`.
pub const LOGIN_RATE_MAX_DEFAULT: u32 = 600;
pub const LOGIN_RATE_WINDOW_SECS_DEFAULT: u64 = 60;

struct Window {
    start: u64,
    count: u32,
}

/// A fixed-window counter. `check()` records one hit and reports whether it was
/// within the cap.
pub struct RateLimiter {
    max: u32,
    window_secs: u64,
    window: Mutex<Window>,
}

impl RateLimiter {
    pub fn new(max: u32, window_secs: u64) -> RateLimiter {
        RateLimiter {
            max,
            window_secs,
            window: Mutex::new(Window { start: 0, count: 0 }),
        }
    }

    /// The default login limiter (`LOGIN_RATE_MAX_DEFAULT` per
    /// `LOGIN_RATE_WINDOW_SECS_DEFAULT`).
    pub fn default_login() -> RateLimiter {
        RateLimiter::new(LOGIN_RATE_MAX_DEFAULT, LOGIN_RATE_WINDOW_SECS_DEFAULT)
    }

    /// Count one attempt. Returns `true` if it is within the window cap (allow),
    /// `false` if the cap is already reached (cheap-reject -> 429). The window
    /// rolls over once `window_secs` have elapsed since it opened.
    pub fn check(&self) -> bool {
        let now = now_unix();
        let mut w = self.window.lock().expect("rate limiter mutex");
        if now.saturating_sub(w.start) >= self.window_secs {
            w.start = now;
            w.count = 0;
        }
        if w.count >= self.max {
            return false;
        }
        w.count += 1;
        true
    }
}

/// SHOWCASE-III: default per-principal proposal-generation cap — 3 per 60s.
/// Generation is expensive; a 4th in the window is cheap-rejected (429).
pub const PROPOSAL_GEN_RATE_MAX_DEFAULT: u32 = 3;
pub const PROPOSAL_GEN_WINDOW_SECS_DEFAULT: u64 = 60;

/// A fixed-window counter keyed PER PRINCIPAL. Separate from the global login
/// limiter so proposal generation is throttled per identity without touching
/// the auth path. Never consulted by `classify()` or the oracle → no oracle
/// impact by construction.
pub struct PrincipalRateLimiter {
    max: u32,
    window_secs: u64,
    windows: Mutex<std::collections::HashMap<String, Window>>,
}

impl PrincipalRateLimiter {
    pub fn new(max: u32, window_secs: u64) -> PrincipalRateLimiter {
        PrincipalRateLimiter {
            max,
            window_secs,
            windows: Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn default_proposals() -> PrincipalRateLimiter {
        PrincipalRateLimiter::new(
            PROPOSAL_GEN_RATE_MAX_DEFAULT,
            PROPOSAL_GEN_WINDOW_SECS_DEFAULT,
        )
    }

    /// Count one attempt for `principal`. `true` = within cap (allow); `false` =
    /// cap reached (429). Each principal's window rolls independently.
    pub fn check(&self, principal: &str) -> bool {
        let now = now_unix();
        let mut map = self.windows.lock().expect("principal rate limiter mutex");
        let w = map.entry(principal.to_string()).or_insert(Window {
            start: now,
            count: 0,
        });
        if now.saturating_sub(w.start) >= self.window_secs {
            w.start = now;
            w.count = 0;
        }
        if w.count >= self.max {
            return false;
        }
        w.count += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_cap_allows_then_rejects() {
        let rl = RateLimiter::new(3, 60);
        assert!(rl.check());
        assert!(rl.check());
        assert!(rl.check());
        assert!(!rl.check(), "the 4th attempt in the window is rejected");
        assert!(!rl.check(), "and stays rejected");
    }

    #[test]
    fn a_zero_window_rolls_over_every_call() {
        // window_secs == 0 => every call opens a fresh window => never limited.
        let rl = RateLimiter::new(1, 0);
        assert!(rl.check());
        assert!(rl.check());
    }
}
