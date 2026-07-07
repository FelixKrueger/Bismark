//! HTML `{{report_timestamp}}` formatting.
//!
//! Perl uses scalar `localtime` (`bismark2summary:488`), a C `ctime` string:
//! `"Www Mmm <space-padded D> HH:MM:SS YYYY"` (e.g. `"Mon Jun  1 09:07:00
//! 2026"` — note the two spaces before a single-digit day-of-month).
//!
//! **Deliberate deviation (documented):** this port formats the timestamp in
//! **UTC** (pure `std`, no `unsafe`, no new dependency), for both the live
//! default (`SystemTime::now`) and the hidden `--__test_timestamp` epoch.
//! Perl's default is *local* time, but (a) the acceptance gate **normalizes
//! this single line** before comparing (Perl `localtime` cannot be pinned), so
//! byte-identity is unaffected; and (b) the committed HTML goldens use
//! `--__test_timestamp` (a fixed epoch), so they are fully deterministic
//! regardless of the runner's timezone.
//!
//! Adopting local time would require an `unsafe` `libc::localtime_r` call
//! (breaking the crate's `#![forbid(unsafe_code)]`) or a heavier dependency.
//! The only user-visible effect is the live timestamp line reading UTC.

const WDAY: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MON: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The current UTC time as a C `ctime` string. Used for the live (non-test)
/// HTML timestamp.
#[must_use]
pub fn now_ctime_utc() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_ctime_utc(epoch)
}

/// Format a UNIX `epoch` (seconds) as a C `ctime`/`asctime` string in UTC:
/// `"Www Mmm <space-padded D> HH:MM:SS YYYY"`. Matches Perl `scalar
/// gmtime(epoch)`.
#[must_use]
pub fn format_ctime_utc(epoch: i64) -> String {
    let days = epoch.div_euclid(86_400);
    let secs = epoch.rem_euclid(86_400);
    let hh = secs / 3600;
    let mm = (secs % 3600) / 60;
    let ss = secs % 60;
    // 1970-01-01 was a Thursday (index 4 with 0=Sun).
    let wday = (days.rem_euclid(7) + 4).rem_euclid(7) as usize;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{} {} {:2} {:02}:{:02}:{:02} {}",
        WDAY[wday],
        MON[(month - 1) as usize],
        day,
        hh,
        mm,
        ss,
        year
    )
}

/// Howard Hinnant's `civil_from_days`: convert a day count since
/// 1970-01-01 into `(year, month [1-12], day [1-31])`.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_epochs_match_c_ctime() {
        // Verified against `perl -e 'print scalar gmtime(E)'`.
        // 0            → Thu Jan  1 00:00:00 1970
        assert_eq!(format_ctime_utc(0), "Thu Jan  1 00:00:00 1970");
        // 1_700_000_000 → Tue Nov 14 22:13:20 2023
        assert_eq!(format_ctime_utc(1_700_000_000), "Tue Nov 14 22:13:20 2023");
        // 1_000_000_000 → Sun Sep  9 01:46:40 2001 (single-digit day → two spaces)
        assert_eq!(format_ctime_utc(1_000_000_000), "Sun Sep  9 01:46:40 2001");
    }

    #[test]
    fn single_digit_day_is_space_padded() {
        // 2026-06-01 00:00:00 UTC = 1780272000
        assert_eq!(format_ctime_utc(1_780_272_000), "Mon Jun  1 00:00:00 2026");
    }
}
