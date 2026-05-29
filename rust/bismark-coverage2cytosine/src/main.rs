//! Binary entry point for `coverage2cytosine_rs`.
//!
//! Parses [`Cli`], handles `--version`, validates into a `ResolvedConfig`,
//! then runs the genome-wide cytosine report (Phase B).
//!
//! Exit codes: `0` success · `1` any [`BismarkC2cError`] · `2` clap parse
//! error (clap convention, emitted by `Cli::parse`).

use std::process::ExitCode;

use clap::Parser;

use bismark_coverage2cytosine::cli::Cli;
use bismark_coverage2cytosine::error::BismarkC2cError;
use bismark_coverage2cytosine::version_string;

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

fn run(cli: Cli) -> Result<(), BismarkC2cError> {
    let config = cli.validate()?;
    bismark_coverage2cytosine::run(&config)
}
