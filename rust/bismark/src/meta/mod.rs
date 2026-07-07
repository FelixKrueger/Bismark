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
/// `bismark (Bismark Rust suite) v3.0.0 (abc1234 — linux/x86_64 — built 2026-…Z)`.
/// Every suite binary's `--version` is this exact shape (pass the CANONICAL tool
/// name — no `_rs` suffix).
pub fn version_line(tool: &str) -> String {
    format!("{tool} (Bismark Rust suite) v{SUITE_VERSION} ({VERSION_BODY})")
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

    /// Drift guard: the crate-local vendored `VERSION` (build.rs's registry-build
    /// fallback so a bare `cargo install bismark` doesn't report "unknown") must equal
    /// the single-source `rust/VERSION`. SKIPS gracefully when `../VERSION` is absent
    /// (the packaged-crate context — only the vendored copy ships in the tarball), so
    /// it doesn't reintroduce the parked-2.0.1 fragility.
    #[test]
    fn vendored_version_matches_repo_version() {
        let dir = env!("CARGO_MANIFEST_DIR");
        let vendored = std::fs::read_to_string(format!("{dir}/VERSION"))
            .expect("crate-local bismark/VERSION must exist (Phase-4 vendored copy)");
        match std::fs::read_to_string(format!("{dir}/../VERSION")) {
            Ok(repo) => assert_eq!(
                vendored.trim(),
                repo.trim(),
                "vendored bismark/VERSION drifted from rust/VERSION"
            ),
            Err(_) => { /* packaged context: ../VERSION absent — nothing to compare */ }
        }
    }

    /// **Publish-version guard (load-bearing for a GA cut).** `cargo publish` registers
    /// the version literal in `bismark/Cargo.toml` (`CARGO_PKG_VERSION`), which is NOT
    /// `version.workspace` and is decoupled from `rust/VERSION`. If it drifts, the crate
    /// on crates.io reports a different version than the binary's `--version` / the release
    /// tag / the image — the exact 2.0.1 `cargo install` bug. Unlike the vendored guard
    /// above this NEVER skips (it runs in every context, incl. the packaged crate), so a
    /// GA that bumped `rust/VERSION` but forgot `Cargo.toml` fails `cargo test` before the
    /// irreversible publish.
    #[test]
    fn cargo_pkg_version_matches_suite_version() {
        assert_eq!(
            env!("CARGO_PKG_VERSION"),
            SUITE_VERSION,
            "bismark/Cargo.toml version (what `cargo publish` registers) drifted from the suite version (rust/VERSION)"
        );
    }
}
