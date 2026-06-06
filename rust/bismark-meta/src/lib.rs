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
/// `bismark_rs (Bismark Rust suite) v2.0.0-beta.1 (abc1234 — linux/x86_64 — built 2026-…Z)`.
pub fn version_line(tool: &str) -> String {
    format!("{tool} (Bismark Rust suite) v{SUITE_VERSION} ({VERSION_BODY})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consts_are_populated() {
        // build.rs always sets these (with `unknown` fallbacks) — never empty.
        assert!(!SUITE_VERSION.is_empty());
        assert!(!GIT_SHORT_HASH.is_empty());
        assert!(!VERSION_BODY.is_empty());
    }

    #[test]
    fn version_line_shape() {
        let l = version_line("bismark_rs");
        assert!(l.starts_with("bismark_rs (Bismark Rust suite) v"));
        assert!(l.contains(SUITE_VERSION));
        assert!(l.contains(GIT_SHORT_HASH));
    }
}
