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
use crate::mbias_writer::{mbias_txt_path, write_mbias_txt};
use crate::output::{OutputFileMap, SplittingReport, write_splitting_report};

/// Aggregated mutable state threaded through `route_call`.
pub struct ExtractState {
    /// Resolved output mode. Read by `route_call` to drive yacht col-6/col-7
    /// computation and (via `OutputFileMap::mode`) to pick the right
    /// per-mode key for write dispatch.
    pub mode: OutputMode,
    /// `--mbias_off` — skip M-bias accumulation entirely.
    pub mbias_off: bool,
    /// `--mbias_only` — skip per-context split-file writes (Phase B always
    /// false; main dispatch rejects `--mbias_only` until Phase E).
    pub mbias_only: bool,
    /// `[R1/SE, R2]` M-bias tables. Phase B only ever increments index 0;
    /// Phase C starts populating index 1 for paired-end reads.
    pub mbias: [MbiasTable; 2],
    /// **Phase D**: `true` iff this run is paired-end. Set by the caller of
    /// `ExtractState::new` (`extract_se` passes `false`; `extract_pe` passes
    /// `true`). Decides whether `M-bias.txt` has 3 or 6 sections at
    /// finalize time. NOT inferable from `mbias[1].max_position() == 0`
    /// alone — an empty PE BAM would yield empty `mbias[1]` and get
    /// misclassified as SE.
    pub is_paired: bool,
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
    ///
    /// `is_paired` (Phase D): caller sets `false` from `extract_se`, `true`
    /// from `extract_pe`. Decides M-bias.txt section count (3 vs 6).
    pub fn new(
        config: &ResolvedConfig,
        input_path: &Path,
        input_basename: &str,
        is_paired: bool,
    ) -> Result<Self, BismarkExtractorError> {
        let fhs = OutputFileMap::new(
            &config.output_dir,
            input_basename,
            config.no_header,
            config.output_mode,
            config.gzip,
        )?;
        let splitting_report_path = config
            .output_dir
            .join(format!("{input_basename}_splitting_report.txt"));
        Ok(Self {
            mode: config.output_mode,
            mbias_off: config.mbias_off,
            // Phase E: derive `mbias_only` from the centralised predicate
            // on `ResolvedConfig` so the three derivation sites
            // (ExtractState, OutputFileMap, pipeline.rs) all read the same
            // source of truth.
            mbias_only: config.is_mbias_only(),
            mbias: [MbiasTable::default(), MbiasTable::default()],
            is_paired,
            fhs,
            report: SplittingReport::default(),
            input_path: input_path.to_path_buf(),
            splitting_report_path,
            emit_splitting_report: config.emit_splitting_report,
        })
    }

    /// Flush every split-file writer + emit the splitting report + emit
    /// M-bias.txt (unless `--mbias_off`).
    ///
    /// **Order** (Phase D rev 1, Reviewer B C1 fix):
    /// 1. `fhs.flush_all()` — buffered writes in the 12 split files
    /// 2. `write_splitting_report` — Perl `:2463` (inline in
    ///    `process_X_read_file`, BEFORE `produce_mbias_plots`)
    /// 3. `write_mbias_txt` (unless `mbias_off`) — Perl `:314` (after
    ///    `process_X_read_file` returns)
    ///
    /// Rev 0 of the Phase D plan had `M-bias.txt → splitting_report`; that
    /// inverted Perl's order and would have lost the splitting-report on
    /// a `write_mbias_txt` failure (e.g. disk-full). Real Perl writes the
    /// report first, so the partial-failure mode preserves diagnostic info.
    ///
    /// **Invariant**: `finalize` failure leaves the already-written split
    /// files in place on disk. The caller does NOT invoke
    /// `cleanup_partial_outputs` after a `finalize` failure — the records
    /// had already been routed successfully; failure here means the
    /// post-loop writes hit an I/O error after the data was on disk.
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
        if !config.mbias_off {
            let mbias_path = mbias_txt_path(&config.output_dir, &self.input_path);
            write_mbias_txt(&mbias_path, &self.mbias, self.is_paired)?;
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
