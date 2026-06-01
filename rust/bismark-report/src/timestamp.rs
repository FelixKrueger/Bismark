//! `{{date}}` / `{{time}}` generation — the report's only non-determinism.
//!
//! Two paths (PLAN A6, zero new dependency trees):
//! * **deterministic** (`--__test_timestamp <epoch>`): format the epoch in
//!   **UTC** with pure-std integer civil-time math → byte-stable golden HTML.
//! * **default** (`None`): local time via the already-locked `libc::localtime_r`
//!   (Perl `localtime`). Not byte-gated — the gate normalizes this line.
//!
//! Both format exactly like Perl: `%04d-%02d-%02d` (date) / `%02d:%02d:%02d`
//! (time) — see `bismark2report:170-171`.

/// Returns `(date, time)` as `("YYYY-MM-DD", "HH:MM:SS")`.
pub fn timestamp(test_epoch: Option<i64>) -> (String, String) {
    match test_epoch {
        Some(epoch) => civil_from_epoch_utc(epoch),
        None => local_now(),
    }
}

/// UTC civil time from a UNIX epoch, pure-std (no leap seconds for Unix time).
fn civil_from_epoch_utc(epoch: i64) -> (String, String) {
    let days = epoch.div_euclid(86_400);
    let secs = epoch.rem_euclid(86_400);
    let (y, mo, d) = civil_from_days(days);
    let (h, mi, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    (
        format!("{y:04}-{mo:02}-{d:02}"),
        format!("{h:02}:{mi:02}:{s:02}"),
    )
}

/// Howard Hinnant's `civil_from_days`: days-since-1970-01-01 → (year, month, day).
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

/// Local time via `libc::localtime_r` (matches Perl `localtime(time)`).
fn local_now() -> (String, String) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // SAFETY: `localtime_r` writes into a zeroed `tm` we own; `t` outlives the
    // call. Contained here per PLAN §11 (this path is not byte-gated).
    unsafe {
        let t = secs as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        (
            format!(
                "{:04}-{:02}-{:02}",
                tm.tm_year + 1900,
                tm.tm_mon + 1,
                tm.tm_mday
            ),
            format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_zero_is_unix_epoch_utc() {
        assert_eq!(timestamp(Some(0)), ("1970-01-01".into(), "00:00:00".into()));
    }

    #[test]
    fn known_epoch_utc() {
        // 2018-08-16 11:25:43 UTC = 1534418743
        assert_eq!(
            timestamp(Some(1_534_418_743)),
            ("2018-08-16".into(), "11:25:43".into())
        );
    }

    #[test]
    fn leap_day_utc() {
        // 2020-02-29 00:00:00 UTC = 1582934400
        assert_eq!(
            civil_from_epoch_utc(1_582_934_400).0,
            "2020-02-29".to_string()
        );
    }
}
