//! S4: the clock seam. Ledger rows gain timestamps, but the ledger's
//! byte-determinism discipline (exact-JSON pins) survives via injection:
//! production reads the wall clock; tests inject a deterministic clock and
//! never read the real one.
//!
//! The seam is deliberately narrow — one method, `now_rfc3339_ms()` — so a
//! test clock is a two-line struct and there is nowhere for a stray
//! `SystemTime::now()` to hide.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A source of timestamps. `now_rfc3339_ms()` returns an RFC3339 UTC string
/// with millisecond precision (e.g. `2026-07-11T09:42:03.500Z`).
pub trait Clock: Send + Sync {
    fn now_rfc3339_ms(&self) -> String;
}

/// Production clock: the real wall clock, UTC, millisecond precision.
pub struct WallClock;

impl Clock for WallClock {
    fn now_rfc3339_ms(&self) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        format_rfc3339_ms(now.as_millis() as u64)
    }
}

/// Test clock: a deterministic clock that starts at a fixed instant and
/// advances by a fixed step on each read. No test may read the real clock —
/// this is what they inject instead.
pub struct FixedClock {
    millis: AtomicU64,
    step_ms: u64,
}

impl FixedClock {
    /// Starts at `start_ms` (Unix ms) and advances `step_ms` per read, so a
    /// sequence of rows gets strictly increasing, reproducible timestamps.
    pub fn new(start_ms: u64, step_ms: u64) -> FixedClock {
        FixedClock {
            millis: AtomicU64::new(start_ms),
            step_ms,
        }
    }

    /// A clock frozen at one instant (step 0) — every read returns the same
    /// timestamp.
    pub fn frozen(at_ms: u64) -> FixedClock {
        FixedClock::new(at_ms, 0)
    }
}

impl Clock for FixedClock {
    fn now_rfc3339_ms(&self) -> String {
        let ms = self.millis.fetch_add(self.step_ms, Ordering::SeqCst);
        format_rfc3339_ms(ms)
    }
}

/// Format Unix-epoch milliseconds as an RFC3339 UTC string with ms
/// precision. Hand-rolled (civil-time from the epoch) so the service takes
/// no new time-formatting dependency, and every timestamp is stable
/// regardless of locale or platform.
pub fn format_rfc3339_ms(unix_ms: u64) -> String {
    let secs = unix_ms / 1000;
    let millis = unix_ms % 1000;
    let (year, month, day, hour, min, sec) = civil_from_unix(secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}.{millis:03}Z")
}

/// Convert Unix seconds (UTC) to civil (Y, M, D, h, m, s). Proleptic
/// Gregorian, days-from-epoch algorithm (Howard Hinnant's `civil_from_days`).
fn civil_from_unix(unix_secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (unix_secs / 86_400) as i64;
    let rem = unix_secs % 86_400;
    let hour = (rem / 3_600) as u32;
    let min = ((rem % 3_600) / 60) as u32;
    let sec = (rem % 60) as u32;

    // days since 1970-01-01 -> civil date.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d, hour, min, sec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_epochs_format_correctly() {
        // 2026-07-11T09:42:03.500Z — a known instant used by the S4 tests.
        // 2026-07-11 is day 20645 from the epoch.
        let ms = 1_783_762_923_500;
        assert_eq!(format_rfc3339_ms(ms), "2026-07-11T09:42:03.500Z");
        // The epoch itself.
        assert_eq!(format_rfc3339_ms(0), "1970-01-01T00:00:00.000Z");
        // A leap-year boundary: 2024-02-29T12:00:00.000Z = 1709208000000.
        assert_eq!(
            format_rfc3339_ms(1_709_208_000_000),
            "2024-02-29T12:00:00.000Z"
        );
    }

    #[test]
    fn fixed_clock_advances_by_its_step() {
        let clock = FixedClock::new(1_783_762_923_500, 1000);
        assert_eq!(clock.now_rfc3339_ms(), "2026-07-11T09:42:03.500Z");
        assert_eq!(clock.now_rfc3339_ms(), "2026-07-11T09:42:04.500Z");
        // A frozen clock never moves.
        let frozen = FixedClock::frozen(0);
        assert_eq!(frozen.now_rfc3339_ms(), frozen.now_rfc3339_ms());
    }
}
