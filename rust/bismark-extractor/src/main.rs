//! Binary entry point for `bismark_methylation_extractor_rs`.
//!
//! Dispatches on the resolved config —
//! - `SingleEnd` → [`bismark_extractor::extract_se`].
//! - `PairedEnd` → [`bismark_extractor::extract_pe`].
//! - `AutoDetect` → header-probe via `bismark_io::detect_paired_from_header`
//!   to pick `extract_se` / `extract_pe`. Errors with `AutoDetectFailed`
//!   if the BAM has no `@PG ID:Bismark*` line.
//!
//! This build supports all 6 output modes
//! (`Default` / `Comprehensive` / `MergeNonCpG` /
//! `ComprehensiveMergeNonCpG` / `Yacht` / `MbiasOnly`), `--gzip`,
//! `--parallel > 1` (Phase F), and `--bedGraph` / `--cytosine_report`
//! (inline-streaming epic Phase 2 — driven in-process from `state.finalize`).
//! Multiple input files are still rejected with
//! [`BismarkExtractorError::PhaseNotYetImplemented`].
//!
//! Exit codes:
//! - `0` — success
//! - `1` — any [`BismarkExtractorError`]
//! - `2` — clap parse error (clap convention)

use std::process::ExitCode;

use clap::Parser;

use bismark_extractor::cli::{Cli, PairedMode};
use bismark_extractor::error::BismarkExtractorError;
use bismark_extractor::{extract_pe_parallel, extract_se_parallel, version_string};
use bismark_io::{detect_paired_from_header, open_reader};

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

    // Phase B dispatches on the supported subset. Anything outside the
    // subset is rejected with `PhaseNotYetImplemented` naming the phase
    // that will land it. This avoids silent acceptance of half-implemented
    // code paths.

    // Multiple input files: deferred (would mirror dedup's --multiple).
    if config.files.len() != 1 {
        return Err(BismarkExtractorError::PhaseNotYetImplemented {
            feature: format!(
                "multiple input files ({} given); v1.x feature",
                config.files.len()
            ),
        });
    }

    // Phase F (this build): all 6 output modes + --gzip + --multicore N are
    // supported. The parallel pipeline is byte-identical to N=1 for any N
    // by construction (per SPEC §9 + PHASE_F_PLAN.md).
    //
    // Inline-streaming epic Phase 2: --bedGraph / --cytosine_report are now
    // driven IN-PROCESS from inside `state.finalize` (no main::run
    // orchestration is added here) — the prior `PhaseNotYetImplemented` gate
    // was removed.

    // Phase F dispatch (parallel pipeline; --parallel N for any N >= 1).
    // PairedMode dispatch:
    //   - SingleEnd → extract_se_parallel.
    //   - PairedEnd → extract_pe_parallel.
    //   - AutoDetect → probe the SAM header's @PG ID:Bismark line; dispatch.
    //
    // The legacy single-threaded `extract_se` / `extract_pe` remain available
    // via `bismark_extractor::{extract_se, extract_pe}` as the byte-identity
    // reference for the test suite (see lib.rs invariant comment).
    let input = config.files[0].clone();
    match config.paired_mode {
        PairedMode::SingleEnd => extract_se_parallel(&input, &config),
        PairedMode::PairedEnd => extract_pe_parallel(&input, &config),
        PairedMode::AutoDetect => {
            // Open reader once for header inspection. The reader is dropped
            // before extract_*_parallel re-opens the file — ~50 ms overhead
            // per run, OS caches the BAM header bytes.
            let probe = open_reader(&input, /*cram_ref=*/ None)?;
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
                extract_pe_parallel(&input, &config)
            } else {
                extract_se_parallel(&input, &config)
            }
        }
    }
}
