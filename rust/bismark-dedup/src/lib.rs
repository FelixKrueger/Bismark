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
//! **Phase B in progress.** Public API surface so far:
//!
//! - [`DedupKey`] — the value used to detect duplicates (SE = 3-tuple,
//!   PE = 4-tuple). Stable 16-byte `#[repr(C)]` layout.
//! - [`DedupState`] — accumulates the seen-set, duplicate-positions set,
//!   and running counters. [`DedupState::observe`] is the one-record
//!   entry point.
//! - [`DedupReport`] — byte-equal-to-Perl dedup report formatter.
//!
//! CLI surface, `bismark-io` wiring, integration tests, and the 10M PE
//! WGBS byte-identity gate land in Phases C through G as separate
//! sub-issues with their own dual-review cycle.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod dedup;
pub mod report;

pub use dedup::{DedupKey, DedupState};
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
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}
