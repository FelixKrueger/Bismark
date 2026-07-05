//! Binary entry point for `bismark2summary_rs`.
//!
//! Parses the CLI, discovers Bismark BAMs (or takes them from argv), parses
//! each sample's report set, and writes the project summary outputs.
//!
//! Exit codes:
//! - `0` — success (also `--version` / `--help` / `--man`)
//! - `1` — any [`BismarkSummaryError`]
//! - `2` — clap parse error (clap convention for usage errors)

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, Parser};

use bismark_summary::cli::Cli;
use bismark_summary::error::BismarkSummaryError;
use bismark_summary::{discovery, html, parse, timestamp, txt, version_string};

fn main() -> ExitCode {
    let cli = Cli::parse();

    // `--version` / `--man` handled here (clap auto-version is disabled so we
    // can emit the custom provenance string; `--man` aliases `--help`).
    if cli.version {
        println!("{}", version_string());
        return ExitCode::SUCCESS;
    }
    if cli.man {
        // Best-effort; help text is not byte-gated.
        let _ = Cli::command().print_long_help();
        println!();
        return ExitCode::SUCCESS;
    }

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<(), BismarkSummaryError> {
    let config = cli.validate();

    // Discovery + report-name derivation + report reads all resolve against
    // the current working directory, matching Perl (which globs and opens
    // reports relative to its launch dir).
    let base_dir = Path::new(".");
    let bams = discovery::discover_bams(&config.bam_files, base_dir)?;

    eprintln!(
        "Generating Bismark summary report from {} Bismark BAM file(s)...",
        bams.len()
    );

    let mut samples = Vec::with_capacity(bams.len());
    for bam in &bams {
        // Alignment report is mandatory: collect_sample errors (→ exit 1)
        // if it is missing, before any output file is written (Perl `die`
        // at :284, mid-loop, so the .txt is never produced — matched here).
        samples.push(parse::collect_sample(base_dir, bam)?);
    }

    // ─── .txt output ─────────────────────────────────────────────────────
    // Written FIRST (Perl `:478-481`, before the HTML at `:1716`). So if the
    // HTML build hits the mixed-sample-types `die` (`:1488`), the `.txt` is
    // still on disk — matching Perl.
    let txt_content = txt::build_txt(&samples);
    let txt_path = PathBuf::from(format!("{}.txt", config.report_basename));
    std::fs::write(&txt_path, txt_content.as_bytes()).map_err(|e| BismarkSummaryError::Io {
        path: Some(txt_path.clone()),
        source: e,
    })?;

    // ─── .html output ────────────────────────────────────────────────────
    let stamp = match config.test_timestamp {
        Some(epoch) => timestamp::format_ctime_utc(epoch),
        None => timestamp::now_ctime_utc(),
    };
    let html_content = html::build_html(&samples, &config.page_title, &stamp)?;
    let html_path = PathBuf::from(format!("{}.html", config.report_basename));
    std::fs::write(&html_path, html_content.as_bytes()).map_err(|e| BismarkSummaryError::Io {
        path: Some(html_path.clone()),
        source: e,
    })?;

    eprintln!(
        "Wrote Bismark project summary to >> {} <<",
        html_path.display()
    );

    Ok(())
}
