//! Binary entry point for `bismark2report_rs`.
//!
//! Exit codes: `0` success · `1` any [`ReportError`] · `2` clap parse error.
//! `--help`/`--man`/`--version` all exit `0` (Perl's exit-1-on-help quirk is
//! intentionally NOT reproduced — PLAN §6.1).

use std::process::ExitCode;

use clap::{CommandFactory, Parser};

use bismark_report::cli::Cli;
use bismark_report::{run, version_string};

fn main() -> ExitCode {
    let cli = Cli::parse();

    // `--version` handled manually (clap auto-version disabled) so we print the
    // Bismark provenance banner.
    if cli.version {
        println!("{}", version_string());
        return ExitCode::SUCCESS;
    }
    // `--man` aliases `--help` (Perl behavior): print the long help.
    if cli.man {
        let _ = Cli::command().print_long_help();
        println!();
        return ExitCode::SUCCESS;
    }

    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
