//! `bismark-summary` — Rust port of Bismark Perl's `bismark2summary`.
//!
//! **Project-level, multi-sample aggregator** — distinct from the
//! per-sample `bismark2report`. It scans a run folder for Bismark BAMs (by
//! filename only — it never opens a BAM), locates each one's text report
//! files, parses per-sample metrics, and emits one project summary:
//!
//! - `<basename>.txt` — a 15-column tab-delimited table, one row per sample.
//! - `<basename>.html` — a self-contained plot.ly report (Phase B).
//!
//! The binary is installed as `bismark2summary`. The byte-identity target
//! is Perl Bismark v0.25.1: the `.txt` fully byte-identical, the `.html`
//! byte-identical modulo the single `localtime` timestamp line.
//!
//! See `plans/06012026_bismark2summary/SPEC.md` (rev 1) for the contract.
//!
//! ## Status
//!
//! **Phase A** (CLI + BAM discovery + report-name derivation + the three
//! report parsers + the `.txt` table) and **Phase B** (the `.html`: embedded
//! plot.ly/logo assets, the inline template, the fill engine, the `%.2f` /
//! `%.15g` percentage maths) are implemented.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod assets;
pub mod cli;
pub mod discovery;
pub mod error;
pub mod fmt_g;
pub mod html;
pub mod parse;
pub mod plot;
pub mod timestamp;
pub mod txt;

pub use cli::{Cli, ResolvedConfig};
pub use error::BismarkSummaryError;
pub use parse::SampleMetrics;

/// The Bismark version string baked into the HTML `{{bismark_version}}`
/// footer and the `--version` banner. Matches the Perl `$bismark_version`
/// constant (`bismark2summary:25`) so the HTML is byte-identical (SPEC O1).
pub const BISMARK_VERSION: &str = "0.25.1";

/// The uniform suite `--version` one-liner via [`crate::meta::version_line`]:
/// `bismark2summary (Bismark Rust suite) v<version> (…)`. Help/version text is
/// not byte-gated against Perl (SPEC §4.4).
#[must_use]
pub fn version_string() -> String {
    crate::meta::version_line("bismark2summary")
}

/// Binary entry point — shared by this crate's own `main.rs` and the `bismark`
/// meta-crate's `bismark2summary` bin (so `cargo install bismark` and `cargo
/// install bismark-summary` behave identically). Parses the CLI, handles
/// `--version` (clap's auto-version is disabled) and `--man` (aliases `--help`,
/// exit 0), then drives discovery + report parsing + output writing via [`run`].
/// Error prints carry no `error:` prefix (faithful to Perl). Exit: `0` ok · `1`
/// [`BismarkSummaryError`] (clap handles `2` parse errors). The
/// `#[global_allocator]`, if any, stays in each binary crate root.
#[must_use]
pub fn run_main() -> std::process::ExitCode {
    run_from_args(std::env::args_os())
}

/// Same as [`run_main`] but parses from an explicit argv — used by the multicall
/// `bismark <subcommand>` dispatcher (argv reconstructed with the subcommand token
/// stripped and `argv[0]` pinned to `bismark <sub>`).
pub fn run_from_args<I>(argv: I) -> std::process::ExitCode
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString> + Clone,
{
    use clap::{CommandFactory, Parser};
    let cli = Cli::parse_from(argv);

    // `--version` / `--man` handled here (clap auto-version is disabled so we
    // can emit the custom provenance string; `--man` aliases `--help`).
    if cli.version {
        println!("{}", version_string());
        return std::process::ExitCode::SUCCESS;
    }
    if cli.man {
        // Best-effort; help text is not byte-gated.
        let _ = Cli::command().print_long_help();
        println!();
        return std::process::ExitCode::SUCCESS;
    }

    match run(cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::from(1)
        }
    }
}

/// The `bismark2summary` driver (Perl top-level flow). Discovers Bismark BAMs
/// (or takes them from argv), parses each sample's report set, and writes the
/// project `.txt` then `.html` outputs. All paths resolve against the current
/// working directory, matching Perl.
fn run(cli: Cli) -> Result<(), BismarkSummaryError> {
    use std::path::{Path, PathBuf};

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
