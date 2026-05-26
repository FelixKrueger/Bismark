//! Binary entry point for `bismark-methylation-extractor-rs`.
//!
//! Phase C (rev 1): dispatches on the resolved config —
//! - `SingleEnd` → [`bismark_extractor::extract_se`].
//! - `PairedEnd` → [`bismark_extractor::extract_pe`].
//! - `AutoDetect` → header-probe via `bismark_io::detect_paired_from_header`
//!   to pick `extract_se` / `extract_pe`. Errors with `AutoDetectFailed`
//!   if the BAM has no `@PG ID:Bismark*` line.
//!
//! Non-default output modes, `--gzip`, `--parallel > 1`, `--bedGraph` /
//! `--cytosine_report`, and multiple input files are still rejected with
//! [`BismarkExtractorError::PhaseNotYetImplemented`].
//!
//! Exit codes:
//! - `0` — success
//! - `1` — any [`BismarkExtractorError`]
//! - `2` — clap parse error (clap convention)

use std::process::ExitCode;

use clap::Parser;

use bismark_extractor::cli::{Cli, OutputMode, PairedMode};
use bismark_extractor::error::BismarkExtractorError;
use bismark_extractor::{extract_pe, extract_se, version_string};
use bismark_io::{detect_paired_from_header, open_reader};

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

    // Non-default output modes: Phase E.
    if config.output_mode != OutputMode::Default {
        return Err(BismarkExtractorError::PhaseNotYetImplemented {
            feature: format!(
                "output mode {:?}; --comprehensive / --merge_non_CpG / --yacht / \
                 --mbias_only arrive in Phase E",
                config.output_mode
            ),
        });
    }

    // Gzip-compressed output: Phase E.
    if config.gzip {
        return Err(BismarkExtractorError::PhaseNotYetImplemented {
            feature: "--gzip; arrives in Phase E".to_string(),
        });
    }

    // Multicore: Phase F.
    if config.parallel != 1 {
        return Err(BismarkExtractorError::PhaseNotYetImplemented {
            feature: format!(
                "--parallel {} (only --parallel 1 supported); multicore arrives in Phase F",
                config.parallel
            ),
        });
    }

    // Downstream subprocess chain: Phase G.
    if config.bedgraph || config.cytosine_report {
        return Err(BismarkExtractorError::PhaseNotYetImplemented {
            feature: "--bedGraph / --cytosine_report subprocess chain; arrives in Phase G"
                .to_string(),
        });
    }

    // Supported subset: SE OR PE in default mode at parallel=1, plain output,
    // no subprocess chain. PairedMode dispatch:
    //   - SingleEnd → extract_se.
    //   - PairedEnd → extract_pe.
    //   - AutoDetect → probe the SAM header's @PG ID:Bismark line; dispatch.
    let input = config.files[0].clone();
    match config.paired_mode {
        PairedMode::SingleEnd => extract_se(&input, &config),
        PairedMode::PairedEnd => extract_pe(&input, &config),
        PairedMode::AutoDetect => {
            // Open reader once for header inspection. The reader is dropped
            // before the chosen extract_se / extract_pe re-opens the file —
            // ~50 ms overhead per run, OS caches the BAM header bytes.
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
                extract_pe(&input, &config)
            } else {
                extract_se(&input, &config)
            }
        }
    }
}
