//! `bismark-bedgraph` — Rust port of Bismark Perl's `bismark2bedGraph`.
//!
//! Consumes the methylation extractor's per-context call files
//! (`CpG_OT_*`, `CpG_OB_*`, … and the CHG/CHH equivalents with `--CX`) and
//! emits a sorted, gzip-compressed bedGraph + coverage file. Installed as
//! `bismark2bedGraph` during the v0.26 → v1.0 coexistence period.
//!
//! See `SPEC.md` (rev 1) for the binding contract — decompressed-content
//! byte-identity to Perl v0.25.1, the argv-order chromosome ownership rule,
//! the `%.15g` percentage formatting, and the in-memory-only scope.
//!
//! ## Pipeline
//!
//! [`run`] orchestrates: select input files ([`input::select_input_files`],
//! argv order preserved) → parse + validate + aggregate into an
//! [`Aggregator`] → write outputs ([`output::write_outputs`]) → optional
//! `--ucsc` post-pass ([`ucsc::write_ucsc`]).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod aggregate;
pub mod cli;
pub mod error;
pub mod filename;
pub mod fmt_g;
pub mod input;
pub mod output;
pub mod ucsc;
pub mod validate;

pub use aggregate::Aggregator;
pub use cli::{Cli, ResolvedConfig};
pub use error::BismarkBedgraphError;

use std::path::Path;

/// One-line `--version` string for the binary (suite-wide shape via
/// [`bismark_meta::version_line`]):
/// `bismark2bedGraph (Bismark Rust suite) v<semver> (<hash> — <os>/<arch> — built <ts>)`.
#[must_use]
pub fn version_string() -> String {
    bismark_meta::version_line("bismark2bedGraph")
}

/// Binary entry point — shared by this crate's own `main.rs` and the `bismark`
/// meta-crate's `bismark2bedGraph` bin (so `cargo install bismark` and
/// `cargo install bismark-bedgraph` behave identically). Parses the CLI,
/// handles `--version` / `--man` short-circuits, validates, and runs. Exit:
/// `0` ok · `1` error (clap handles `2` parse errors before this).
#[must_use]
pub fn run_main() -> std::process::ExitCode {
    use clap::{CommandFactory, Parser};
    let cli = Cli::parse();

    // `--version` is handled here (clap's auto-version is disabled in
    // src/cli.rs so we can emit our custom provenance string).
    if cli.version {
        println!("{}", version_string());
        return std::process::ExitCode::SUCCESS;
    }

    // `--man` is Perl's alias for the long help text.
    if cli.man {
        let mut cmd = Cli::command();
        // print_long_help writes to stdout; ignore the unlikely write error.
        let _ = cmd.print_long_help();
        println!();
        return std::process::ExitCode::SUCCESS;
    }

    match cli.validate().and_then(|config| run(&config)) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::from(1)
        }
    }
}

/// Run the full conversion for a validated [`ResolvedConfig`].
///
/// Steps:
/// 1. Create the output directory if needed.
/// 2. Select the input files (CpG-only unless `--CX`), preserving argv order.
/// 3. Read every call into the [`Aggregator`], attributing chromosome
///    ownership to each file's basename.
/// 4. Write the bedGraph + coverage (+ optional zero-based) outputs.
/// 5. If `--ucsc`, write the UCSC-compatible bedGraph.
///
/// Memory note (SPEC §1.1 D3): all covered positions are held in RAM. This
/// is sub-GB for CpG-context human/mouse runs but can reach tens of GB for
/// a full `--CX` WGBS run; external-spill is a future capability (SPEC §9).
pub fn run(cfg: &ResolvedConfig) -> Result<(), BismarkBedgraphError> {
    if !cfg.output_dir.as_os_str().is_empty() && cfg.output_dir != Path::new(".") {
        std::fs::create_dir_all(&cfg.output_dir)?;
    }

    let inputs = input::select_input_files(&cfg.files, cfg.cx)?;

    let mut agg = Aggregator::new();
    for path in &inputs {
        let source_basename = input::basename(path);
        input::read_into(path, cfg.no_header, &source_basename, &mut agg)?;
    }

    output::write_outputs(cfg, agg)?;

    if cfg.ucsc {
        ucsc::write_ucsc(cfg)?;
    }
    Ok(())
}
