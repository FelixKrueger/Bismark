//! Typed errors for `bismark-summary`.
//!
//! All variants are produced at the orchestration / pipeline layer. The
//! binary's `main` maps any error to a nonzero exit code (the value is not
//! byte-gated; STDERR text is informational). Mirrors the `bismark-dedup`
//! `error.rs` shape.

use std::path::PathBuf;

/// All errors raised by the `bismark-summary` pipeline.
#[derive(Debug, thiserror::Error)]
pub enum BismarkSummaryError {
    /// No Bismark BAM files were supplied on the command line and none were
    /// auto-detected in the current directory. Mirrors Perl's `die` at
    /// `bismark2summary:200-202`.
    #[error(
        "No Bismark BAM files found to generate a Bismark project summary. Please respecify...\n\n\
         USAGE:\nbismark2summary (*.bam), or bismark2summary --help for more information"
    )]
    NoBamFiles,

    /// A mandatory Bismark alignment report could not be found for a
    /// discovered BAM. Mirrors Perl's `die` at `bismark2summary:284`.
    #[error("Could not find Bismark report ({0}) to open")]
    MissingAlignmentReport(PathBuf),

    /// A mix of data types (e.g. RRBS *and* WGBS) was detected in the same
    /// folder: in raw-alignment mode a deduplicated sample's blanked
    /// `aligned` count is the empty string. Mirrors Perl's `die` at
    /// `bismark2summary:1488-1490`.
    #[error(
        "It looks like there is a mix of samples, e.g. RRBS as well as WGBS, in this folder. \
         Please consider running bismark2summary only on samples of the same data type, or \
         specify the input files manually (--help for more information). Exiting..."
    )]
    MixedSampleTypes,

    /// Failed to inject the plot.ly library into the HTML template — the
    /// `{{plotly_goes_here}}` markers were not found. Mirrors Perl's `die`
    /// at `bismark2summary:1381-1383`. Defensive: the template is embedded,
    /// so this indicates a build-time template corruption.
    #[error(
        "Plot.ly injection not working, won't be able to construct any meaningful HTML reports \
         in this case...."
    )]
    PlotlyInjectionFailed,

    /// A plotted sample's alignment total (raw aligned — or deduplicated
    /// unique + duplicate — plus no-genomic + unaligned + ambiguous) was
    /// zero, so an alignment percentage cannot be computed. Perl dies here
    /// with "Illegal division by zero" (`bismark2summary:1506-1515`); this is
    /// degenerate / unreachable on real Bismark data (a plotted sample passed
    /// the methylation-context exclusion, so it has alignments). Reproduced as
    /// a typed error (raised during the HTML build, AFTER the `.txt` is
    /// written — matching Perl).
    #[error(
        "Illegal division by zero while computing alignment percentages \
         (a plotted sample has zero total reads)"
    )]
    ZeroAlignmentTotal,

    /// Direct `std::io::Error` from the orchestration layer (reading a
    /// report, writing an output file).
    #[error("I/O error{}: {source}", path_suffix(.path))]
    Io {
        /// The path being read/written when the error occurred, if known.
        path: Option<PathBuf>,
        /// The underlying I/O error.
        source: std::io::Error,
    },
}

impl From<std::io::Error> for BismarkSummaryError {
    fn from(source: std::io::Error) -> Self {
        BismarkSummaryError::Io { path: None, source }
    }
}

/// Render `" (<path>)"` for the [`BismarkSummaryError::Io`] Display, or the
/// empty string when no path is attached.
fn path_suffix(path: &Option<PathBuf>) -> String {
    match path {
        Some(p) => format!(" ({})", p.display()),
        None => String::new(),
    }
}
