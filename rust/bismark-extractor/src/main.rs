//! Binary entry point for `bismark_methylation_extractor_rs`.
//!
//! Processes EACH input file independently (per-file outputs, no
//! cross-file pooling — faithful to Perl's `foreach my $filename`), then
//! dispatches each on the resolved config —
//! - `SingleEnd` → [`bismark_extractor::extract_se_parallel`].
//! - `PairedEnd` → [`bismark_extractor::extract_pe_parallel`].
//! - `AutoDetect` → header-probe via `bismark_io::detect_paired_from_header`
//!   to pick SE / PE. Errors with `AutoDetectFailed`
//!   if the BAM has no `@PG ID:Bismark*` line.
//!
//! Coordinate-sorted input: accepted for SINGLE-END (SE calls are
//! order-independent — faithful to Perl, which only sort-checks paired-end),
//! rejected for PAIRED-END with `UnsortedInput` (coordinate sorting breaks
//! adjacent-mate pairing). The AutoDetect probe opens without the sort check
//! so a coordinate-sorted SE file can be inspected; the PE branch re-opens
//! WITH the check.
//!
//! This build supports all 6 output modes
//! (`Default` / `Comprehensive` / `MergeNonCpG` /
//! `ComprehensiveMergeNonCpG` / `Yacht` / `MbiasOnly`), `--gzip`,
//! `--parallel > 1` (Phase F), `--bedGraph` / `--cytosine_report`
//! (inline-streaming epic Phase 2 — driven in-process from `state.finalize`),
//! and multiple input files (v1.x — looped per-file).
//!
//! Exit codes:
//! - `0` — success
//! - `1` — any [`BismarkExtractorError`]
//! - `2` — clap parse error (clap convention)

use std::process::ExitCode;

use clap::Parser;

use std::path::Path;

use bismark_extractor::cli::{Cli, PairedMode, ResolvedConfig};
use bismark_extractor::error::BismarkExtractorError;
use bismark_extractor::{extract_pe_parallel, extract_se_parallel, version_string};
use bismark_io::{detect_paired_from_header, open_reader_without_sort_check};

// Multithreaded global allocator (#884). The parallel pipeline's worker threads
// allocate heavily per record (record parsing, call Vecs, batch Vecs); under the
// default system allocator they blocked on arena locks, making `--parallel N>1`
// run ~2x SLOWER than N=1 (`top` showed only ~364% CPU at `--parallel 8` — i.e.
// blocking-bound, not CPU-bound). mimalloc removes that contention: default N=4
// dropped 155.8s -> 23.5s and the anti-scaling vanished. Allocator choice does
// not affect computed output — byte-identity to the system allocator holds
// (guarded by the `parallel_phase_f` N≡1 tests + the Phase H matrix).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    let cli = Cli::parse();

    // `--version` / `-V` handled here (clap auto-version disabled in cli.rs
    // so we can emit the TG-style provenance string).
    if cli.version {
        println!("{}", version_string());
        return ExitCode::SUCCESS;
    }

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
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
