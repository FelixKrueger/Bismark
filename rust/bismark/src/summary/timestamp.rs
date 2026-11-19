//! HTML `{{report_timestamp}}` formatting.
//!
//! Perl uses scalar `localtime` (`bismark2summary:488`), a C `ctime` string:
//! `"Www Mmm <space-padded D> HH:MM:SS YYYY"` (e.g. `"Mon Jun  1 09:07:00
//! 2026"` — note the two spaces before a single-digit day-of-month).
//!
//! The live default formats local time (like Perl and `bismark2report`); the
//! hidden `--__test_timestamp` epoch is formatted in UTC so the committed HTML
//! goldens are deterministic regardless of the runner's timezone. The
//! acceptance gate normalizes this single line before comparing.

use crate::report::timestamp::{civil_from_days, local_utc_offset, now_epoch};

const WDAY: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MON: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The current local time as a C `ctime` string (Perl scalar `localtime`,
/// `bismark2summary:488`). Used for the live (non-test) HTML timestamp.
#[must_use]
pub fn now_ctime_local() -> String {
    let epoch = now_epoch();
    format_ctime_utc(epoch + local_utc_offset(epoch))
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
