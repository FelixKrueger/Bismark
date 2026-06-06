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

/// Version provenance string for the `--version` flag.
///
/// Format: `filter_non_conversion_rs <semver> (<os>/<arch>)`.
#[must_use]
pub fn version_string() -> String {
    format!(
        "filter_non_conversion_rs {} ({}/{})",
        bismark_meta::SUITE_VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
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
