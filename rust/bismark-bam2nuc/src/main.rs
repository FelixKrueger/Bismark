//! Binary entry point for `bam2nuc_rs`.
//!
//! Parses [`Cli`], handles `--version`, validates into a [`ResolvedConfig`],
//! then runs the nucleotide-coverage report.
//!
//! Exit codes: `0` success · `1` any [`BismarkBam2nucError`] · `2` clap parse
//! error (clap convention, emitted by `Cli::parse`).

use std::process::ExitCode;

use clap::Parser;

use bismark_bam2nuc::cli::Cli;
use bismark_bam2nuc::error::BismarkBam2nucError;
use bismark_bam2nuc::version_string;

// Multithreaded allocator (#884/#915 sibling precedent). Allocator-only — the
// per-read counting loop allocates a span Vec per read; mimalloc trims the
// malloc cost. Output is byte-identical.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

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

fn run(cli: Cli) -> Result<(), BismarkBam2nucError> {
    let config = cli.validate()?;
    bismark_bam2nuc::run(&config)
}
