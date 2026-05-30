//! Binary entry point for `bismark2bedGraph_rs`.
//!
//! Parses CLI via [`bismark_bedgraph::cli::Cli`], handles `--version` /
//! `--man` short-circuits, validates into a
//! [`bismark_bedgraph::cli::ResolvedConfig`], then runs the conversion via
//! [`bismark_bedgraph::run`].
//!
//! Exit codes:
//! - `0` — success
//! - `1` — any [`BismarkBedgraphError`]
//! - `2` — clap parse error (clap convention for usage errors)

use std::process::ExitCode;

use clap::{CommandFactory, Parser};

use bismark_bedgraph::cli::Cli;
use bismark_bedgraph::error::BismarkBedgraphError;
use bismark_bedgraph::version_string;

fn main() -> ExitCode {
    let cli = Cli::parse();

    // `--version` is handled here (clap's auto-version is disabled in
    // src/cli.rs so we can emit our custom provenance string).
    if cli.version {
        println!("{}", version_string());
        return ExitCode::SUCCESS;
    }

    // `--man` is Perl's alias for the long help text.
    if cli.man {
        let mut cmd = Cli::command();
        // print_long_help writes to stdout; ignore the unlikely write error.
        let _ = cmd.print_long_help();
        println!();
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

fn run(cli: Cli) -> Result<(), BismarkBedgraphError> {
    let config = cli.validate()?;
    bismark_bedgraph::run(&config)
}
