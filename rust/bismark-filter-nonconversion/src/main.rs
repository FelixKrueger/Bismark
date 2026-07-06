//! Binary entry point for `filter_non_conversion`.
//!
//! Exit codes: `0` success; `1` any [`BismarkFilterError`] or no input files;
//! `2` clap parse error (clap convention).
//!
//! `--help` is clap's default (exit 0 — a documented deviation from Perl's
//! `print_helpfile`/`exit 1`, SPEC §10.1); `--version` is handled here
//! (clap's auto-version is disabled) and exits 0, matching Perl.

#![forbid(unsafe_code)]

use std::process::ExitCode;

use clap::Parser;

use bismark_filter_nonconversion::cli::Cli;
use bismark_filter_nonconversion::{run, version_string};

// Multithreaded global allocator (free ~10% win, byte-neutral; same pin as
// the extractor #884 / bedgraph #915).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    let cli = Cli::parse();

    // `--version` handled here (clap auto-version disabled in cli.rs).
    if cli.version {
        println!("{}", version_string());
        return ExitCode::SUCCESS;
    }

    // No-files check precedes option validation (Perl `@ARGV`-empty at line
    // 513, before the percentage/threshold checks) — so a bad option value
    // with no files yields the no-files message, not the option error.
    if cli.files.is_empty() {
        eprintln!(
            "Please provide one or more Bismark output files for non-bisulfite conversion filtering"
        );
        return ExitCode::from(1);
    }

    let config = match cli.validate() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };

    match run(&config) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
