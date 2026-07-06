//! Binary entry point for `bismark_genome_preparation`.
//!
//! Exit codes: `0` success · `1` any [`GenomePrepError`] · `2` clap parse error.

use std::process::ExitCode;

use clap::{CommandFactory, Parser};

use bismark_genome_preparation::cli::Cli;
use bismark_genome_preparation::{run, version_string};

fn main() -> ExitCode {
    let cli = Cli::parse();

    // `--version` / `-V` handled manually (clap auto-version disabled) so we
    // can print the Bismark provenance banner.
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

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
