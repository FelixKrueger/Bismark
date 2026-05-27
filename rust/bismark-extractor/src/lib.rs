//! `bismark-extractor` — Rust port of Bismark Perl's `bismark_methylation_extractor`.
//!
//! This crate is the biggest single-tool rewrite in the Bismark Rust workspace
//! — Perl source is 6,050 LOC across 35 CLI flags. Implementation is phased
//! per the design contract at [`bismark-extractor/SPEC.md`](../SPEC.md).
//!
//! ## Status
//!
//! **Phase B — SE extraction loop** (crate version: `1.0.0-alpha.2`).
//! The binary now runs end-to-end on SE Bismark BAM/SAM/CRAM input in
//! `OutputMode::Default` at `--parallel 1`, producing the 12 strand×context
//! split files (eagerly opened with the Perl version header line) plus
//! `_splitting_report.txt`. PE, non-default modes, `--gzip`, `--parallel > 1`,
//! and the `--bedGraph` / `--cytosine_report` subprocess chain are rejected
//! at the resolved-config boundary with [`BismarkExtractorError::PhaseNotYetImplemented`].
//!
//! See [SPEC.md §10](../SPEC.md) for the full phase outline.
//!
//! ## Library surface
//!
//! - [`cli::Cli`] — clap-derived parser matching all 35 Perl flags.
//! - [`cli::ResolvedConfig`] — validated subset of CLI args + derived
//!   [`cli::OutputMode`] and [`cli::PairedMode`].
//! - [`error::BismarkExtractorError`] — typed errors raised at validation
//!   and the extraction-pipeline boundary.
//! - [`params::ExtractParams`] — scaffold for Phase C/D/E parameter
//!   structs; not yet used in Phase B.
//! - [`call::extract_calls`] — Phase B kernel.
//! - [`pipeline::extract_se`] — Phase B SE main loop.
//!
//! ## Binary
//!
//! Installs as `bismark-methylation-extractor-rs` (with `_rs` suffix
//! during the Perl → Rust coexistence period; matches the `bismark-dedup`
//! precedent).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod call;
pub mod cli;
pub mod error;
pub mod header;
pub mod mbias;
pub mod output;
pub mod params;
pub mod pipeline;
pub mod route;
pub mod state;

pub use call::{CytosineContext, MethCall, extract_calls};
pub use cli::{Cli, OutputMode, PairedMode, ResolvedConfig};
pub use error::BismarkExtractorError;
pub use mbias::{MbiasPos, MbiasTable};
pub use params::ExtractParams;
pub use pipeline::extract_se;

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
