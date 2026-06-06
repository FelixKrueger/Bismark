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
//! The binary is installed as `bismark2summary_rs`. The byte-identity target
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

/// Returns a TG-style provenance string for the binary's `--version` output.
///
/// Format: `bismark2summary_rs <semver> (<os>/<arch>)`. Help/version text is
/// not byte-gated against Perl (SPEC §4.4).
#[must_use]
pub fn version_string() -> String {
    format!(
        "bismark2summary_rs {} ({}/{})",
        bismark_meta::SUITE_VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}
