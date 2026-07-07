//! Build-time version/provenance capture for the single-crate Bismark Rust suite.
//!
//! Emits (as `cargo:rustc-env`) the SUITE version + git short-hash + a reproducible
//! build timestamp + a single suite-wide "last modified" date, which `src/meta`
//! re-exports as consts. Every binary's `--version`/`--help` footer reports these.
//!
//! Inlined from the former `bismark-meta/build.rs` when the 14 crates folded into
//! one (epic `plans/07062026_single-binary-suite/`). Two deliberate changes vs the
//! former per-crate build.rs:
//!  - the SUITE version comes from `$BISMARK_SUITE_VERSION` (CI) or `../VERSION`
//!    (`rust/VERSION`); a crate-local vendored `VERSION` tier (re-added in Phase 4 — a
//!    registry-build fallback is re-added in the Phase-4 packaging rework if needed).
//!  - a SINGLE `BISMARK_LAST_MODIFIED` (D4 uniform footer): `git log -1` HEAD date,
//!    not a per-module date (`git log -- src/<mod>/` can't follow the fold rename).

use std::env;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn git_short_hash() -> String {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn build_epoch() -> u64 {
    match env::var("SOURCE_DATE_EPOCH") {
        Ok(s) => s.trim().parse::<u64>().unwrap_or_else(|_| {
            panic!(
                "SOURCE_DATE_EPOCH must be a non-negative decimal seconds-since-epoch integer, got {s:?}"
            )
        }),
        Err(_) => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before 1970-01-01")
            .as_secs(),
    }
}

fn format_iso8601_utc(epoch: u64) -> String {
    let secs_of_day = epoch % 86_400;
    let days = epoch / 86_400;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

// Howard Hinnant's days-from-civil algorithm (public domain). Days since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = y + i64::from(m <= 2);
    (year, m, d)
}

/// SUITE version: `$BISMARK_SUITE_VERSION` (CI/Docker) → `../VERSION` (`rust/VERSION`,
/// workspace/local) → crate-local vendored `VERSION` (registry `cargo install`) →
/// `"unknown"`. (The vendored tier was re-added in the Phase-4 packaging rework.)
fn suite_version() -> String {
    if let Ok(v) = env::var("BISMARK_SUITE_VERSION") {
        let v = v.trim();
        if !v.is_empty() {
            return v.to_string();
        }
    }
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let repo_version = std::path::Path::new(&manifest).join("..").join("VERSION");
    // crate-local vendored `VERSION`: a registry `cargo install bismark` tarball
    // can't reach `../VERSION` (outside the crate root), so without this a bare
    // registry install reports "unknown" (the parked-2.0.1 bug class). Kept in
    // lockstep with `rust/VERSION` by `meta::tests::vendored_version_matches_repo_version`.
    let vendored = std::path::Path::new(&manifest).join("VERSION");
    for path in [repo_version, vendored] {
        if let Some(v) = std::fs::read_to_string(&path)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            return v;
        }
    }
    "unknown".to_string()
}

/// A SINGLE suite-wide "Last modified" date (`YYYY-MM-DD`) — the HEAD commit date
/// (D4 uniform footer). Falls back to the build date when there is no git checkout.
fn last_modified(build_timestamp: &str) -> String {
    Command::new("git")
        .args(["log", "-1", "--format=%cd", "--date=short"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            build_timestamp
                .split('T')
                .next()
                .unwrap_or("unknown")
                .to_string()
        })
}

/// Resolve the real path of a git metadata file (`HEAD`/`index`) via git so the
/// rerun trigger works in a worktree (`.git` is a file) and from a sub-dir crate.
fn git_path(file: &str) -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--git-path", file])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn main() {
    let hash = git_short_hash();
    let timestamp = format_iso8601_utc(build_epoch());
    let version = suite_version();
    let last_mod = last_modified(&timestamp);
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| "unknown".to_string());
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_else(|_| "unknown".to_string());
    let version_body = format!("{hash} — {target_os}/{target_arch} — built {timestamp}");

    println!("cargo:rustc-env=BISMARK_SUITE_VERSION={version}");
    println!("cargo:rustc-env=GIT_SHORT_HASH={hash}");
    println!("cargo:rustc-env=BUILD_TIMESTAMP={timestamp}");
    println!("cargo:rustc-env=VERSION_BODY={version_body}");
    println!("cargo:rustc-env=BISMARK_LAST_MODIFIED={last_mod}");

    for f in ["HEAD", "index"] {
        if let Some(p) = git_path(f) {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    println!("cargo:rerun-if-changed=../VERSION");
    println!("cargo:rerun-if-changed=VERSION"); // crate-local vendored copy (registry-build fallback)
    println!("cargo:rerun-if-env-changed=BISMARK_SUITE_VERSION");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
}
