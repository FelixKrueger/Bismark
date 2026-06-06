//! Command-line interface for `bismark_methylation_extractor_rs`.
//!
//! All 35 Perl flags from [SPEC §3](../../SPEC.md) are mapped to clap-derived
//! fields below. [`Cli::validate`] returns a [`ResolvedConfig`] after
//! checking every documented mutex pair + precondition from SPEC §11 +
//! Perl source.
//!
//! ## Flag-to-Perl mapping
//!
//! Each `#[arg]` annotation has a comment citing the Perl source line
//! (`bismark_methylation_extractor:NNN`) and SPEC §3 row number. The
//! defaults match Perl exactly. Mutex pairs use clap's `conflicts_with`
//! where possible (Perl-runtime die mirrored at parse time when feasible;
//! die-only-at-runtime cases handled in [`Cli::validate`]).

use std::path::PathBuf;

use clap::Parser;

use crate::error::BismarkExtractorError;

/// Parsed command-line arguments. Use [`Cli::validate`] to convert to a
/// [`ResolvedConfig`] after parsing.
#[derive(Parser, Debug)]
#[command(
    name = "bismark_methylation_extractor_rs",
    about = "Extract methylation calls from Bismark-aligned BAM/SAM/CRAM files",
    long_about = None,
    disable_version_flag = true
)]
pub struct Cli {
    /// Bismark BAM/SAM/CRAM file(s) to extract methylation calls from.
    pub files: Vec<PathBuf>,

    // ─── Library mode (SPEC §3 rows 2-3, Perl 960-961) ───
    /// Force single-end mode (auto-detected from `@PG` if neither -s nor -p
    /// is set). Mutex with --paired-end.
    #[arg(short = 's', long = "single-end", conflicts_with = "paired_end")]
    pub single_end: bool,

    /// Force paired-end mode (auto-detected from `@PG` if neither -s nor -p
    /// is set). Mutex with --single-end.
    #[arg(short = 'p', long = "paired-end")]
    pub paired_end: bool,

    // ─── Splitting-report annotations (SPEC §3 row 4, Perl 962) ───
    /// Legacy. Perl line 5040 writes a `_splitting_report.txt` annotation
    /// line ("Genomic equivalent sequences will be printed out in FastA
    /// format") when set; no actual FASTA output is produced. Rust port
    /// mirrors the splitting-report behaviour.
    #[arg(long = "fasta")]
    pub fasta: bool,

    // ─── Read-region trimming (SPEC §3 rows 5-8, Perl 963-964 + 989-990) ───
    /// Trim N bp from the 5' end of R1 (or SE) before extraction.
    #[arg(long = "ignore", default_value_t = 0u32)]
    pub ignore: u32,

    /// Trim N bp from the 5' end of R2 (PE-only).
    #[arg(long = "ignore_r2", default_value_t = 0u32)]
    pub ignore_r2: u32,

    /// Trim N bp from the 3' end of R1 (or SE).
    #[arg(long = "ignore_3prime", default_value_t = 0u32)]
    pub ignore_3prime: u32,

    /// Trim N bp from the 3' end of R2 (PE-only).
    #[arg(long = "ignore_3prime_r2", default_value_t = 0u32)]
    pub ignore_3prime_r2: u32,

    // ─── Output mode flags (SPEC §3 rows 9, 14, 34, Perl 965, 969, 992) ───
    /// Merge the 4 strand-specific files per context into 1.
    /// Output count: 3 (or 2 with --merge_non_CpG).
    #[arg(long = "comprehensive")]
    pub comprehensive: bool,

    /// Collapse CHG + CHH into one "non-CpG" output.
    /// Output count: 8 (or 2 with --comprehensive). Mutex with --yacht.
    #[arg(long = "merge_non_CpG")]
    pub merge_non_cpg: bool,

    /// SE-only NOMe-Seq mode: emit a single `any_C_context_*.txt[.gz]` with
    /// read metadata (start, end, orientation). Forces --comprehensive +
    /// --merge_non_CpG; mutex with --paired-end and --mbias_only.
    #[arg(long = "yacht")]
    pub yacht: bool,

    // ─── Output controls (SPEC §3 rows 10, 15, 16, 28, Perl 966, 970, 971, 983) ───
    /// Emit `_splitting_report.txt` (ON by default in Perl; --no-report disables).
    /// Note: the Perl flag is `--report` and is enabled-by-default; we
    /// follow the same default but expose it for compat.
    #[arg(long = "report", default_value_t = true)]
    pub report: bool,

    /// Output directory (created if missing). Default: current working
    /// directory.
    #[arg(short = 'o', long = "output_dir", default_value = ".")]
    pub output_dir: PathBuf,

    /// Suppress the Bismark-version header in all output files.
    #[arg(long = "no_header")]
    pub no_header: bool,

    /// Gzip-compress all split files (`.gz` suffix on output filenames).
    #[arg(long = "gzip")]
    pub gzip: bool,

    // ─── Paired-end overlap handling (SPEC §3 rows 12-13, Perl 968, 988) ───
    /// Drop R2 calls overlapping R1's reference span. PE-only. ON by
    /// default for paired-end inputs (Perl: --no_overlap is the default).
    /// Use --include_overlap to override.
    #[arg(long = "no_overlap")]
    pub no_overlap: bool,

    /// Keep R2 calls in the overlap region (override default --no_overlap).
    /// PE-only.
    #[arg(long = "include_overlap")]
    pub include_overlap: bool,

    // ─── BedGraph subprocess chain (SPEC §3 rows 17-19, 23, 35, Perl 972-974, 978, 993) ───
    /// Post-process methylation calls into sorted bedGraph + coverage
    /// outputs. Triggers a subprocess call to `bismark2bedGraph` (Phase G);
    /// auto-triggered when --cytosine_report is set.
    #[arg(long = "bedGraph")]
    pub bedgraph: bool,

    /// Minimum read coverage threshold for bedGraph emission (--bedGraph
    /// only; Phase G).
    #[arg(long = "cutoff", default_value_t = 1u32)]
    pub cutoff: u32,

    /// Replace whitespace in read IDs with underscores before sorting
    /// (--bedGraph only; passes through to the bismark2bedGraph subprocess).
    #[arg(long = "remove_spaces")]
    pub remove_spaces: bool,

    /// Counts per position in coverage output (always ON in Perl; exposed
    /// for compat).
    #[arg(long = "counts", default_value_t = true)]
    pub counts: bool,

    /// Emit 0-based half-open coordinates in bedGraph + cytosine_report
    /// (default is 1-based closed). --bedGraph/--cytosine_report only.
    #[arg(long = "zero_based")]
    pub zero_based: bool,

    /// UCSC-compatible bedGraph: prefix `chr`, rename `MT` → `chrM`, emit
    /// `chromosome_sizes.txt`. --bedGraph-only.
    #[arg(long = "ucsc")]
    pub ucsc: bool,

    /// Sort-buffer size for the UNIX-sort step in `bismark2bedGraph`.
    /// Mutex with --ample_memory when **explicitly set** (Perl line 1295
    /// checks `unless($sort_size)`, so the implicit default "2G" doesn't
    /// trip the mutex). Optional here to preserve explicit-vs-default
    /// distinction.
    #[arg(long = "buffer_size")]
    pub buffer_size: Option<String>,

    /// Disable per-chromosome pre-split (filehandle-limit workaround for
    /// genomes with thousands of contigs). Forces single-file sort path
    /// via `bismark2bedGraph`. Mutex with --ample_memory.
    #[arg(long = "gazillion", visible_alias = "scaffolds")]
    pub gazillion: bool,

    /// Use in-memory arrays for sorting (faster but ~16 GB RSS for human
    /// chr1). Mutex with --gazillion and --buffer_size.
    #[arg(long = "ample_memory")]
    pub ample_memory: bool,

    // ─── Cytosine-report subprocess chain (SPEC §3 rows 21-22, 24-25, Perl 976-980) ───
    /// Post-process methylation calls into a genome-wide cytosine report
    /// via a subprocess call to `coverage2cytosine` (Phase G). Auto-triggers
    /// --bedGraph. Requires --genome_folder.
    #[arg(long = "cytosine_report")]
    pub cytosine_report: bool,

    /// Path to a Bismark-prepared genome folder (FASTA + `.fai` indexes).
    /// Required when --cytosine_report is set. NO default in the Rust port
    /// (Perl's hardcoded mouse default is rejected; see SPEC §11).
    #[arg(short = 'g', long = "genome_folder")]
    pub genome_folder: Option<PathBuf>,

    /// Report all C-contexts (not just CpG) in cytosine_report.
    /// --cytosine_report only; significant runtime increase.
    #[arg(long = "CX", visible_alias = "CX_context")]
    pub cx_context: bool,

    /// Per-chromosome output of cytosine_report. --cytosine_report only.
    #[arg(long = "split_by_chromosome")]
    pub split_by_chromosome: bool,

    // ─── M-bias toggles (SPEC §3 rows 29-30, Perl 984-985) ───
    /// Skip all per-context split-file output; emit M-bias only.
    /// Mutex with --bedGraph, --cytosine_report, --mbias_off, --yacht.
    #[arg(long = "mbias_only")]
    pub mbias_only: bool,

    /// Skip M-bias computation (still emit per-context split files +
    /// _splitting_report.txt). Mutex with --mbias_only.
    #[arg(long = "mbias_off")]
    pub mbias_off: bool,

    // ─── Console diagnostics (#882) ───
    /// Suppress the informational stderr log (banner, mode, parameter summary,
    /// header provenance, progress counter, final summary, kept/deleted).
    /// Genuine warnings + errors still print. Default off (verbose like Perl).
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,

    /// Also print the `@SQ` reference dictionary in the header provenance
    /// (long-only; `@HD`/`@PG` are shown regardless). Off by default — the
    /// per-contig dump is noise on large genomes.
    #[arg(long = "verbose")]
    pub verbose: bool,

    // ─── Compat / silent-accept flags (SPEC §3 row 27) ───
    /// Path to a samtools binary. Silently accepted in the Rust port —
    /// bismark-io uses pure-Rust noodles; no samtools subprocess is
    /// spawned. Matches the bismark-dedup precedent at `cli.rs:228-230`.
    #[arg(long = "samtools_path")]
    pub samtools_path: Option<PathBuf>,

    // ─── Parallelism (SPEC §3 row 33, Perl 991) ───
    /// Number of extraction worker threads (default 1; floored at 2 for BAM).
    ///
    /// Replaces Perl's fork+modulo `--multicore N`. Sets ONLY the worker count —
    /// BGZF decode (fixed 2 threads) and gzip output (a compression pool) are
    /// always-on and independent of `--parallel`, so even `--parallel 1` uses
    /// ~7-8 CPU cores in gzip mode (by design, not a runaway). The default is
    /// already throughput-optimal; raising `--parallel` does NOT speed up BAM
    /// extraction (decode-bound). `0` is rejected at validate-time. See the
    /// README "Resource usage" section.
    #[arg(long = "parallel", visible_alias = "multicore", default_value_t = 1u32)]
    pub parallel: u32,

    // ─── Version (SPEC §3 row 11, Perl 967) ───
    /// Print TG-style provenance string and exit.
    #[arg(short = 'V', long = "version")]
    pub version: bool,
}

/// Library-mode resolution (after `--single`/`--paired`/auto-detect).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairedMode {
    /// Force single-end.
    SingleEnd,
    /// Force paired-end.
    PairedEnd,
    /// Auto-detect from `@PG ID:Bismark` line at pipeline-open time.
    AutoDetect,
}

/// Output-mode resolution from the combination of `--comprehensive`,
/// `--merge_non_CpG`, `--yacht`, `--mbias_only`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Default — 12 strand-specific split files (CpG/CHG/CHH × OT/CTOT/CTOB/OB)
    /// for PE; 6 (only OT + OB populated) for SE-directional.
    Default,
    /// `--comprehensive` — 3 files (one per context, merged strands).
    Comprehensive,
    /// `--merge_non_CpG` — 8 files (CpG ×4 strands + Non_CpG ×4 strands).
    MergeNonCpG,
    /// `--comprehensive --merge_non_CpG` — 2 files (CpG + Non_CpG).
    ComprehensiveMergeNonCpG,
    /// `--yacht` — 1 file (`any_C_context_*`) with read-metadata columns.
    /// SE-only NOMe-Seq mode. Forces comprehensive + merge_non_CpG semantically.
    Yacht,
    /// `--mbias_only` — 0 split files; only `M-bias.txt` (+ optional
    /// `_splitting_report.txt`).
    MbiasOnly,
}

/// The validated, resolved subset of CLI arguments passed to the
/// extraction pipeline. Constructed by [`Cli::validate`].
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// Positional input file paths.
    pub files: Vec<PathBuf>,
    /// Resolved library mode (explicit `-s`/`-p`, else auto-detect).
    pub paired_mode: PairedMode,
    /// Resolved output mode (combination of mode flags).
    pub output_mode: OutputMode,
    /// Read-region trims.
    pub ignore_5p_r1: u32,
    /// (see field above)
    pub ignore_3p_r1: u32,
    /// (see field above)
    pub ignore_5p_r2: u32,
    /// (see field above)
    pub ignore_3p_r2: u32,
    /// Drop R2 calls overlapping R1's reference span (PE-only). Resolved
    /// to `true` for PE by default; user can override with `--include_overlap`.
    pub no_overlap: bool,
    /// Output directory.
    pub output_dir: PathBuf,
    /// Suppress version header in output files.
    pub no_header: bool,
    /// Gzip-compress all split files.
    pub gzip: bool,
    /// Emit `_splitting_report.txt` (default true).
    pub emit_splitting_report: bool,
    /// `--fasta` annotation (splitting-report only; no FASTA output).
    pub fasta_annotation: bool,
    /// Skip M-bias computation.
    pub mbias_off: bool,
    /// `--bedGraph` chain via subprocess (Phase G).
    pub bedgraph: bool,
    /// `--cytosine_report` chain via subprocess (Phase G). Auto-implies bedgraph.
    pub cytosine_report: bool,
    /// Coverage cutoff for bedgraph (--bedGraph only).
    pub cutoff: u32,
    /// Replace whitespace in read IDs (--bedGraph only).
    pub remove_spaces: bool,
    /// Coverage emission (always ON in Perl; preserved for compat).
    pub counts: bool,
    /// 0-based half-open coordinates (--bedGraph/--cytosine_report only).
    pub zero_based: bool,
    /// All-C-context cytosine report (--cytosine_report only).
    pub cx_context: bool,
    /// Per-chromosome cytosine report (--cytosine_report only).
    pub split_by_chromosome: bool,
    /// UCSC-compatible bedGraph (--bedGraph only).
    pub ucsc: bool,
    /// Sort buffer size string (passed to bismark2bedGraph). `None` if
    /// the user didn't pass `--buffer_size`; the subprocess will use
    /// `bismark2bedGraph`'s own default ("2G" per Perl).
    pub buffer_size: Option<String>,
    /// Gazillion-contigs mode (mutex with ample_memory).
    pub gazillion: bool,
    /// In-memory sort path (mutex with gazillion).
    pub ample_memory: bool,
    /// Genome folder for cytosine_report (mandatory when cytosine_report set).
    pub genome_folder: Option<PathBuf>,
    /// Rayon worker thread count (Phase F).
    pub parallel: usize,
    /// Suppress informational stderr log (#882). Errors/warnings still print.
    pub quiet: bool,
    /// Include `@SQ` lines in header provenance (#882).
    pub verbose: bool,
}

impl ResolvedConfig {
    /// True when `--mbias_only` is in effect: skip per-context file writes
    /// and silence `InvalidXmByte` errors. `M-bias.txt` +
    /// `_splitting_report.txt` are still produced.
    ///
    /// Centralised predicate (Phase E rev 1, Reviewer B Important-1):
    /// every site that needs to test for mbias-only (`ExtractState::new`,
    /// `OutputFileMap::new` via mode dispatch, `pipeline.rs::extract_se/pe`'s
    /// `mbias_only_silence` derivation) calls this one method so the three
    /// derivations can't drift.
    pub fn is_mbias_only(&self) -> bool {
        self.output_mode == OutputMode::MbiasOnly
    }
}

impl Cli {
    /// Validate CLI args + reject documented mutex/precondition violations.
    ///
    /// Validation order (matches the SPEC §11 + Perl source):
    /// 1. M-bias mutexes (Perl 1034-1038).
    /// 2. Memory-strategy mutex (`--gazillion` × `--ample_memory`, Perl 1310-1312).
    /// 3. `--yacht` PE rejection + `--mbias_only` mutex (Perl 1328-1336).
    /// 4. `--cytosine_report` precondition (`--genome_folder` required;
    ///    SPEC §11 rev 2).
    /// 5. Flag-only-valid-with rules (`--zero_based`, `--CX`, etc).
    /// 6. `--parallel 0` rejection.
    /// 7. Input file existence (fail fast).
    pub fn validate(self) -> Result<ResolvedConfig, BismarkExtractorError> {
        // M-bias mutex group (Perl 1034-1038).
        if self.mbias_only && self.bedgraph {
            return Err(BismarkExtractorError::MbiasOnlyWithBedGraph);
        }
        if self.mbias_only && self.cytosine_report {
            return Err(BismarkExtractorError::MbiasOnlyWithCytosineReport);
        }
        if self.mbias_only && self.mbias_off {
            return Err(BismarkExtractorError::MbiasOnlyWithMbiasOff);
        }

        // Memory strategy mutex (Perl 1310-1312).
        if self.gazillion && self.ample_memory {
            return Err(BismarkExtractorError::GazillionWithAmpleMemory);
        }

        // Explicit --buffer_size × --ample_memory mutex (Perl 1295).
        // Reviewer A Medium #1: only fires on EXPLICIT --buffer_size, not
        // on the implicit default. `Option<String>` captures this.
        if self.buffer_size.is_some() && self.ample_memory {
            return Err(BismarkExtractorError::BufferSizeWithAmpleMemory);
        }

        // --include_overlap is PE-only (Perl 1217).
        // Reviewer A Medium #2.
        if self.include_overlap && !self.paired_end {
            return Err(BismarkExtractorError::IncludeOverlapRequiresPairedEnd);
        }

        // --yacht constraints (Perl 1328-1336).
        if self.yacht && self.paired_end {
            return Err(BismarkExtractorError::YachtRequiresSingleEnd);
        }
        if self.yacht && self.mbias_only {
            return Err(BismarkExtractorError::YachtWithMbiasOnly);
        }
        // --yacht × --bedGraph/--cytosine_report (inline-streaming epic Phase 2,
        // T6). DELIBERATE divergence from Perl (which has no such mutex): the
        // yacht `any_C_context_*` file is not consumable by the bedGraph chain,
        // so the combination would silently produce no downstream output. We
        // guard on the RESOLVED bedgraph flag (`--cytosine_report` forces
        // `--bedGraph` below), so `--cytosine_report` alone also trips this.
        if self.yacht && (self.bedgraph || self.cytosine_report) {
            return Err(BismarkExtractorError::YachtWithBedgraphOrCytosineReport);
        }

        // --cytosine_report needs --genome_folder (SPEC §11 rev 2).
        if self.cytosine_report && self.genome_folder.is_none() {
            return Err(BismarkExtractorError::CytosineReportRequiresGenomeFolder);
        }

        // Flag-only-valid-with rules.
        if self.zero_based && !self.bedgraph && !self.cytosine_report {
            return Err(BismarkExtractorError::ZeroBasedRequiresBedgraphOrCytosineReport);
        }
        if self.ucsc && !self.bedgraph {
            return Err(BismarkExtractorError::UcscRequiresBedgraph);
        }
        if self.cx_context && !self.cytosine_report {
            return Err(BismarkExtractorError::CxRequiresCytosineReport);
        }
        if self.split_by_chromosome && !self.cytosine_report {
            return Err(BismarkExtractorError::SplitByChromosomeRequiresCytosineReport);
        }

        // --parallel >= 1 (Clap's u32 parser accepts 0).
        if self.parallel == 0 {
            return Err(BismarkExtractorError::InvalidParallelValue { value: 0 });
        }

        // Empty input file list.
        if self.files.is_empty() {
            return Err(BismarkExtractorError::NoInputFiles);
        }

        // Input file existence (fail fast).
        for path in &self.files {
            if !path.exists() {
                return Err(BismarkExtractorError::InputFileNotFound(path.clone()));
            }
        }

        // --genome_folder must be an existing directory (fail fast — defer
        // .fai check to Phase G). Reviewer B Medium: was `exists()` which
        // also accepted files. Tightened to `is_dir()` — the genome folder
        // is expected to contain Bismark-prepared FASTA + .fai indexes.
        if let Some(gf) = &self.genome_folder
            && !gf.is_dir()
        {
            return Err(BismarkExtractorError::GenomeFolderNotFound(gf.clone()));
        }

        // Derived: library mode.
        let paired_mode = match (self.single_end, self.paired_end) {
            (true, false) => PairedMode::SingleEnd,
            (false, true) => PairedMode::PairedEnd,
            (false, false) => PairedMode::AutoDetect,
            (true, true) => unreachable!("clap conflicts_with prevents this"),
        };

        // Derived: output mode. --yacht wins over --mbias_only mutex
        // already rejected above. The other combinations are orthogonal.
        let output_mode = if self.mbias_only {
            OutputMode::MbiasOnly
        } else if self.yacht {
            OutputMode::Yacht
        } else {
            match (self.comprehensive, self.merge_non_cpg) {
                (false, false) => OutputMode::Default,
                (true, false) => OutputMode::Comprehensive,
                (false, true) => OutputMode::MergeNonCpG,
                (true, true) => OutputMode::ComprehensiveMergeNonCpG,
            }
        };

        // Derived: --cytosine_report auto-triggers --bedGraph (Perl 1281-1283).
        let bedgraph = self.bedgraph || self.cytosine_report;

        // Derived: no_overlap. PE default is ON; --include_overlap overrides.
        // SE doesn't have R2, so the field is meaningless there (kept as
        // false for SE).
        //
        // Phase C rev 1 fix (Reviewer A §1.1 Critical): include AutoDetect
        // in the "set to !include_overlap" branch. Rev 0's `== PairedEnd`
        // left AutoDetect at false, so a Phase C dispatch that auto-detects
        // PE would silently leak R2 overlap calls. The fix makes any non-SE
        // path inherit the PE default; SE actual extraction ignores the
        // field (no overlap concept).
        let no_overlap = if paired_mode != PairedMode::SingleEnd {
            !self.include_overlap
        } else {
            false
        };

        // --samtools_path is silently accepted (no warning) — matches
        // bismark-dedup precedent at `cli.rs:228-230`.
        let _ = self.samtools_path;

        Ok(ResolvedConfig {
            files: self.files,
            paired_mode,
            output_mode,
            ignore_5p_r1: self.ignore,
            ignore_3p_r1: self.ignore_3prime,
            ignore_5p_r2: self.ignore_r2,
            ignore_3p_r2: self.ignore_3prime_r2,
            no_overlap,
            output_dir: self.output_dir,
            no_header: self.no_header,
            gzip: self.gzip,
            emit_splitting_report: self.report,
            fasta_annotation: self.fasta,
            mbias_off: self.mbias_off,
            bedgraph,
            cytosine_report: self.cytosine_report,
            cutoff: self.cutoff,
            remove_spaces: self.remove_spaces,
            counts: self.counts,
            zero_based: self.zero_based,
            cx_context: self.cx_context,
            split_by_chromosome: self.split_by_chromosome,
            ucsc: self.ucsc,
            buffer_size: self.buffer_size,
            gazillion: self.gazillion,
            ample_memory: self.ample_memory,
            genome_folder: self.genome_folder,
            parallel: self.parallel as usize,
            quiet: self.quiet,
            verbose: self.verbose,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full = vec!["bismark_methylation_extractor_rs"];
        full.extend(args.iter().copied());
        Cli::try_parse_from(full)
    }

    /// Build a temp BAM/SAM path for tests — file existence is checked
    /// in validate(), so we use `tempfile` to make existence guaranteed.
    fn temp_input() -> tempfile::NamedTempFile {
        let f = tempfile::Builder::new()
            .suffix(".bam")
            .tempfile()
            .expect("tempfile");
        // Write a single byte so the file isn't 0-length (defensive).
        std::fs::write(f.path(), b"x").expect("write tempfile");
        f
    }

    // ─── clap config sanity ─────────────────────────────────────────

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn version_flag_parses_without_files() {
        let cli = parse(&["--version"]).unwrap();
        assert!(cli.version);
        assert!(cli.files.is_empty());
    }

    #[test]
    fn parses_single_positional_input() {
        let cli = parse(&["sample.bam"]).unwrap();
        assert_eq!(cli.files, vec![PathBuf::from("sample.bam")]);
        assert!(!cli.single_end && !cli.paired_end);
    }

    #[test]
    fn single_end_and_paired_end_are_mutex_at_parse_time() {
        let err = parse(&["-s", "-p", "sample.bam"]).unwrap_err();
        assert!(
            err.to_string().contains("cannot be used with"),
            "got: {err}"
        );
    }

    // ─── validate(): mutex rejections ────────────────────────────────

    #[test]
    fn validate_rejects_mbias_only_with_bedgraph() {
        let f = temp_input();
        let cli = parse(&["--mbias_only", "--bedGraph", f.path().to_str().unwrap()]).unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::MbiasOnlyWithBedGraph)
        ));
    }

    #[test]
    fn validate_rejects_mbias_only_with_cytosine_report() {
        let f = temp_input();
        let cli = parse(&[
            "--mbias_only",
            "--cytosine_report",
            "--genome_folder",
            "/tmp",
            f.path().to_str().unwrap(),
        ])
        .unwrap();
        // The genome_folder needs to exist for that branch not to fire first.
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::MbiasOnlyWithCytosineReport)
        ));
    }

    #[test]
    fn validate_rejects_mbias_only_with_mbias_off() {
        let f = temp_input();
        let cli = parse(&["--mbias_only", "--mbias_off", f.path().to_str().unwrap()]).unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::MbiasOnlyWithMbiasOff)
        ));
    }

    #[test]
    fn validate_rejects_gazillion_with_ample_memory() {
        let f = temp_input();
        let cli = parse(&["--gazillion", "--ample_memory", f.path().to_str().unwrap()]).unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::GazillionWithAmpleMemory)
        ));
    }

    #[test]
    fn validate_rejects_yacht_with_paired_end() {
        let f = temp_input();
        let cli = parse(&["--yacht", "--paired-end", f.path().to_str().unwrap()]).unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::YachtRequiresSingleEnd)
        ));
    }

    #[test]
    fn validate_rejects_yacht_with_mbias_only() {
        let f = temp_input();
        let cli = parse(&["--yacht", "--mbias_only", f.path().to_str().unwrap()]).unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::YachtWithMbiasOnly)
        ));
    }

    /// Inline-streaming epic Phase 2 (T6): `--yacht --bedGraph` rejects.
    /// Deliberate divergence from Perl (which has no such mutex).
    #[test]
    fn validate_rejects_yacht_with_bedgraph() {
        let f = temp_input();
        let cli = parse(&["--yacht", "--bedGraph", f.path().to_str().unwrap()]).unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::YachtWithBedgraphOrCytosineReport)
        ));
    }

    /// Inline-streaming epic Phase 2 (T6): `--yacht --cytosine_report` rejects
    /// (cytosine_report forces bedgraph; the resolved-bedgraph guard catches it).
    #[test]
    fn validate_rejects_yacht_with_cytosine_report() {
        let f = temp_input();
        let tmp_dir = tempfile::tempdir().unwrap();
        let cli = parse(&[
            "--yacht",
            "--cytosine_report",
            "--genome_folder",
            tmp_dir.path().to_str().unwrap(),
            f.path().to_str().unwrap(),
        ])
        .unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::YachtWithBedgraphOrCytosineReport)
        ));
    }

    #[test]
    fn validate_rejects_cytosine_report_without_genome_folder() {
        let f = temp_input();
        let cli = parse(&["--cytosine_report", f.path().to_str().unwrap()]).unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::CytosineReportRequiresGenomeFolder)
        ));
    }

    #[test]
    fn validate_rejects_zero_based_without_bedgraph_or_cytosine() {
        let f = temp_input();
        let cli = parse(&["--zero_based", f.path().to_str().unwrap()]).unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::ZeroBasedRequiresBedgraphOrCytosineReport)
        ));
    }

    #[test]
    fn validate_rejects_ucsc_without_bedgraph() {
        let f = temp_input();
        let cli = parse(&["--ucsc", f.path().to_str().unwrap()]).unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::UcscRequiresBedgraph)
        ));
    }

    #[test]
    fn validate_rejects_cx_without_cytosine_report() {
        let f = temp_input();
        let cli = parse(&["--CX", f.path().to_str().unwrap()]).unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::CxRequiresCytosineReport)
        ));
    }

    #[test]
    fn validate_rejects_split_by_chromosome_without_cytosine_report() {
        let f = temp_input();
        let cli = parse(&["--split_by_chromosome", f.path().to_str().unwrap()]).unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::SplitByChromosomeRequiresCytosineReport)
        ));
    }

    #[test]
    fn validate_rejects_parallel_zero() {
        let f = temp_input();
        let cli = parse(&["--parallel", "0", f.path().to_str().unwrap()]).unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::InvalidParallelValue { value: 0 })
        ));
    }

    #[test]
    fn validate_rejects_no_input_files() {
        // `--version` lets us reach validate with empty files.
        let cli = parse(&[]).unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::NoInputFiles)
        ));
    }

    #[test]
    fn validate_rejects_input_file_not_found() {
        let cli = parse(&["/tmp/definitely_does_not_exist_98765.bam"]).unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::InputFileNotFound(_))
        ));
    }

    /// Reviewer A Medium #1: explicit `--buffer_size 4G` combined with
    /// `--ample_memory` should fatal-error, mirroring Perl line 1295's
    /// `die unless($sort_size)` semantics.
    #[test]
    fn validate_rejects_explicit_buffer_size_with_ample_memory() {
        let f = temp_input();
        let cli = parse(&[
            "--buffer_size",
            "4G",
            "--ample_memory",
            f.path().to_str().unwrap(),
        ])
        .unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::BufferSizeWithAmpleMemory)
        ));
    }

    /// Reviewer A Medium #1 (negative case): `--ample_memory` ALONE
    /// (no explicit `--buffer_size`) is fine. The default 2G doesn't
    /// trip the explicit-vs-default mutex.
    #[test]
    fn validate_ample_memory_alone_passes() {
        let f = temp_input();
        let config = parse(&["--ample_memory", f.path().to_str().unwrap()])
            .unwrap()
            .validate()
            .unwrap();
        assert!(config.ample_memory);
        assert!(config.buffer_size.is_none());
    }

    /// Reviewer A Medium #2: `--include_overlap` without `--paired-end`
    /// rejects (Perl line 1217).
    #[test]
    fn validate_rejects_include_overlap_without_paired_end() {
        let f = temp_input();
        let cli = parse(&["--include_overlap", f.path().to_str().unwrap()]).unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::IncludeOverlapRequiresPairedEnd)
        ));
    }

    /// Reviewer B Medium: `--genome_folder /path/to/regular-file` should
    /// reject. Previously `gf.exists()` returned true for files; tightened
    /// to `gf.is_dir()`.
    #[test]
    fn validate_rejects_genome_folder_that_is_a_file_not_a_dir() {
        let f = temp_input();
        // Use the input file's path as the (deliberately wrong) genome folder.
        let cli = parse(&[
            "--cytosine_report",
            "--genome_folder",
            f.path().to_str().unwrap(),
            f.path().to_str().unwrap(),
        ])
        .unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::GenomeFolderNotFound(_))
        ));
    }

    /// Reviewer A Low (test gap): `--genome_folder` pointing at a
    /// nonexistent path also rejects.
    #[test]
    fn validate_rejects_genome_folder_that_does_not_exist() {
        let f = temp_input();
        let cli = parse(&[
            "--cytosine_report",
            "--genome_folder",
            "/tmp/definitely_not_a_real_genome_path_98765",
            f.path().to_str().unwrap(),
        ])
        .unwrap();
        assert!(matches!(
            cli.validate(),
            Err(BismarkExtractorError::GenomeFolderNotFound(_))
        ));
    }

    // ─── validate(): derived-config correctness ──────────────────────

    #[test]
    fn validate_resolves_paired_mode_explicit_single() {
        let f = temp_input();
        let config = parse(&["-s", f.path().to_str().unwrap()])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(config.paired_mode, PairedMode::SingleEnd);
    }

    #[test]
    fn validate_resolves_paired_mode_explicit_paired() {
        let f = temp_input();
        let config = parse(&["-p", f.path().to_str().unwrap()])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(config.paired_mode, PairedMode::PairedEnd);
    }

    #[test]
    fn validate_resolves_paired_mode_auto_detect_default() {
        let f = temp_input();
        let config = parse(&[f.path().to_str().unwrap()])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(config.paired_mode, PairedMode::AutoDetect);
    }

    #[test]
    fn validate_resolves_output_mode_default() {
        let f = temp_input();
        let config = parse(&[f.path().to_str().unwrap()])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(config.output_mode, OutputMode::Default);
    }

    #[test]
    fn validate_resolves_output_mode_comprehensive() {
        let f = temp_input();
        let config = parse(&["--comprehensive", f.path().to_str().unwrap()])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(config.output_mode, OutputMode::Comprehensive);
    }

    #[test]
    fn validate_resolves_output_mode_merge_non_cpg() {
        let f = temp_input();
        let config = parse(&["--merge_non_CpG", f.path().to_str().unwrap()])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(config.output_mode, OutputMode::MergeNonCpG);
    }

    #[test]
    fn validate_resolves_output_mode_comprehensive_merge_non_cpg() {
        let f = temp_input();
        let config = parse(&[
            "--comprehensive",
            "--merge_non_CpG",
            f.path().to_str().unwrap(),
        ])
        .unwrap()
        .validate()
        .unwrap();
        assert_eq!(config.output_mode, OutputMode::ComprehensiveMergeNonCpG);
    }

    #[test]
    fn validate_resolves_output_mode_yacht() {
        let f = temp_input();
        let config = parse(&["--yacht", f.path().to_str().unwrap()])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(config.output_mode, OutputMode::Yacht);
    }

    #[test]
    fn validate_resolves_output_mode_mbias_only() {
        let f = temp_input();
        let config = parse(&["--mbias_only", f.path().to_str().unwrap()])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(config.output_mode, OutputMode::MbiasOnly);
    }

    #[test]
    fn validate_cytosine_report_auto_triggers_bedgraph() {
        let f = temp_input();
        let tmp_dir = tempfile::tempdir().unwrap();
        let config = parse(&[
            "--cytosine_report",
            "--genome_folder",
            tmp_dir.path().to_str().unwrap(),
            f.path().to_str().unwrap(),
        ])
        .unwrap()
        .validate()
        .unwrap();
        assert!(
            config.bedgraph,
            "--cytosine_report should auto-trigger --bedGraph"
        );
        assert!(config.cytosine_report);
    }

    #[test]
    fn validate_pe_no_overlap_defaults_on() {
        let f = temp_input();
        let config = parse(&["-p", f.path().to_str().unwrap()])
            .unwrap()
            .validate()
            .unwrap();
        assert!(config.no_overlap, "PE default should enable --no_overlap");
    }

    #[test]
    fn validate_pe_include_overlap_overrides_no_overlap() {
        let f = temp_input();
        let config = parse(&["-p", "--include_overlap", f.path().to_str().unwrap()])
            .unwrap()
            .validate()
            .unwrap();
        assert!(
            !config.no_overlap,
            "--include_overlap should disable no_overlap"
        );
    }

    #[test]
    fn validate_se_no_overlap_is_false() {
        let f = temp_input();
        let config = parse(&["-s", f.path().to_str().unwrap()])
            .unwrap()
            .validate()
            .unwrap();
        assert!(
            !config.no_overlap,
            "SE has no R2; no_overlap is meaningless and stays false"
        );
    }

    #[test]
    fn validate_samtools_path_silently_accepted() {
        let f = temp_input();
        let config = parse(&[
            "--samtools_path",
            "/usr/bin/samtools",
            f.path().to_str().unwrap(),
        ])
        .unwrap()
        .validate()
        .unwrap();
        // No warning, no rejection. Field is in Cli but dropped at validate.
        assert_eq!(config.files.len(), 1);
    }

    #[test]
    fn validate_parallel_aliases_multicore() {
        let f = temp_input();
        let config = parse(&["--multicore", "4", f.path().to_str().unwrap()])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(config.parallel, 4);
    }

    #[test]
    fn validate_ignore_flags_thread_through() {
        let f = temp_input();
        let config = parse(&[
            "--ignore",
            "5",
            "--ignore_r2",
            "3",
            "--ignore_3prime",
            "2",
            "--ignore_3prime_r2",
            "1",
            f.path().to_str().unwrap(),
        ])
        .unwrap()
        .validate()
        .unwrap();
        assert_eq!(config.ignore_5p_r1, 5);
        assert_eq!(config.ignore_5p_r2, 3);
        assert_eq!(config.ignore_3p_r1, 2);
        assert_eq!(config.ignore_3p_r2, 1);
    }

    #[test]
    fn validate_cx_alias_cx_context() {
        let f = temp_input();
        let tmp_dir = tempfile::tempdir().unwrap();
        let config = parse(&[
            "--CX_context",
            "--cytosine_report",
            "--genome_folder",
            tmp_dir.path().to_str().unwrap(),
            f.path().to_str().unwrap(),
        ])
        .unwrap()
        .validate()
        .unwrap();
        assert!(config.cx_context);
    }
}
