//! `bismark-report` — Rust port of the Perl `bismark2report`.
//!
//! Reads a Bismark **alignment** report (mandatory) plus up to four optional
//! companion reports — **deduplication**, methylation-extractor **splitting**,
//! **M-bias**, and **nucleotide coverage** — and fills a single self-contained
//! HTML template (`plotly_template.tpl`, with the ~3 MB plotly.js + two logos
//! inlined) to produce one graphical per-sample report.
//!
//! **Acceptance gate:** the generated HTML is **byte-identical** to Perl Bismark
//! v0.25.1, modulo the single `localtime` timestamp line (normalized in the gate
//! — see [`timestamp`]). No BAM I/O; does not depend on `bismark-io`.
//!
//! It is mechanically a parser + a string-substitution templating engine — there
//! is essentially **no numeric reformatting** (values inject verbatim; only a
//! `%`-strip, a `\s.*`-trim on the dedup counts, and one integer subtraction).

pub mod assets;
pub mod cli;
pub mod discovery;
pub mod error;
pub mod logging;
pub mod reports;
pub mod template;
pub mod timestamp;

use std::path::Path;

pub use error::ReportError;

/// The Bismark version this port belongs to (diagnostic banners only — never
/// injected into HTML bytes; the report's `{{bismark_version}}` is parsed from
/// the input alignment report).
pub const BISMARK_VERSION: &str = "v0.25.1";

/// `--version` banner (dedup/genomeprep precedent). Not part of the gate.
pub fn version_string() -> String {
    crate::meta::version_line("bismark2report")
}

/// Binary entry point — shared by this crate's own `main.rs` and the `bismark`
/// meta-crate's `bismark2report` bin (so `cargo install bismark` and `cargo
/// install bismark-report` behave identically). Parses the CLI, handles
/// `--version` (clap's auto-version is disabled) and `--man` (aliases `--help`,
/// exit 0), then runs. Exit: `0` ok · `1` [`ReportError`] (clap handles `2`
/// parse errors). The `#[global_allocator]`, if any, stays in each binary crate
/// root.
#[must_use]
pub fn run_main() -> std::process::ExitCode {
    use crate::report::cli::Cli;
    use clap::{CommandFactory, Parser};
    let cli = Cli::parse();

    // `--version` handled manually (clap auto-version disabled) so we print the
    // Bismark provenance banner.
    if cli.version {
        println!("{}", version_string());
        return std::process::ExitCode::SUCCESS;
    }
    // `--man` aliases `--help` (Perl behavior): print the long help.
    if cli.man {
        let _ = Cli::command().print_long_help();
        println!();
        return std::process::ExitCode::SUCCESS;
    }

    match run(&cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::from(1)
        }
    }
}

/// Top-level: resolve the output dir, the alignment report(s) and their
/// companions, then build + write one HTML per alignment report.
pub fn run(cli: &cli::Cli) -> Result<(), ReportError> {
    let log = logging::Logger::new(cli.verbose);

    // Output dir: trailing '/' appended unless empty (Perl 1093-1102).
    let output_dir = match &cli.dir {
        None => String::new(),
        Some(d) if d.is_empty() => String::new(),
        Some(d) if d.ends_with('/') => d.clone(),
        Some(d) => format!("{d}/"),
    };

    let alignments = discovery::find_alignment_reports(cli)?;

    // `-o`/`--output` is only legal with a single alignment report (Perl 1128).
    if alignments.len() > 1 && cli.output.is_some() {
        return Err(ReportError::Validation(
            "You cannot run bismark2report on more than 1 file while specifying a single output \
             file. Either lose the option -o to derive the output filenames automatically, or \
             specify a single Bismark alignment report file using the option '--alignment_report \
             FILE'"
                .into(),
        ));
    }

    let jobs = discovery::resolve_companions(cli, &alignments)?;

    for job in &jobs {
        // Perl chooses the name with truthiness (`if ($manual_output_file)`,
        // line 50) — so `-o ""` AND `-o 0` fall back to the derived name (both
        // are Perl-falsy). The `>1 report` guard above uses `is_some`, matching
        // Perl's `defined` at line 1129.
        let out_name = match cli.output.as_deref() {
            Some(o) if discovery::perl_truthy(o) => o.to_string(),
            _ => derive_output_name(&job.alignment),
        };
        let out_path = format!("{output_dir}{out_name}");
        log.note(&format!(
            "\nWriting Bismark HTML report to >> {out_path} <<\n"
        ));
        let doc = template::build_report(job, cli.test_timestamp, &log)?;
        std::fs::write(&out_path, &doc)?;
    }
    Ok(())
}

/// Derive the HTML filename from the alignment report (Perl 43-47): strip the
/// directory, strip a trailing `.txt`, append `.html`.
fn derive_output_name(aln: &Path) -> String {
    let base = aln
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = base.strip_suffix(".txt").unwrap_or(&base);
    format!("{stem}.html")
}
