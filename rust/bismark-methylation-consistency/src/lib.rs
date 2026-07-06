//! `bismark-methylation-consistency` — Rust port of Bismark Perl's
//! `methylation_consistency` script.
//!
//! Splits a Bismark alignment BAM into three BAMs by the **read-level**
//! consistency of its CpG (or, with `--chh`, CHH) methylation calls —
//! consistently methylated (`>= upper_threshold`), consistently unmethylated
//! (`<= lower_threshold`), and mixed — plus a `_consistency_report.txt`. Built
//! on [`bismark_io`] for all BAM I/O (pure Rust, no `samtools` subprocess);
//! the binary installs as `methylation_consistency`.
//!
//! Acceptance contract: byte-identical to the Perl original for the report and
//! (at the decompressed-record level) the three BAMs. See
//! `plans/05292026_bismark-methylation-consistency/{SPEC,PLAN}.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod classify;
pub mod cli;
pub mod error;
pub mod filename;
pub mod logging;
pub mod pipeline;
pub mod report;

pub use classify::{Bucket, Counts, Routing};
pub use cli::{Cli, LibraryMode, ResolvedConfig};
pub use error::MethConsError;
pub use report::Tally;

/// Provenance string for the binary's `--version` output.
///
/// The uniform suite one-liner via [`bismark_meta::version_line`]:
/// `methylation_consistency (Bismark Rust suite) v<version> (…)` — the SUITE
/// version, **not** the Bismark `0.25.1` constant (methcons injects no header).
#[must_use]
pub fn version_string() -> String {
    bismark_meta::version_line("methylation_consistency")
}
