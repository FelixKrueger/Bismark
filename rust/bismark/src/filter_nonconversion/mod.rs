//! `bismark-filter-nonconversion` — Rust port of Bismark Perl's
//! `filter_non_conversion` script.
//!
//! Reads a Bismark BAM, walks each read's `XM:Z:` methylation-call string,
//! and removes reads (SE) / read-pairs (PE) with too much **non-CpG**
//! methylation (apparent incomplete bisulfite conversion). A verbatim
//! pass-through: records are written unchanged; only their routing (kept vs
//! removed) is decided.
//!
//! See `plans/05312026_bismark-filter-nonconversion/SPEC.md` (rev 1) for the
//! byte-identity contract and design.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod error;
pub mod filename;
pub mod filter;
pub mod pipeline;
pub mod report;

pub use cli::{Cli, ResolvedConfig};
pub use error::BismarkFilterError;
pub use filter::FilterMode;
pub use report::FilterReport;

use std::io::Write as _;
use std::time::Instant;

/// The uniform suite `--version` one-liner via [`crate::meta::version_line`]:
/// `filter_non_conversion (Bismark Rust suite) v<version> (<hash> — <os>/<arch> — built <ts>)`.
#[must_use]
pub fn version_string() -> String {
    crate::meta::version_line("filter_non_conversion")
}

/// Binary entry point — shared by this crate's own `main.rs` and the `bismark`
/// meta-crate's `filter_non_conversion` bin (so `cargo install bismark` and
/// `cargo install bismark-filter-nonconversion` behave identically). Parses the
/// CLI, handles `--version` (clap's auto-version is disabled in `cli.rs`),
/// enforces the no-files check (Perl's `@ARGV`-empty at :513, before option
/// validation), then validates and runs. Error prints carry no `error:` prefix
/// (faithful to Perl). Exit: `0` ok · `1` [`BismarkFilterError`] or no input
/// files (clap handles `2` parse errors). The `#[global_allocator]` stays in the
/// binary crate root.
#[must_use]
pub fn run_main() -> std::process::ExitCode {
    use clap::Parser;
    let cli = Cli::parse();

    // `--version` handled here (clap auto-version disabled in cli.rs).
    if cli.version {
        println!("{}", version_string());
        return std::process::ExitCode::SUCCESS;
    }

    // No-files check precedes option validation (Perl `@ARGV`-empty at line
    // 513, before the percentage/threshold checks) — so a bad option value
    // with no files yields the no-files message, not the option error.
    if cli.files.is_empty() {
        eprintln!(
            "Please provide one or more Bismark output files for non-bisulfite conversion filtering"
        );
        return std::process::ExitCode::from(1);
    }

    let config = match cli.validate() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return std::process::ExitCode::from(1);
        }
    };

    match run(&config) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::from(1)
        }
    }
}

/// Run the filter over every configured input file, each independently
/// (Perl's `foreach my $file (@ARGV)` loop — there is no `--multiple`).
///
/// The run-time line is appended to the **last** file's report only, matching
/// Perl (the `REPORT` filehandle is reused per file; only the last stays open
/// at script exit for line 664). If any file errors, the loop stops and the
/// error is returned (so the timing line is never appended — faithful to
/// Perl's fatal `die`).
///
/// # Errors
/// Returns [`BismarkFilterError`] on any I/O, format, or contract violation.
pub fn run(config: &ResolvedConfig) -> Result<(), BismarkFilterError> {
    let start = Instant::now();
    let n = config.files.len();

    for (i, infile) in config.files.iter().enumerate() {
        let report = pipeline::filter_one(infile, config.mode, config.explicit_mode)?;

        // STDERR summary (NOT part of the byte-identity gate — the report
        // FILE is). Perl warns a per-file SUMMARY at lines 311–353.
        eprint!(
            "NON-CONVERSION SUMMARY\n======================\n{}",
            report.format()
        );

        // Append the run-time line to the last file's report (Perl line 664).
        if i + 1 == n {
            let report_path = filename::report_name(&infile.to_string_lossy());
            let line = report::run_time_line(start.elapsed().as_secs());
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&report_path)?;
            f.write_all(line.as_bytes())?;
        }
    }

    // Perl's closing advisory (line 77) — STDERR, not gated.
    eprintln!("Please continue with deduplication or methylation extraction now");
    Ok(())
}
