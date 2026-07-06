//! Binary entry point for `bismark` (Phase 1).
//!
//! Exit codes: `0` success · `1` any [`bismark_aligner::AlignerError`] ·
//! `2` clap parse error.

use std::process::ExitCode;

use clap::Parser;

use bismark_aligner::cli::Cli;
use bismark_aligner::{run, version_string};

// Multithreaded global allocator (Apple Silicon perf epic, 06222026). Relieves
// system-allocator arena-lock contention on the aligner's per-record String/Vec
// churn (bowtie2-output parse, conversion loop, methylation/tag path) — the same
// win the extractor + 3 other crates already take. Allocator-only: output is
// byte-identical (guarded by tests/byte_identity_real_data.rs + `just reproduce`).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

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
