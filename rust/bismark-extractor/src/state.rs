//! Aggregated extraction state — file handles, M-bias counters, splitting
//! report.
//!
//! Owned by the SE/PE pipeline; created at `extract_se` entry and dropped
//! at exit. Per SPEC §6.1 + §6.3: per-call helpers receive `&mut ExtractState`
//! rather than 14 positional args.

use std::path::Path;

use crate::cli::{OutputMode, ResolvedConfig};
use crate::error::BismarkExtractorError;
use crate::mbias::MbiasTable;
use crate::output::{OutputFileMap, SplittingReport, write_splitting_report};

/// Aggregated mutable state threaded through `route_call`.
pub struct ExtractState {
    /// Resolved output mode. Phase B always sees `Default` (main dispatch
    /// rejects others); Phase E reads this field to pick between
    /// `Comprehensive` / `MergeNonCpG` / `Yacht` / `MbiasOnly` routing.
    #[allow(dead_code)]
    pub mode: OutputMode,
    /// `--mbias_off` — skip M-bias accumulation entirely.
    pub mbias_off: bool,
    /// `--mbias_only` — skip per-context split-file writes (Phase B always
    /// false; main dispatch rejects `--mbias_only` until Phase E).
    pub mbias_only: bool,
    /// `[R1/SE, R2]` M-bias tables. Phase B only ever increments index 0;
    /// Phase C starts populating index 1 for paired-end reads.
    pub mbias: [MbiasTable; 2],
    /// Eagerly-opened per-(context, strand) split files.
    pub fhs: OutputFileMap,
    /// Per-context counters for the splitting report.
    pub report: SplittingReport,
    /// Path of the input BAM/SAM/CRAM — needed for splitting-report header.
    input_path: std::path::PathBuf,
    /// Where to write `_splitting_report.txt`.
    splitting_report_path: std::path::PathBuf,
    /// Whether to emit the splitting report at finalize time.
    emit_splitting_report: bool,
}

impl ExtractState {
    /// Construct state for one input file. Eagerly opens all 12 split files
    /// in `config.output_dir` (writing the version header line to each
    /// unless `config.no_header`).
    pub fn new(
        config: &ResolvedConfig,
        input_path: &Path,
        input_basename: &str,
    ) -> Result<Self, BismarkExtractorError> {
        let fhs = OutputFileMap::new(&config.output_dir, input_basename, config.no_header)?;
        let splitting_report_path = config
            .output_dir
            .join(format!("{input_basename}_splitting_report.txt"));
        Ok(Self {
            mode: config.output_mode,
            mbias_off: config.mbias_off,
            // Phase B never reaches `extract_se` with mbias_only set
            // (main.rs::run rejects), but ExtractState carries the field
            // for Phase E's route_call short-circuit pre-wiring.
            mbias_only: false,
            mbias: [MbiasTable::default(), MbiasTable::default()],
            fhs,
            report: SplittingReport::default(),
            input_path: input_path.to_path_buf(),
            splitting_report_path,
            emit_splitting_report: config.emit_splitting_report,
        })
    }

    /// Flush every split-file writer + emit the splitting report.
    ///
    /// **Invariant** (rev 1): `finalize` failure leaves the already-written
    /// split files in place on disk. The caller does NOT invoke
    /// `cleanup_partial_outputs` after a `finalize` failure — the records
    /// had already been routed successfully; failure here means the report
    /// write or final flush hit an I/O error after the data was on disk.
    /// Matches Perl's "die after writing" semantics.
    pub fn finalize(&mut self, config: &ResolvedConfig) -> Result<(), BismarkExtractorError> {
        self.fhs.flush_all()?;
        if self.emit_splitting_report {
            write_splitting_report(
                &self.splitting_report_path,
                &self.input_path,
                config,
                &self.report,
            )?;
        }
        Ok(())
    }

    /// Drop file handles + remove every partially-written split file. Called
    /// from `extract_se`'s pre-finalize error paths. Best-effort; one removal
    /// failure doesn't prevent the others.
    pub fn cleanup_partial_outputs(&mut self) {
        self.fhs.cleanup_all();
    }
}
