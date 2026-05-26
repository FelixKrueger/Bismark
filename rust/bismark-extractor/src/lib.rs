//! `bismark-extractor` — Rust port of Bismark Perl's `bismark_methylation_extractor`.
//!
//! This crate is the biggest single-tool rewrite in the Bismark Rust workspace
//! — Perl source is 6,050 LOC across 35 CLI flags. Implementation is phased
//! per the design contract at [`bismark-extractor/SPEC.md`](../SPEC.md).
//!
//! ## Status
//!
//! **Phase A — workspace scaffold + CLI** (this crate version: `1.0.0-alpha.1`).
//! The binary boots, `--help` prints all 35 flags, `--version` emits a
//! provenance string, and `Cli::validate()` rejects every documented flag
//! mutex from SPEC §11 + Perl source. No extraction logic yet — that's
//! Phase B (SE loop) through G (subprocess chain).
//!
//! See [SPEC.md §10](../SPEC.md) for the full phase outline.
//!
//! ## Library surface (Phase A)
//!
//! - [`cli::Cli`] — clap-derived parser matching all 35 Perl flags.
//! - [`cli::ResolvedConfig`] — validated subset of CLI args + derived
//!   [`cli::OutputMode`] and [`cli::PairedMode`].
//! - [`error::BismarkExtractorError`] — typed errors raised at validation
//!   + (later) the extraction-pipeline boundary.
//! - [`params::ExtractParams`] — Phase-B-onwards argument struct (typed
//!   replacement for the 14-arg `extract_calls` signature seen in the
//!   prior-art Rust port). Phase A defines the shape; Phase B populates.
//!
//! ## Binary
//!
//! Installs as `bismark-methylation-extractor-rs` (with `_rs` suffix
//! during the Perl → Rust coexistence period; matches the `bismark-dedup`
//! precedent).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod error;
pub mod params;

pub use cli::{Cli, OutputMode, PairedMode, ResolvedConfig};
pub use error::BismarkExtractorError;
pub use params::ExtractParams;

/// Returns a TG-style provenance string for the binary's `--version` output.
///
/// Format: `bismark-methylation-extractor-rs <semver> (<os>/<arch>)`.
/// Matches the `bismark-dedup` precedent. Phase H will extend this with
/// git commit hash + ISO-8601 build timestamp via `build.rs`.
#[must_use]
pub fn version_string() -> String {
    format!(
        "bismark-methylation-extractor-rs {} ({}/{})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}
