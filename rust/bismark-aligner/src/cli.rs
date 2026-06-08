//! Command-line surface (clap derive).
//!
//! Declares the **full** Bismark option set (the `GetOptions` block, Perl
//! 7317–7382) so `--help` and `@PG` argv fidelity are complete, but only the
//! **v1 spine** (Bowtie2 + FastQ + directional + SE) is *wired* downstream by
//! [`crate::config`]. Options exclusive to later phases / the HISAT2-minimap2
//! follow-up are parsed and either stored or rejected at the point of use.
//!
//! Flag spellings match the Perl tool (underscores preserved: `--single_end`,
//! `--path_to_bowtie2`, `--non_directional`). `--version` is handled manually
//! (clap auto-version disabled) so the binary can print the Bismark banner.

use std::path::PathBuf;

use clap::Parser;

/// The Bismark aligner wrapper (Rust port). Phase 1: parse + discover + detect.
#[derive(Parser, Debug)]
#[command(
    name = "bismark_rs",
    about = "Bisulfite read aligner — wraps Bowtie 2 against a bisulfite-converted genome.",
    disable_version_flag = true
)]
pub struct Cli {
    // ---- genome + reads ----------------------------------------------------
    /// Genome folder (prepared with bismark_genome_preparation). May also be
    /// given as the first positional argument.
    #[arg(long = "genome", visible_alias = "genome_folder", value_name = "PATH")]
    pub genome: Option<PathBuf>,

    /// Comma-separated Read-1 mate files (paired-end).
    #[arg(short = '1', value_name = "mates1")]
    pub mates1: Option<String>,

    /// Comma-separated Read-2 mate files (paired-end).
    #[arg(short = '2', value_name = "mates2")]
    pub mates2: Option<String>,

    /// Single-end read file(s), separated by `,` or `:`.
    #[arg(long = "single_end", visible_alias = "se", value_name = "files")]
    pub single_end: Option<String>,

    /// Positional file(s): `<genome_folder>` (if `--genome` absent) then the
    /// single-end read file(s).
    #[arg(value_name = "files")]
    pub positional: Vec<String>,

    // ---- aligner selection + paths ----------------------------------------
    /// Align with Bowtie 2 (default).
    #[arg(long)]
    pub bowtie2: bool,
    /// Align with HISAT2 (single-core only — see the multicore reject).
    #[arg(long)]
    pub hisat2: bool,
    /// Align with minimap2 (single-end only; paired-end is rejected).
    #[arg(long = "minimap2", visible_alias = "mm2")]
    pub minimap2: bool,
    /// Folder containing the `bowtie2` executable (not the executable itself).
    #[arg(long = "path_to_bowtie2", value_name = "PATH")]
    pub path_to_bowtie2: Option<PathBuf>,
    /// Folder containing `hisat2`.
    #[arg(long = "path_to_hisat2", value_name = "PATH")]
    pub path_to_hisat2: Option<PathBuf>,
    /// Folder containing `minimap2`.
    #[arg(long = "path_to_minimap2", value_name = "PATH")]
    pub path_to_minimap2: Option<PathBuf>,
    /// Folder containing `samtools`.
    #[arg(long = "samtools_path", value_name = "PATH")]
    pub samtools_path: Option<PathBuf>,

    // ---- input format / library type --------------------------------------
    /// Reads are FASTA.
    #[arg(short = 'f', long = "fasta")]
    pub fasta: bool,
    /// Reads are FASTQ (default).
    #[arg(short = 'q', long = "fastq")]
    pub fastq: bool,
    /// Non-directional library (4 alignment instances).
    #[arg(long = "non_directional")]
    pub non_directional: bool,
    /// PBAT library.
    #[arg(long)]
    pub pbat: bool,
    /// SLAM-seq mode.
    #[arg(long)]
    pub slam: bool,
    /// EXPERIMENTAL (v2, opt-in, never-silent): align against a single combined
    /// CT+GA index (`Bisulfite_Genome/Combined/BS_combined`) in one both-strands
    /// Bowtie 2 pass instead of separate per-strand instances, recovering strand
    /// from the RNAME suffix × FLAG. Concordance-gated, NOT byte-identical to the
    /// faithful default. This phase: single-end directional only.
    #[arg(long = "combined_index")]
    pub combined_index: bool,

    /// EXPERIMENTAL (v2, opt-in, never-silent): the single-pass "model (b)"
    /// execution model for `--combined_index --non_directional`. Aligns ONE
    /// Bowtie 2 pass over conversion-tagged interleaved reads (one combined index
    /// load instead of two — lower peak RSS) instead of model (a)'s two parallel
    /// passes. Requires `--combined_index --non_directional` (single-end Bowtie 2).
    /// NOT byte-identical AND NOT decision-equivalent to model (a): the qname tag
    /// perturbs Bowtie 2's read-name-seeded RNG, so a tiny fraction of co-optimal
    /// reads get a different (validated-equally-accurate) alignment. Ground-truth
    /// validated against Sherman; never the default.
    #[arg(long = "combined_index_single_pass")]
    pub combined_index_single_pass: bool,

    // ---- read trimming / quality ------------------------------------------
    /// Skip the first <int> reads/pairs.
    #[arg(short = 's', long = "skip", value_name = "int")]
    pub skip: Option<u64>,
    /// Align only the first <int> reads/pairs.
    #[arg(short = 'u', long = "upto", value_name = "int")]
    pub upto: Option<u64>,
    /// Qualities are Phred+33 (requires -q).
    #[arg(long = "phred33-quals")]
    pub phred33: bool,
    /// Qualities are Phred+64 (requires -q).
    #[arg(long = "phred64-quals")]
    pub phred64: bool,

    // ---- Bowtie 2 alignment parameters ------------------------------------
    /// Multiseed mismatches, 0 or 1 (Bowtie 2 -N).
    #[arg(short = 'n', long = "seedmms", value_name = "int")]
    pub seedmms: Option<i64>,
    /// Seed length (Bowtie 2 -L).
    #[arg(short = 'l', long = "seedlen", value_name = "int")]
    pub seedlen: Option<u32>,
    /// Consecutive seed-extension fails (Bowtie 2 -D).
    #[arg(short = 'D', value_name = "int")]
    pub seed_extension_fails: Option<u32>,
    /// Re-seed repetitive seeds (Bowtie 2 -R).
    #[arg(short = 'R', value_name = "int")]
    pub reseed_repetitive_seeds: Option<u32>,
    /// Min-score function, e.g. `L,0,-0.2` (Bowtie 2 --score-min).
    #[arg(long = "score_min", value_name = "func")]
    pub score_min: Option<String>,
    /// Read gap open,extend (Bowtie 2 --rdg).
    #[arg(long = "rdg", value_name = "int,int")]
    pub rdg: Option<String>,
    /// Reference gap open,extend (Bowtie 2 --rfg).
    #[arg(long = "rfg", value_name = "int,int")]
    pub rfg: Option<String>,
    /// Minimum insert size (paired-end, Bowtie 2 -I).
    #[arg(short = 'I', long = "minins", value_name = "int")]
    pub minins: Option<u32>,
    /// Maximum insert size (paired-end, Bowtie 2 -X).
    #[arg(short = 'X', long = "maxins", value_name = "int")]
    pub maxins: Option<u32>,
    /// Deprecated (-M); no effect.
    #[arg(long = "most_valid_alignments", value_name = "int")]
    pub most_valid_alignments: Option<i64>,
    /// Bowtie 2 local-alignment mode (deferred — off the v1 byte-identity spine).
    #[arg(long)]
    pub local: bool,
    /// Allow a non-bisulfite mismatch.
    #[arg(long = "non_bs_mm")]
    pub non_bs_mm: bool,
    /// Threads *per Bowtie 2 instance* (Bowtie 2 -p; ≥ 2).
    #[arg(short = 'p', value_name = "int")]
    pub bowtie_threads: Option<u32>,
    /// `--dovetail` (paired-end; kept for backwards compatibility, no effect here).
    #[arg(long)]
    pub dovetail: bool,
    /// Disable the automatic `--dovetail` for paired-end.
    #[arg(long = "no_dovetail")]
    pub no_dovetail: bool,

    // ---- parallelisation (file-level) -------------------------------------
    /// File-level multicore (split input into N chunks). Wired in Phase 9.
    #[arg(long = "multicore", visible_alias = "parallel", value_name = "int")]
    pub multicore: Option<u32>,

    // ---- output -----------------------------------------------------------
    /// Output directory (default: current directory).
    #[arg(short = 'o', long = "output_dir", value_name = "PATH")]
    pub output_dir: Option<PathBuf>,
    /// Temporary directory (default: output directory's parent / CWD).
    #[arg(long = "temp_dir", value_name = "PATH")]
    pub temp_dir: Option<PathBuf>,
    /// Output base name (overrides the derived name).
    #[arg(short = 'B', long = "basename", value_name = "name")]
    pub basename: Option<String>,
    /// Prefix prepended to the output file name.
    #[arg(long = "prefix", value_name = "str")]
    pub prefix: Option<String>,
    /// Write SAM instead of BAM (deferred in v1).
    #[arg(long)]
    pub sam: bool,
    /// Write CRAM instead of BAM (deferred in v1).
    #[arg(long)]
    pub cram: bool,
    /// CRAM reference.
    #[arg(long = "cram_ref", value_name = "PATH")]
    pub cram_ref: Option<PathBuf>,
    /// Gzip the (SAM/text) output.
    #[arg(long)]
    pub gzip: bool,
    /// Use the legacy (pre-v0.8.3) SAM FLAG values.
    #[arg(long = "old_flag")]
    pub old_flag: bool,
    /// Omit the SAM `@HD`/`@SQ`/`@PG` header.
    #[arg(long = "sam-no-hd")]
    pub sam_no_hd: bool,
    /// Write unmapped reads to a file.
    #[arg(long = "unmapped", visible_alias = "un")]
    pub unmapped: bool,
    /// Write ambiguously-mapping reads to a file.
    #[arg(long = "ambiguous")]
    pub ambiguous: bool,
    /// Also write ambiguous alignments to a BAM.
    #[arg(long = "ambig_bam")]
    pub ambig_bam: bool,
    /// Add a nucleotide-coverage report.
    #[arg(long = "nucleotide_coverage")]
    pub nucleotide_coverage: bool,

    // ---- read-group tags --------------------------------------------------
    /// Add an `@RG` read-group header line.
    #[arg(long = "rg_tag")]
    pub rg_tag: bool,
    /// Read-group ID.
    #[arg(long = "rg_id", value_name = "str")]
    pub rg_id: Option<String>,
    /// Read-group sample.
    #[arg(long = "rg_sample", value_name = "str")]
    pub rg_sample: Option<String>,

    // ---- HISAT2 / minimap2 specific ---------------------------------------
    /// HISAT2 known splice sites (HISAT2 mode only).
    #[arg(long = "known-splicesite-infile", value_name = "PATH")]
    pub known_splices: Option<PathBuf>,
    /// HISAT2: disable spliced alignment (HISAT2 mode only).
    #[arg(long = "no-spliced-alignment")]
    pub nosplice: bool,
    /// Truncate read IDs at the first space/tab instead of replacing
    /// whitespace with underscores (Bismark issue #236; affects `fix_IDs`).
    #[arg(long)]
    pub icpc: bool,
    /// minimap2 short-read preset (`-x sr`; minimap2 mode only).
    #[arg(long = "mm2_short_reads")]
    pub mm2_short_read: bool,
    /// minimap2 maximum read length cutoff (`100..=100000`; default 10000;
    /// minimap2 mode only).
    #[arg(long = "mm2_maximum_length", value_name = "int")]
    pub maximum_length_cutoff: Option<u32>,
    /// minimap2 PacBio preset (`-x map-pb`; minimap2 mode only).
    #[arg(long = "mm2_pacbio", visible_alias = "pacbio")]
    pub mm2_pacbio: bool,
    /// minimap2 Nanopore preset (`-x map-ont`; the default; minimap2 mode only).
    #[arg(long = "mm2_nanopore", visible_alias = "nanopore")]
    pub mm2_nanopore: bool,
    /// Report the strand identity (deferred).
    #[arg(long = "strandID")]
    pub strand_id: bool,

    // ---- meta -------------------------------------------------------------
    /// Suppress most Bowtie 2 warnings (Bowtie 2 --quiet).
    #[arg(long)]
    pub quiet: bool,
    /// Print version information and exit.
    #[arg(long = "version")]
    pub version: bool,
}
