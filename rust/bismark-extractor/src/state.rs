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
    /// **Phase 3a (inline-streaming epic)**: in-memory bedGraph aggregator.
    /// `Some` iff `config.bedgraph` (i.e. `--bedGraph` or `--cytosine_report`);
    /// `None` otherwise. During extraction, every kept call routed by
    /// [`OutputFileMap::write_call`] is tee'd into this aggregator via
    /// `add_ranked` (the tee), so finalize can write the `.bedGraph`/`.cov.gz`
    /// from `into_sorted()` instead of re-reading the (large) per-context files.
    /// The per-context files are still written (D2 additive).
    pub bedgraph_aggregator: Option<bismark_bedgraph::Aggregator>,
    /// **Phase 3a**: cached `config.cx_context`, since `config` is NOT in scope
    /// at the tee sites (`parallel.rs::write_routed_call` / `route.rs::route_call`
    /// take `&mut ExtractState`, not `&ResolvedConfig`). Drives the R4 tee
    /// selection gate: feed a call into the aggregator iff `cx` OR the
    /// destination basename starts with `"CpG"` (mirrors bedGraph's
    /// `select_input_files`).
    pub bedgraph_cx: bool,
    /// Per-context counters for the splitting report.
    pub report: SplittingReport,
    /// Path of the input BAM/SAM/CRAM — needed for splitting-report header.
    input_path: std::path::PathBuf,
    /// Where to write `_splitting_report.txt`.
    splitting_report_path: std::path::PathBuf,
    /// Whether to emit the splitting report at finalize time.
    emit_splitting_report: bool,
    // Phase G rev 2 (code-review A L1 fix): `input_basename: String` field
    // removed. It stored the `.bam`/`.sam`/`.cram`-stripped basename, which
    // the Phase G chain needs in its RAW form — using the stripped value
    // produced `…deduplicatedbedGraph` instead of `…deduplicated.bedGraph`.
    // Phase G now derives the raw filename from `self.input_path.file_name()`
    // via [`derive_raw_filename_for_phase_g`] at the chain-dispatch site.
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
            // Phase 3a: init the aggregator iff the downstream bedGraph step is
            // engaged. `config.bedgraph` is true iff --bedGraph or
            // --cytosine_report (c2c auto-triggers bedgraph at cli.rs:479).
            bedgraph_aggregator: if config.bedgraph {
                Some(bismark_bedgraph::Aggregator::new())
            } else {
                None
            },
            bedgraph_cx: config.cx_context,
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
        // Order (Phase D rev 1 + Phase C.2 #865):
        //   1. flush_all  — buffered writes hit disk
        //   2. finalize_with_empty_sweep — unlink empty per-strand files
        //      (matches Perl's `was empty -> deleted` sweep). MUST run
        //      AFTER flush so the records_written counter reflects all
        //      successful writes, and BEFORE write_splitting_report so
        //      the sweep's stderr log lines appear before any subsequent
        //      output.
        //   3. write_splitting_report
        //   4. write_mbias_txt (unless --mbias_off)
        let logger = crate::logging::Logger::from_config(config);
        // Empty-sample handling (plan 06142026_empty-sample-extractor-c2c):
        // a no-alignment sample has zero total methylation calls
        // (Z+z+X+x+H+h == SplittingReport.calls_total). On that condition,
        // when a downstream chain is expected (`--bedGraph`/`--cytosine_report`,
        // which methylseq always passes) we DELIBERATELY diverge from Perl:
        // force-create the empty per-context `.txt.gz` files (instead of
        // sweep-deleting them) and let the empty input flow through the
        // bedGraph/cov writer + c2c so methylseq's required output globs match.
        // A bare extraction with no `--bedGraph` keeps the Perl-faithful delete.
        let is_empty_run = self.report.calls_total == 0;
        self.fhs.flush_all()?;
        // Phase C.2 code-review B H2: gate the sweep on `!mbias_only` to
        // mirror Perl `:319 unless ($mbias_only) { delete_unused_files; }`.
        // In MbiasOnly the OutputFileMap is already empty (mode_keys()
        // returns Vec::new()) so the loop would no-op, but the sweep
        // emits two trailing `eprintln!()` blank lines unconditionally
        // — Perl emits nothing in this case. Guard at the call site.
        //
        // Phase G (rev 1 I10): finalize_with_empty_sweep now returns a
        // FinalizationReport. We retain it across the gap between sweep
        // and Phase G chain dispatch so the kept paths can be fed to
        // bismark2bedGraph as its positional argv tail. Under
        // `--mbias_only`, the OutputFileMap is already empty, so the
        // kept set is empty by construction — we still build an empty
        // FinalizationReport for uniformity.
        let finalization = if !self.mbias_only {
            // `force_create_empty` only fires when downstream outputs are
            // expected (a zero-call run with `--bedGraph`/`--cytosine_report`),
            // so a bare extraction keeps Perl's empty-file delete.
            let force_create_empty = is_empty_run && config.bedgraph;
            self.fhs
                .finalize_with_empty_sweep(logger, force_create_empty)?
        } else {
            crate::output::FinalizationReport::default()
        };
        if self.emit_splitting_report {
            write_splitting_report(
                &self.splitting_report_path,
                &self.input_path,
                config,
                self.is_paired,
                &self.report,
            )?;
        }
        // Console final methylation summary (#882) — mirror of the
        // splitting-report numbers, Perl `warn`'d at :2480-:2521. Emitted
        // regardless of `--report` (Perl always warns it); gated by --quiet.
        logger.final_summary(&self.report);
        if !config.mbias_off {
            let mbias_path = mbias_txt_path(&config.output_dir, &self.input_path);
            write_mbias_txt(&mbias_path, &self.mbias, self.is_paired)?;
        }

        // In-process downstream chain (inline-streaming epic Phase 2). Gated
        // on `config.bedgraph`, which is true iff the user set --bedGraph or
        // --cytosine_report (c2c auto-triggers bedgraph at `cli.rs:479`). Runs
        // AFTER the M-bias write so the user has already seen the M-bias
        // summary before any downstream progress lines stream. The chain
        // drives `bismark2bedGraph` + `coverage2cytosine` IN-PROCESS (no
        // fork/exec, no Perl) via each crate's `Cli`/`validate`/`run`.
        //
        // **Phase G rev 2 (code-review A L1 fix), preserved**: pass the RAW
        // input filename (un-stripped), NOT `self.input_basename` (which
        // `pipeline::derive_basename` already stripped of `.bam`/`.sam`/
        // `.cram`). `downstream_filenames::derive_bedgraph_filename` mirrors
        // Perl `:325-330` which only path-splits + strips literal `gz`/`sam`/
        // `bam`/`txt` — feeding it the already-stripped basename would produce
        // `…deduplicatedbedGraph` instead of `…deduplicated.bedGraph`, breaking
        // byte-identity on every real `.bam` input.
        if config.bedgraph {
            let raw_filename = derive_raw_filename_for_downstream(&self.input_path);
            // Phase 3a (F2): take the in-memory aggregator and materialize its
            // sorted chromosome data. `run_downstream_chain` writes the
            // `.bedGraph`/`.cov.gz` from THIS, instead of re-reading the
            // per-context call files (the I/O win). `take()` leaves `None`
            // behind (the aggregator is single-use). The aggregator is `Some`
            // here by construction (init'd in `new` iff `config.bedgraph`); the
            // `unwrap_or_default()` is a defensive empty-sorted fallback.
            let sorted = self
                .bedgraph_aggregator
                .take()
                .map(|a| a.into_sorted())
                .unwrap_or_default();
            crate::downstream_filenames::run_downstream_chain(
                config,
                &raw_filename,
                &config.output_dir,
                &finalization.kept,
                &sorted,
                is_empty_run,
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

/// Extract the RAW input filename (un-stripped) for the downstream chain's
/// filename derivation. Mirrors Perl `bismark_methylation_extractor:325` which
/// does `my $out = (split (/\//, $filename))[-1];` — i.e. path-split only, no
/// extension stripping. The subsequent `s/gz$//`, `s/sam$//`, `s/bam$//`,
/// `s/txt$//` pipeline lives in
/// [`crate::downstream_filenames::derive_bedgraph_filename`].
///
/// **Phase G rev 2 (code-review A L1 fix), preserved**: separated from
/// [`crate::pipeline::derive_basename`] because that function strips
/// `.bam`/`.sam`/`.cram` (used by the split-file naming + splitting-report
/// path) which would double-strip when fed to `derive_bedgraph_filename`.
///
/// Returns the file_name() component of `input_path`. Falls back to the
/// full lossy path string if `input_path` has no filename component
/// (defensive — CLI validation guarantees a real file).
fn derive_raw_filename_for_downstream(input_path: &std::path::Path) -> String {
    input_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| input_path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{OutputMode, PairedMode};
    use std::path::Path;

    /// Minimal `ResolvedConfig` for `ExtractState::new` tests. `output_dir`
    /// is the caller-supplied tempdir; `bedgraph` toggles the Phase 3a
    /// aggregator init.
    fn config_for_state(output_dir: &Path, bedgraph: bool, cx_context: bool) -> ResolvedConfig {
        ResolvedConfig {
            files: vec![std::path::PathBuf::from("sample.bam")],
            paired_mode: PairedMode::SingleEnd,
            output_mode: OutputMode::Default,
            ignore_5p_r1: 0,
            ignore_3p_r1: 0,
            ignore_5p_r2: 0,
            ignore_3p_r2: 0,
            no_overlap: false,
            output_dir: output_dir.to_path_buf(),
            no_header: false,
            gzip: false,
            emit_splitting_report: true,
            fasta_annotation: false,
            mbias_off: false,
            bedgraph,
            cytosine_report: false,
            cutoff: 1,
            remove_spaces: false,
            counts: true,
            zero_based: false,
            cx_context,
            split_by_chromosome: false,
            ucsc: false,
            buffer_size: None,
            gazillion: false,
            ample_memory: false,
            genome_folder: None,
            parallel: 1,
            quiet: false,
            verbose: false,
        }
    }

    /// Phase 3a T1: with `--bedGraph` (config.bedgraph == true), the state's
    /// aggregator field is `Some` so the tee has a sink during extraction.
    #[test]
    fn new_initializes_aggregator_when_bedgraph_enabled() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = config_for_state(
            tmp.path(),
            /*bedgraph=*/ true,
            /*cx_context=*/ false,
        );
        let state = ExtractState::new(&cfg, Path::new("sample.bam"), "sample", false)
            .expect("construct ExtractState with bedgraph");
        assert!(
            state.bedgraph_aggregator.is_some(),
            "aggregator must be Some when config.bedgraph is true"
        );
    }

    /// Phase 3a T1: without `--bedGraph`, the aggregator is `None` (no tee
    /// overhead on the common extract-only path).
    #[test]
    fn new_leaves_aggregator_none_when_bedgraph_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = config_for_state(
            tmp.path(),
            /*bedgraph=*/ false,
            /*cx_context=*/ false,
        );
        let state = ExtractState::new(&cfg, Path::new("sample.bam"), "sample", false)
            .expect("construct ExtractState without bedgraph");
        assert!(
            state.bedgraph_aggregator.is_none(),
            "aggregator must be None when config.bedgraph is false"
        );
    }

    /// Phase 3a T1: `bedgraph_cx` caches `config.cx_context` for the R4 tee
    /// gate (config is not in scope at the tee sites).
    #[test]
    fn new_caches_cx_context_for_r4_gate() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = config_for_state(
            tmp.path(),
            /*bedgraph=*/ true,
            /*cx_context=*/ true,
        );
        let state = ExtractState::new(&cfg, Path::new("sample.bam"), "sample", false)
            .expect("construct ExtractState with cx");
        assert!(
            state.bedgraph_cx,
            "bedgraph_cx must mirror config.cx_context"
        );
    }

    /// Phase G rev 2 (code-review A L1 regression guard), preserved: verify
    /// that the helper producing the downstream chain's input filename returns
    /// the RAW filename (un-stripped), so that
    /// `downstream_filenames::derive_bedgraph_filename` sees the full extension
    /// and produces a Perl-byte-identical bedGraph filename.
    #[test]
    fn derive_raw_filename_for_downstream_preserves_bam_extension() {
        assert_eq!(
            derive_raw_filename_for_downstream(Path::new("/tmp/foo.bam")),
            "foo.bam"
        );
    }

    #[test]
    fn derive_raw_filename_for_downstream_preserves_real_bismark_pe_filename() {
        // The byte-identity-critical case: chained extensions on real
        // Bismark output names.
        assert_eq!(
            derive_raw_filename_for_downstream(Path::new(
                "/path/to/sample.fastq_bismark_bt2_pe.deduplicated.bam"
            )),
            "sample.fastq_bismark_bt2_pe.deduplicated.bam"
        );
    }

    #[test]
    fn derive_raw_filename_for_downstream_preserves_cram_extension() {
        assert_eq!(
            derive_raw_filename_for_downstream(Path::new("/tmp/foo.cram")),
            "foo.cram"
        );
    }

    #[test]
    fn derive_raw_filename_for_downstream_preserves_chained_bam_gz_extension() {
        assert_eq!(
            derive_raw_filename_for_downstream(Path::new("/tmp/foo.bam.gz")),
            "foo.bam.gz"
        );
    }
}
