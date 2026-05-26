//! Binary entry point for `bismark-methylation-extractor-rs`.
//!
//! Phase A: parses CLI, validates flags, prints provenance on `--version`.
//! NO extraction logic — that's Phase B onward. A run that passes
//! validation currently prints a one-line note + exits 0 (placeholder
//! pipeline call site).
//!
//! Exit codes:
//! - `0` — success (currently: validation passes + placeholder exit)
//! - `1` — any [`BismarkExtractorError`]
//! - `2` — clap parse error (clap convention)

use std::process::ExitCode;

use clap::Parser;

use bismark_extractor::cli::Cli;
use bismark_extractor::error::BismarkExtractorError;
use bismark_extractor::version_string;

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
    let _config = cli.validate()?;

    // Phase A placeholder: validation passed, but no pipeline exists yet.
    // Phase B lands the SE extraction loop and wires it here. For now,
    // explicitly tell the user the binary is not yet feature-complete.
    eprintln!(
        "note: bismark-methylation-extractor-rs is in Phase A (scaffold + CLI). \
         Extraction pipeline lands in Phase B (SE) through G (subprocess chain). \
         For production use, run Perl `bismark_methylation_extractor` until \
         Phase H (byte-identity gate) is reached. See \
         <https://github.com/FelixKrueger/Bismark/issues/798> for status."
    );
    Ok(())
}
