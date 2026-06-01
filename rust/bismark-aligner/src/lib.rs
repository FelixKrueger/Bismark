//! `bismark-aligner` — Rust port of the Perl `bismark` aligner **wrapper**.
//!
//! `bismark` is not an aligner: it converts reads (C→T, plus the G→A complement
//! for non-directional), drives 2–4 external **Bowtie 2** instances against the
//! bisulfite-converted indexes, merges/scores their SAM in read-ID lockstep,
//! performs the bisulfite best-alignment selection + strand assignment + the
//! `XM`/`XR`/`XG` methylation call, and writes the Bismark BAM + reports.
//!
//! **Acceptance gate:** byte-identical *decompressed* SAM content (`samtools
//! view` + `-H`) vs Perl Bismark v0.25.1 driving the pinned Bowtie 2 2.5.5
//! (Phase-0 spike confirmed the premise; raw BGZF bytes are NOT gated since the
//! Rust path writes via noodles, not samtools).
//!
//! **This crate is built phase by phase** (see `plans/05312026_bismark-aligner/`).
//! Phase 1 (here): CLI + option parsing + genome/index discovery + aligner
//! detection + `aligner_options` assembly → a [`config::RunConfig`]; **no
//! alignment is performed yet**.

pub mod aligner;
pub mod cli;
pub mod config;
pub mod discovery;
pub mod error;
pub mod options;

pub use config::{RunConfig, resolve};
pub use error::{AlignerError, Result};

/// The Bismark version this port reproduces in `@PG`/reports/banners.
pub const BISMARK_VERSION: &str = "v0.25.1";

/// `--version` banner (uses the crate's own `CARGO_PKG_VERSION`; not byte-gated).
pub fn version_string() -> String {
    format!(
        "\n          Bismark - Bisulfite Mapper and Methylation Caller.\n\n          \
         Bismark Aligner (Rust port) Version: {}\n        \
         Copyright 2010-25, Felix Krueger, Altos Bioinformatics\n\n               \
         https://github.com/FelixKrueger/Bismark\n",
        env!("CARGO_PKG_VERSION")
    )
}

/// Phase-1 entry: resolve the configuration and print a summary. No alignment.
///
/// `command_line` is the verbatim argv (program name excluded), captured before
/// parsing, for the eventual `@PG` `CL:` line.
pub fn run(cli: &cli::Cli, command_line: String) -> Result<()> {
    let config = resolve(cli, command_line)?;
    let deferred = config::deferred_flags(cli);
    if !deferred.is_empty() {
        eprintln!(
            "Note: these options are recognised but not yet active in this build \
             (wired in a later phase): {}",
            deferred.join(", ")
        );
    }
    eprintln!("{}", config.summary());
    Ok(())
}
