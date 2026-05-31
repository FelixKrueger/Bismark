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

// Multithreaded global allocator. The parallel per-file parse (`parallel.rs`)
// is hashmap-insert-bound — i.e. allocation-heavy (per-thread map growth).
// Under the default system allocator, concurrent maps block on shared arena
// locks, so `--parallel N>1` ran SLOWER than N=1 on a full `--CX` gate (system
// allocator: p1 973s, p3 1790s, p6 1508s — anti-scaling). mimalloc's
// per-thread heaps remove that contention — the same fix that eliminated the
// extractor's parallel anti-scaling (#884, `8a2a147`). Allocator choice does
// not affect computed output; decompressed-content byte-identity (SPEC D1) is
// unchanged (guarded by the N-invariance tests + the real-data gate).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

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
