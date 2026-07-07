//! Typed errors for `bismark-report`.
//!
//! `main` maps any of these to exit code `1`; clap parse errors map to `2`
//! (clap convention). None of the error *text* is part of the byte-identity
//! gate — diagnostics go to STDERR only.

#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    /// Direct `std::io::Error` (reading a report, writing the HTML output).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A usage/validation failure that mirrors a Perl `die` (e.g. `-o` with more
    /// than one alignment report, an ambiguous companion glob, a no-report run,
    /// or a malformed nucleotide-coverage header). Text is STDERR only.
    #[error("{0}")]
    Validation(String),

    /// An asset-injection marker was not found (twice) in the template — this
    /// would mean the embedded `plotly_template.tpl` is corrupt. Mirrors Perl's
    /// `die "Plot.ly injection not working…"`.
    #[error("asset injection failed: marker `{0}` not found twice in the template")]
    AssetInjection(String),
}
