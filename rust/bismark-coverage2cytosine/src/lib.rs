//! `bismark-coverage2cytosine` — Rust port of Bismark Perl's `coverage2cytosine`.
//!
//! Reads a Bismark coverage file (`*.bismark.cov[.gz]`) + the genome FASTA and
//! emits a genome-wide per-cytosine report (CpG by default; all-context with
//! `--CX`). This crate is the second producer (after `bismark-bedgraph`,
//! epic #797) that unblocks the extractor's Phase H sub-gate 2 byte-identity
//! gate. The binary is installed as `coverage2cytosine_rs`.
//!
//! See `plans/05292026_bismark-coverage2cytosine/SPEC.md` (rev 2) for the
//! design contract and the byte-identity-vs-Perl-v0.25.1 discipline.
//!
//! ## Status
//!
//! **Phase A** — scaffold + CLI/validation + genome reader. Public surface:
//!
//! - [`cli::Cli`] / [`cli::ResolvedConfig`] — clap parser + validated config.
//! - [`genome::Genome`] — whole-genome FASTA reader (uppercased, Perl-quirk
//!   faithful: `Mus_musculus.NCBIM37.fa` skip, four-suffix glob priority,
//!   duplicate-name + malformed-file detection, `u32` length guard).
//! - [`error::BismarkC2cError`] — typed errors.
//!
//! The genome-wide report algorithm, `--gzip`/`--split_by_chromosome`,
//! `--merge_CpGs`, and the real-data byte-identity gate land in Phases B–E.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod error;
pub mod genome;

pub use cli::{Cli, ResolvedConfig};
pub use error::BismarkC2cError;
pub use genome::Genome;

/// TG-style provenance string for the binary's `--version` output.
///
/// Format: `coverage2cytosine_rs <semver> (<os>/<arch>)`.
#[must_use]
pub fn version_string() -> String {
    format!(
        "coverage2cytosine_rs {} ({}/{})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}
