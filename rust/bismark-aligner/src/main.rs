//! Binary entry point for `bismark_rs` (Phase 1).
//!
//! Exit codes: `0` success · `1` any [`bismark_aligner::AlignerError`] ·
//! `2` clap parse error.

use std::process::ExitCode;

use clap::Parser;

use bismark_aligner::cli::Cli;
use bismark_aligner::{run, version_string};

fn main() -> ExitCode {
    // Capture the verbatim argv (program name excluded) BEFORE parsing — this is
    // the `@PG` `CL:` string (Perl captures `join(" ",@ARGV)` at startup, line 32).
    let raw: Vec<String> = std::env::args().collect();
    let command_line = raw.get(1..).unwrap_or(&[]).join(" ");

    let cli = Cli::parse_from(&raw);

    // `--version` handled manually (clap auto-version disabled) to print the
    // Bismark provenance banner.
    if cli.version {
        println!("{}", version_string());
        return ExitCode::SUCCESS;
    }

    match run(&cli, command_line) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
