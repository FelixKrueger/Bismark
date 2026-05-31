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
//! **v1.0 (Phases A–E) shipped + byte-identity-proven** (tagged
//! `bismark-coverage2cytosine-v1.0.0-beta.1`): the genome-wide report
//! (CpG / `--CX`, `--zero_based`, `--coverage_threshold`, cytosine-context
//! summary), `--gzip` + `--split_by_chromosome`, and the `--merge_CpGs`
//! (+ `--discordance_filter`) post-pass. **v1.x Phase 1** adds the
//! `--gc`/`--gc_context` GpC-context report and `--nome-seq` NOMe-Seq
//! filtering ([`gpc`]). Public surface:
//!
//! - [`cli::Cli`] / [`cli::ResolvedConfig`] — clap parser + validated config.
//! - [`genome::Genome`] — whole-genome FASTA reader.
//! - [`run`] — load the genome + generate the report(s) + summary, the
//!   `--merge_CpGs` post-pass, and the `--gc`/`--nome-seq` GpC report.
//! - [`error::BismarkC2cError`] — typed errors.
//!
//! Remaining v1.x niche modes (`--drach`/`--m6A`, `--ffs`) are still rejected
//! at the CLI (Phases 2–3).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod cov;
pub mod error;
pub mod genome;
pub mod gpc;
pub mod merge;
pub mod report;
pub mod summary;

pub use cli::{Cli, ResolvedConfig};
pub use error::BismarkC2cError;
pub use genome::Genome;

/// Run the genome-wide cytosine report: load the genome into memory, stream the
/// coverage file and emit the report + cytosine-context summary, then the
/// optional `--merge_CpGs` post-pass and the optional `--gc`/`--nome-seq`
/// GpC-context report. Mirrors Perl `coverage2cytosine`'s top-level flow
/// (`:44` report → `:49` summary → `:58` merge → `:82` GpC).
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
    // Phase 1 (v1.x): the GpC-context report runs LAST (Perl main flow :82),
    // after the core report + summary (and after any --merge_CpGs pass). Set by
    // both --gc/--gc_context and --nome-seq. --nome-seq ✗ --merge_CpGs, so the
    // merge and GpC arms never co-occur.
    if config.gc_context {
        gpc::run_gpc(config, &genome)?;
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
