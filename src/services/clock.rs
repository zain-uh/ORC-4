//! Wall-clock time for the status bar.
//!
//! The Pi has no real-time clock, so until NTP or the phone sets the time the
//! system clock reads 1970. Any time before 2025 is treated as "unknown" and
//! the clock is hidden rather than showing a wrong time.
//!
//! The phone sends its time and UTC offset when it connects. The time is kept
//! as a correction to the system clock (setting the system clock needs root;
//! that comes with the device image), so it survives until the next restart.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 2025-01-01T00:00:00Z.
const EARLIEST_VALID_UNIX: u64 = 1_735_689_600;

/// Seconds to add to the system clock (set from the phone).
static CORRECTION_SECS: AtomicI64 = AtomicI64::new(0);

/// Trust `unix` (from the phone) as the current time.
pub fn set_time(unix: i64) {
    let system = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let correction = unix - system;
    if correction.abs() > 2 {
        log::info!("clock corrected by {correction} s from the phone");
    }
    CORRECTION_SECS.store(correction, Ordering::Relaxed);
}

fn now() -> SystemTime {
    let c = CORRECTION_SECS.load(Ordering::Relaxed);
    let d = Duration::from_secs(c.unsigned_abs());
    if c >= 0 {
        SystemTime::now() + d
    } else {
        SystemTime::now() - d
    }
}

/// Local hours and minutes, or `None` if the clock has not been set.
pub fn local_hhmm(utc_offset_minutes: i32) -> Option<(u8, u8)> {
    hhmm_at(now(), utc_offset_minutes)
}

/// Time until the next minute boundary, so an idle UI wakes exactly when the
/// clock needs redrawing.
pub fn until_next_minute() -> Duration {
    let since_epoch = now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let into_minute = Duration::from_nanos((since_epoch.as_nanos() % 60_000_000_000) as u64);
    Duration::from_secs(60) - into_minute
}

fn hhmm_at(t: SystemTime, utc_offset_minutes: i32) -> Option<(u8, u8)> {
    let secs = t.duration_since(UNIX_EPOCH).ok()?.as_secs();
    if secs < EARLIEST_VALID_UNIX {
        return None;
    }
    let local_minutes = (secs as i64 / 60 + utc_offset_minutes as i64).rem_euclid(24 * 60);
    Some(((local_minutes / 60) as u8, (local_minutes % 60) as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(unix: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(unix)
    }

    #[test]
    fn unset_clock_is_hidden() {
        assert_eq!(hhmm_at(at(60), 0), None, "1970 = never set");
    }

    #[test]
    fn formats_utc_and_applies_offset() {
        // 2026-10-07T13:45:30Z
        let t = at(1_791_380_730);
        assert_eq!(hhmm_at(t, 0), Some((13, 45)));
        assert_eq!(hhmm_at(t, 60), Some((14, 45)), "UTC+1");
        assert_eq!(
            hhmm_at(t, -14 * 60),
            Some((23, 45)),
            "wraps to previous day"
        );
    }

    #[test]
    fn next_minute_is_within_a_minute() {
        let d = until_next_minute();
        assert!(d > Duration::ZERO && d <= Duration::from_secs(60));
    }
}
