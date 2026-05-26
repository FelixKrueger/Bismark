//! Typed errors for `bismark-extractor`.
//!
//! Phase A: errors produced at the CLI-validation boundary. Phase B+
//! will add pipeline/extraction errors via `#[from] BismarkIoError`
//! (same pattern as `bismark-dedup`'s `BismarkDedupError`).

use std::path::PathBuf;

/// All errors raised by `bismark-extractor` at validation time. Pipeline
/// + extraction errors land in subsequent phases.
#[derive(Debug, thiserror::Error)]
pub enum BismarkExtractorError {
    /// User passed no positional input files.
    #[error(
        "no input file(s) provided — pass one or more Bismark-aligned \
         BAM/SAM/CRAM paths on the command line"
    )]
    NoInputFiles,

    /// `--mbias_only` is mutex with `--bedGraph`. Mirrors Perl
    /// `bismark_methylation_extractor:1037-1038`.
    #[error(
        "--mbias_only cannot be combined with --bedGraph (extracting M-bias \
         skips the per-context output files that --bedGraph consumes)"
    )]
    MbiasOnlyWithBedGraph,

    /// `--mbias_only` is mutex with `--cytosine_report` (Perl forces the
    /// `--bedGraph` chain via `--cytosine_report` and then dies on the
    /// `--bedGraph` mutex).
    #[error(
        "--mbias_only cannot be combined with --cytosine_report \
         (--cytosine_report implies --bedGraph, which conflicts)"
    )]
    MbiasOnlyWithCytosineReport,

    /// `--mbias_only` is mutex with `--mbias_off`. Mirrors Perl
    /// `bismark_methylation_extractor:1034-1036`.
    #[error("--mbias_only and --mbias_off are mutually exclusive")]
    MbiasOnlyWithMbiasOff,

    /// `--gazillion` is mutex with `--ample_memory`. Mirrors Perl
    /// `bismark_methylation_extractor:1310-1312`.
    #[error(
        "--gazillion (--scaffolds) and --ample_memory are mutually exclusive \
         (--gazillion forces a single-file UNIX-sort path; --ample_memory \
         forces the in-memory sort path)"
    )]
    GazillionWithAmpleMemory,

    /// Explicit `--buffer_size` is mutex with `--ample_memory`. Mirrors
    /// Perl `bismark_methylation_extractor:1295` (`unless($sort_size)`
    /// — Perl only fires this `die` when the user explicitly set
    /// `--buffer_size`; the implicit "2G" default doesn't trip it).
    /// The Rust port preserves this explicit-vs-default distinction by
    /// making `buffer_size: Option<String>` (None = default).
    #[error(
        "explicit --buffer_size and --ample_memory are mutually exclusive \
         (--ample_memory uses an in-memory sort path; --buffer_size only \
         applies to the UNIX-sort path)"
    )]
    BufferSizeWithAmpleMemory,

    /// `--include_overlap` is paired-end only. Mirrors Perl
    /// `bismark_methylation_extractor:1217`.
    #[error("--include_overlap requires --paired-end (no R2 to include in SE mode)")]
    IncludeOverlapRequiresPairedEnd,

    /// `--cytosine_report` requires `--genome_folder <PATH-TO-BISMARK-GENOME-DIR>`.
    /// Locked in SPEC §11 (rev 2): the Perl default is a hardcoded mouse
    /// path; the Rust port rejects without explicit value to avoid silent
    /// mis-targeting.
    #[error(
        "--cytosine_report requires --genome_folder <PATH-TO-BISMARK-GENOME-DIR>; \
         the Perl default mouse path is not honoured in the Rust port. \
         Pass `--genome_folder /path/to/Bismark/genome/` to proceed."
    )]
    CytosineReportRequiresGenomeFolder,

    /// `--yacht` is single-end only. Mirrors Perl
    /// `bismark_methylation_extractor:1328-1336`.
    #[error("--yacht is single-end only (NOMe-Seq filtering); cannot combine with --paired-end")]
    YachtRequiresSingleEnd,

    /// `--yacht` is mutex with `--mbias_only` (yacht emits a single
    /// `any_C_context_*` file; mbias_only skips all output files).
    #[error("--yacht and --mbias_only are mutually exclusive")]
    YachtWithMbiasOnly,

    /// `--zero_based` is only valid with `--bedGraph` or `--cytosine_report`.
    /// Coordinate convention only affects those output streams.
    #[error("--zero_based is only valid with --bedGraph or --cytosine_report")]
    ZeroBasedRequiresBedgraphOrCytosineReport,

    /// `--ucsc` is only valid with `--bedGraph`. UCSC reformatting only
    /// applies to the bedGraph output.
    #[error("--ucsc is only valid with --bedGraph")]
    UcscRequiresBedgraph,

    /// `--CX` (`--CX_context`) is only valid with `--cytosine_report`.
    /// `--CX` extends the genome-walk to all C-contexts (default is CpG only).
    #[error("--CX (--CX_context) is only valid with --cytosine_report")]
    CxRequiresCytosineReport,

    /// `--split_by_chromosome` is only valid with `--cytosine_report`.
    #[error("--split_by_chromosome is only valid with --cytosine_report")]
    SplitByChromosomeRequiresCytosineReport,

    /// `--parallel N` (`--multicore N`) was given with `N == 0`. Clap's
    /// `u32` parser accepts 0 (valid u32); explicit check needed here.
    #[error("--parallel (--multicore) must be ≥ 1 (got {value})")]
    InvalidParallelValue {
        /// The invalid value the user supplied.
        value: u32,
    },

    /// Input file does not exist on disk. Validation catches this early
    /// rather than waiting for the pipeline reader to fail with a less
    /// clear `Io` error.
    #[error("input file does not exist: {0}")]
    InputFileNotFound(PathBuf),

    /// `--genome_folder <PATH>` does not exist on disk. Same rationale
    /// as `InputFileNotFound` — fail fast before the cytosine_report
    /// subprocess.
    #[error("--genome_folder path does not exist: {0}")]
    GenomeFolderNotFound(PathBuf),
}
