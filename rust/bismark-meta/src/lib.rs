//! Shared build-time version/provenance for the Bismark Rust suite.
//!
//! The **suite version** (single source: `rust/VERSION` or `$BISMARK_SUITE_VERSION`),
//! the git short-hash, and a reproducible build timestamp are captured at build
//! time by `build.rs`. Every Bismark Rust binary reports the *suite* version via
//! [`version_line`] — distinct from each crate's internal Cargo version (which is
//! reserved for the eventual crates.io publish at GA).

/// The suite version, e.g. `2.0.0-beta.1` (the user-facing version).
pub const SUITE_VERSION: &str = env!("BISMARK_SUITE_VERSION");
/// Git short-hash of the build commit (or `unknown`).
pub const GIT_SHORT_HASH: &str = env!("GIT_SHORT_HASH");
/// ISO-8601 UTC build timestamp (`SOURCE_DATE_EPOCH`-reproducible).
pub const BUILD_TIMESTAMP: &str = env!("BUILD_TIMESTAMP");
/// `<hash> — <os>/<arch> — built <timestamp>` provenance body.
pub const VERSION_BODY: &str = env!("VERSION_BODY");

/// A one-line `--version` string for a suite tool, e.g.
/// `bismark (Bismark Rust suite) v2.0.0 (abc1234 — linux/x86_64 — built 2026-…Z)`.
/// Every suite binary's `--version` is this exact shape (pass the CANONICAL tool
/// name — no `_rs` suffix).
pub fn version_line(tool: &str) -> String {
    format!("{tool} (Bismark Rust suite) v{SUITE_VERSION} ({VERSION_BODY})")
}

/// Build-time helper: the last-modified date (`YYYY-MM-DD`) of a crate directory,
/// from git (`git log -1 --date=short` filtered to that dir). Call it from a
/// dependent crate's `build.rs` (add `bismark-meta` as a `[build-dependencies]`)
/// to embed a per-tool "Last modified" date in that tool's `--help` footer.
///
/// Falls back to the build date (then `"unknown"`) when there is no git checkout
/// — e.g. a crates.io registry build from the packaged tarball. Release binaries,
/// the container, and `cargo install --git` are all built from a checkout, so
/// they bake the tool's true last-commit date.
pub fn last_modified_date(manifest_dir: &str) -> String {
    use std::process::Command;
    let from_git = Command::new("git")
        .current_dir(manifest_dir)
        .args(["log", "-1", "--format=%cd", "--date=short", "--", "."])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    from_git.unwrap_or_else(|| {
        // `BUILD_TIMESTAMP` is `YYYY-MM-DDThh:mm:ssZ`; take the date part.
        BUILD_TIMESTAMP
            .split('T')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("unknown")
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // (The consts' presence is compile-guaranteed: `env!()` is a compile error if
    // build.rs didn't set the var — so a runtime `!CONST.is_empty()` assert is a
    // tautology clippy flags as `const_is_empty`. `version_line_shape` covers the
    // meaningful behavior at runtime.)

    #[test]
    fn version_line_shape() {
        let l = version_line("bismark");
        assert!(l.starts_with("bismark (Bismark Rust suite) v"));
        assert!(l.contains(SUITE_VERSION));
        assert!(l.contains(GIT_SHORT_HASH));
        // Canonical name only — the `_rs` dev suffix is retired at GA.
        assert!(!l.contains("_rs"));
    }

    #[test]
    fn vendored_version_matches_repo_version() {
        // Drift guard: the VENDORED crate-local `VERSION` (build.rs's registry-build
        // fallback so a bare `cargo install` doesn't report "unknown") must equal the
        // canonical single-source `rust/VERSION`. Runs only under `cargo test`
        // (workspace present) — reads both at runtime, so it does not affect a
        // `cargo package` verify-build.
        let dir = env!("CARGO_MANIFEST_DIR");
        let vendored = std::fs::read_to_string(format!("{dir}/VERSION")).unwrap();
        let repo = std::fs::read_to_string(format!("{dir}/../VERSION")).unwrap();
        assert_eq!(
            vendored.trim(),
            repo.trim(),
            "vendored bismark-meta/VERSION drifted from rust/VERSION"
        );
    }
}
