//! Binary entry point for `bismark-methylation-extractor-rs`.
//!
//! Phase B (rev 1): dispatches on the resolved config — SE + default mode +
//! parallel=1 + no gzip + no bedGraph/cytosine_report + single input file
//! routes to [`bismark_extractor::extract_se`]. Every other configuration
//! returns a [`BismarkExtractorError::PhaseNotYetImplemented`] naming the
//! deferring phase.
//!
//! Exit codes:
//! - `0` — success
//! - `1` — any [`BismarkExtractorError`]
//! - `2` — clap parse error (clap convention)

use std::process::ExitCode;

use clap::Parser;

use bismark_extractor::cli::{Cli, OutputMode, PairedMode};
use bismark_extractor::error::BismarkExtractorError;
use bismark_extractor::{extract_se, version_string};

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

    // Paired-end: Phase C.
    if config.paired_mode == PairedMode::PairedEnd {
        return Err(BismarkExtractorError::PhaseNotYetImplemented {
            feature: "paired-end extraction; arrives in Phase C".to_string(),
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

    // Supported subset: SE (or AutoDetect treated as SE; per-record PAIRED-flag
    // check inside the loop catches PE BAMs). Default mode. Single core.
    // Plain (uncompressed) output. No subprocess chain.
    let input = config.files[0].clone();
    extract_se(&input, &config)
}
