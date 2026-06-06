//! `bismark-extractor` — Rust port of Bismark Perl's `bismark_methylation_extractor`.
//!
//! This crate is the biggest single-tool rewrite in the Bismark Rust workspace
//! — Perl source is 6,050 LOC across 35 CLI flags. Implementation is phased
//! per the design contract at [`bismark-extractor/SPEC.md`](../SPEC.md).
//!
//! ## Status
//!
//! **Phase E — non-Default output modes + `--gzip` + `--mbias_only`** (crate
//! version: `1.0.0-alpha.5`). The binary runs end-to-end on Bismark
//! BAM/SAM/CRAM input at `--parallel 1` across the full output-shape
//! surface:
//!   - `Default` (12 strand×context files),
//!   - `Comprehensive` (3 per-context files with `_context_` infix),
//!   - `MergeNonCpG` (8 files: CpG×4 + Non_CpG×4 strands),
//!   - `ComprehensiveMergeNonCpG` (2 files),
//!   - `Yacht` (1 file `any_C_context_*` with 8-col rows; SE-only),
//!   - `MbiasOnly` (0 split files; M-bias.txt + splitting-report only).
//!
//! `--gzip` wraps every per-mode split file in a parallel-gzip
//! `gzp::par::compress::ParCompress` writer (#884 R2) and appends `.gz` to
//! filenames. `--mbias_only` silently skips `InvalidXmByte` errors (per Perl
//! `:2972/3054`).
//!
//! SE + PE both run end-to-end, with SE-vs-PE auto-detect via
//! `@PG ID:Bismark` header probe. M-bias.txt + `_splitting_report.txt`
//! emit per Phase D's byte-identity contract. `--multicore` (Phase F) is
//! supported. `--bedGraph` / `--cytosine_report` drive the `bismark2bedGraph`
//! and `coverage2cytosine` tools **in-process** (inline-streaming epic Phase
//! 2; no fork/exec, no Perl): the extractor builds the argv each tool's CLI
//! accepts, parses and validates it via that crate's `Cli`, and calls its
//! `run()` on the per-context files. Multiple input files are still
//! rejected with [`BismarkExtractorError::PhaseNotYetImplemented`].
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
//! - [`params::ExtractParams`] — scaffold for later-phase parameter
//!   structs; not yet used.
//! - [`call::extract_calls`] — kernel (Phase B).
//! - [`pipeline::extract_se`] — SE main loop (Phase B).
//! - [`pipeline::extract_pe`] — PE main loop (Phase C).
//! - [`overlap::drop_overlap`] — PE overlap-detection filter (Phase C).
//! - [`mbias::MbiasTable`] — M-bias accumulator (Phase B + C).
//! - [`mbias_writer::write_mbias_txt`] — M-bias.txt writer (Phase D).
//!
//! ## Binary
//!
//! Installs as `bismark_methylation_extractor_rs` (with `_rs` suffix
//! during the Perl → Rust coexistence period; matches the `bismark-dedup`
//! precedent).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod call;
pub mod cli;
pub mod downstream_filenames;
pub mod error;
pub mod header;
pub mod logging;
pub mod mbias;
pub mod mbias_writer;
pub mod output;
pub mod output_mode;
pub mod overlap;
pub mod parallel;
pub mod params;
pub mod pipeline;
pub mod route;
pub mod state;

pub use call::{CytosineContext, MethCall, extract_calls};
pub use cli::{Cli, OutputMode, PairedMode, ResolvedConfig};
pub use error::BismarkExtractorError;
pub use mbias::{MbiasPos, MbiasTable};
pub use mbias_writer::{derive_mbias_basename, mbias_txt_path, write_mbias_txt};
pub use output_mode::{
    CpGOrNonCpG, OutputKey, mode_keys, orient_byte, route_to_key, write_yacht_row,
};
pub use overlap::{drop_overlap, is_forward_pair_strand};
pub use parallel::{extract_pe_parallel, extract_se_parallel};
pub use params::ExtractParams;
// PHASE F INVARIANT: the legacy single-threaded `extract_se` / `extract_pe`
// remain re-exported because they're the byte-identity reference for
// `extract_se_parallel` / `extract_pe_parallel` tests. Do NOT delete them
// without replacing the byte-identity oracle.
pub use pipeline::{extract_pe, extract_se};

/// Returns a TG-style provenance string for the binary's `--version` output.
///
/// Format: `bismark_methylation_extractor_rs <semver> (<os>/<arch>)`.
/// Matches the `bismark-dedup` precedent. Phase H will extend this with
/// git commit hash + ISO-8601 build timestamp via `build.rs`.
#[must_use]
pub fn version_string() -> String {
    format!(
        "bismark_methylation_extractor_rs {} ({}/{})",
        bismark_meta::SUITE_VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}
