//! `bismark-coverage2cytosine` — Rust port of Bismark Perl's `coverage2cytosine`.
//!
//! Reads a Bismark coverage file (`*.bismark.cov[.gz]`) + the genome FASTA and
//! emits a genome-wide per-cytosine report (CpG by default; all-context with
//! `--CX`). This crate is the second producer (after `bismark-bedgraph`,
//! epic #797) that unblocks the extractor's Phase H sub-gate 2 byte-identity
//! gate. The binary is installed as `coverage2cytosine_rs`.
//!
//! See `plans/05292026_bismark-coverage2cytosine/SPEC.md` (rev 3) for the
//! design contract and the byte-identity-vs-Perl-v0.25.1 discipline.
//!
//! ## Status
//!
//! **Phase C** — genome-wide report with `--gzip` + `--split_by_chromosome`,
//! on top of the Phase B core (CpG / `--CX`, `--zero_based`,
//! `--coverage_threshold`, cytosine-context summary) and Phase A (CLI/validation
//! + genome reader). Public surface:
//!
//! - [`cli::Cli`] / [`cli::ResolvedConfig`] — clap parser + validated config.
//! - [`genome::Genome`] — whole-genome FASTA reader.
//! - [`run`] — load the genome + generate the report(s) + summary (single or
//!   per-chromosome; plain or gzip).
//! - [`error::BismarkC2cError`] — typed errors.
//!
//! `--merge_CpGs` (Phase D) and the real-data byte-identity gate (Phase E)
//! land next.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod cov;
pub mod error;
pub mod genome;
pub mod merge;
pub mod report;
pub mod summary;

pub use cli::{Cli, ResolvedConfig};
pub use error::BismarkC2cError;
pub use genome::Genome;

/// Run the genome-wide cytosine report: load the genome into memory, then
/// stream the coverage file and emit the report + cytosine-context summary
/// (Phase B; plain output). Mirrors Perl `coverage2cytosine`'s top-level flow.
pub fn run(config: &ResolvedConfig) -> Result<(), BismarkC2cError> {
    let genome = Genome::load(&config.genome_folder)?;
    eprintln!(
        "Stored sequence information of {} chromosomes/scaffolds in total",
        genome.len()
    );
    report::run_report(config, &genome)?;
    // Phase D: --merge_CpGs post-pass (re-reads the just-written CpG report).
    if config.merge_cpgs {
        merge::run_merge(config)?;
    }
    Ok(())
}

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
