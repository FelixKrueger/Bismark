//! Binary entry point for `methylation_consistency`.
//!
//! Parses CLI via [`bismark_methylation_consistency::Cli`], validates into a
//! [`ResolvedConfig`], then runs the per-file consistency split via
//! [`pipeline::run`].
//!
//! Exit codes:
//! - `0` — success
//! - `1` — any [`MethConsError`]
//! - `2` — clap parse error (clap convention for usage errors)

use std::process::ExitCode;

use clap::Parser;

use bismark_methylation_consistency::cli::Cli;
use bismark_methylation_consistency::error::MethConsError;
use bismark_methylation_consistency::pipeline;
use bismark_methylation_consistency::version_string;

fn main() -> ExitCode {
    let cli = Cli::parse();

    // `--version` / `-V` handled here (clap auto-version disabled in cli.rs).
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

fn run(cli: Cli) -> Result<(), MethConsError> {
    let config = cli.validate()?;
    pipeline::run(&config)
}
