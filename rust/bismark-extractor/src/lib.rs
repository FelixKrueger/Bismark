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
//! Installs as `bismark_methylation_extractor` (the canonical name; the
//! beta-era `_rs` suffix is retired at GA).

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

use std::path::Path;

use bismark_io::{detect_paired_from_header, open_reader_without_sort_check};

/// One-line `--version` string for the binary (suite-wide shape via
/// [`bismark_meta::version_line`]):
/// `bismark_methylation_extractor (Bismark Rust suite) v<semver> (<hash> — <os>/<arch> — built <ts>)`.
#[must_use]
pub fn version_string() -> String {
    bismark_meta::version_line("bismark_methylation_extractor")
}

/// Binary entry point — shared by this crate's own `main.rs` and the `bismark`
/// meta-crate's `bismark_methylation_extractor` bin (so `cargo install bismark`
/// and `cargo install bismark-extractor` behave identically). Parses the CLI,
/// handles `--version`, then dispatches to [`run`]. Exit: `0` ok · `1` error
/// (clap handles `2` parse errors before this). The `#[global_allocator]` stays
/// in each binary crate root.
#[must_use]
pub fn run_main() -> std::process::ExitCode {
    use clap::Parser;
    let cli = Cli::parse();

    // `--version` / `-V` handled here (clap auto-version disabled in cli.rs
    // so we can emit the TG-style provenance string).
    if cli.version {
        println!("{}", version_string());
        return std::process::ExitCode::SUCCESS;
    }

    match run(cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<(), BismarkExtractorError> {
    let config = cli.validate()?;

    // Multiple input files (v1.x): process EACH file independently, in
    // command-line order, with full per-file state — faithful to Perl's
    // `foreach my $filename (@filenames)` loop (no cross-file pooling; each
    // file's split files / M-bias / splitting report / bedGraph / cytosine
    // report are named from that file's basename). Fail-fast: the first
    // file that errors aborts the run (propagate via `?`), matching Perl's
    // `die`; files already completed keep their outputs. The empty-file-list
    // case is handled earlier by `validate()` (`NoInputFiles`), so this loop
    // always runs at least once.
    for input in &config.files {
        process_one_file(input, &config)?;
    }
    Ok(())
}

/// Extract one input file end-to-end. Dispatches on `paired_mode`:
///   - `SingleEnd` → `extract_se_parallel` (accepts coordinate-sorted input).
///   - `PairedEnd` → `extract_pe_parallel` (rejects coordinate-sorted input).
///   - `AutoDetect` → probe the `@PG ID:Bismark` header (without the sort
///     check, so a coordinate-sorted SE file can be inspected), then
///     dispatch. The PE branch re-opens WITH the sort check, so a
///     coordinate-sorted PE file is still rejected with `UnsortedInput`.
///
/// The parallel pipeline is byte-identical to `--parallel 1` for any N by
/// construction (SPEC §9). The legacy single-threaded `extract_se` /
/// `extract_pe` remain the byte-identity reference for the test suite.
fn process_one_file(input: &Path, config: &ResolvedConfig) -> Result<(), BismarkExtractorError> {
    match config.paired_mode {
        PairedMode::SingleEnd => extract_se_parallel(input, config),
        PairedMode::PairedEnd => extract_pe_parallel(input, config),
        PairedMode::AutoDetect => {
            // Open reader once for header inspection, WITHOUT the
            // coordinate-sort check (detection must work on a coord-sorted
            // SE file). The probe is dropped before extract_*_parallel
            // re-opens the file — OS caches the header bytes.
            let probe = open_reader_without_sort_check(input, /*cram_ref=*/ None)?;
            let is_paired = detect_paired_from_header(probe.header()).ok_or_else(|| {
                BismarkExtractorError::AutoDetectFailed {
                    message: format!(
                        "no `@PG` line with `ID:Bismark*` found in {}'s header; \
                         pass `--single-end` or `--paired-end` explicitly",
                        input.display()
                    ),
                }
            })?;
            drop(probe);
            if is_paired {
                // PE re-opens with the checking constructor → coordinate-sorted
                // PE input is rejected here with `UnsortedInput`.
                extract_pe_parallel(input, config)
            } else {
                extract_se_parallel(input, config)
            }
        }
    }
}
