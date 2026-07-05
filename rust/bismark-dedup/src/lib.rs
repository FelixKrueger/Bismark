//! `bismark-dedup` — Rust port of Bismark Perl's `deduplicate_bismark` script.
//!
//! This crate is the first downstream binary of the Bismark Rust rewrite, built
//! on top of [`bismark_io`] for all BAM/SAM/CRAM I/O. The binary is installed as
//! `deduplicate_bismark_rs` during the v0.26 → v1.0 coexistence period.
//!
//! See `~/.claude/plans/05242026_bismark-dedup-v1/PLAN.md` (rev 3) for the
//! design contract, behaviour specification, and phased implementation plan.
//!
//! ## Status
//!
//! **Phase C in progress.** Public API surface so far:
//!
//! - [`DedupKey`] — the value used to detect duplicates (SE = 3-tuple,
//!   PE = 4-tuple). Stable 16-byte `#[repr(C)]` layout.
//! - [`DedupState`] — accumulates the seen-set, duplicate-positions set,
//!   and running counters. [`DedupState::observe`] is the one-record
//!   entry point.
//! - [`DedupReport`] — byte-equal-to-Perl dedup report formatter.
//! - [`pipeline::run_single`] / [`pipeline::run_multiple`] — the
//!   end-to-end dedup pipelines for one input file or several inputs
//!   concatenated. Both wire [`bismark_io`] reader/writer to
//!   [`DedupState::observe`].
//! - [`filename`] — Perl-compatible output-stem derivation.
//! - [`BismarkDedupError`] — typed errors raised at the orchestration
//!   layer.
//!
//! CLI surface, integration tests on seeded-dup fixtures, and the 10M PE
//! WGBS byte-identity gate land in Phases D through G as separate
//! sub-issues with their own dual-review cycle.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod dedup;
pub mod error;
pub mod filename;
pub mod pipeline;
pub mod report;

pub use cli::UmiMode;
pub use dedup::{DedupKey, DedupState, UmiDedupKey, UmiDedupState};
pub use error::BismarkDedupError;
pub use report::DedupReport;

/// Returns a TG-style provenance string for the binary's `--version` output.
///
/// Format: `deduplicate_bismark_rs <semver> (<os>/<arch>)`.
///
/// Phase A keeps the provenance minimal. Phase G will extend this with git
/// commit hash and ISO-8601 build timestamp via a `build.rs` step.
#[must_use]
pub fn version_string() -> String {
    format!(
        "deduplicate_bismark_rs {} ({}/{})",
        bismark_meta::SUITE_VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}
