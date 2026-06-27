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

/// The Bismark aligner wrapper (Rust port): parse + discover + detect, then convert, align, and merge.
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
    /// Align with HISAT2. `--multicore N` is interpreted as `-p N` single-instance
    /// threading (Approach B-faithful — HISAT2 splice discovery is not chunk-invariant).
    #[arg(long)]
    pub hisat2: bool,
    /// Align with minimap2 (single-end only; paired-end is rejected).
    #[arg(long = "minimap2", visible_alias = "mm2")]
    pub minimap2: bool,
    /// Align with rammap, the pure-Rust minimap2 reimplementation (single-end
    /// only; same `map-ont`/`.mmi`/SE-only model as `--minimap2`). Opt-in,
    /// concordance-validated — NOT byte-identical to minimap2.
    #[arg(long = "rammap", visible_alias = "ram")]
    pub rammap: bool,
    /// Folder containing the `bowtie2` executable (not the executable itself).
    #[arg(long = "path_to_bowtie2", value_name = "PATH")]
    pub path_to_bowtie2: Option<PathBuf>,
    /// Folder containing `hisat2`.
    #[arg(long = "path_to_hisat2", value_name = "PATH")]
    pub path_to_hisat2: Option<PathBuf>,
    /// Folder containing `minimap2`.
    #[arg(long = "path_to_minimap2", value_name = "PATH")]
    pub path_to_minimap2: Option<PathBuf>,
    /// Folder containing `rammap`.
    #[arg(long = "path_to_rammap", value_name = "PATH")]
    pub path_to_rammap: Option<PathBuf>,
    /// `[v2/experimental]` Opt into the in-process `rammap-core` backend (single-end).
    /// `--rammap` defaults to the proven SUBPROCESS path; this flag selects the
    /// in-process path instead: **lower RAM, but slower (single-threaded)**, and
    /// **concordant — NOT byte-identical — to the subprocess** (a handful of borderline
    /// long-read alignments differ). Requires `--rammap` AND a `--features
    /// rammap-inprocess` build; on a default (feature-OFF) binary it is accepted but
    /// inert (the in-process path isn't compiled, so the subprocess runs).
    #[arg(long = "rammap_inprocess")]
    pub rammap_inprocess: bool,
    /// `[v2, opt-in, never-silent, concordance-gated]` Illumina 5-Base (5mC->T)
    /// mode. Unlike bisulfite, the 5-Base chemistry converts METHYLATED C to T and
    /// leaves unmethylated C intact, so reads keep full complexity and align to the
    /// UNCONVERTED genome with a standard aligner; methylation is then called with
    /// inverted polarity (a read T at a genomic C = methylated). v1 is single-end +
    /// directional only and aligns with minimap2 (`-x sr`) against the raw genome
    /// FASTA. NOT byte-identical (Perl Bismark has no 5-Base oracle); validated by
    /// concordance with DRAGEN. See FelixKrueger/Bismark#787.
    #[arg(long = "illumina_5base", visible_alias = "five_base")]
    pub illumina_5base: bool,
    /// `[#787 EXPERIMENTAL/PREVIEW]` After a `--illumina_5base` run, deconvolute
    /// methylation from C>T/G>A genetic variants using both strands (DRAGEN's rule), and
    /// write a per-CpG report `<out>.5base_deconvolution.txt` (chrom, pos, strand,
    /// verdict, methylated, total, %). A CpG whose OPPOSITE strand also lost the cytosine
    /// is a variant, not 5mC, and is excluded from the methylation totals. Requires
    /// `--illumina_5base`. EXPERIMENTAL: not byte-identity- or per-site-concordance-gated;
    /// the supported output is the core per-read 5-Base BAM.
    #[arg(long = "five_base_deconvolution", visible_alias = "five_base_deconv")]
    pub five_base_deconvolution: bool,
    /// `[#787]` Basename of a NORMAL (unconverted) bowtie2/hisat2 index of the genome,
    /// used by `--illumina_5base --bowtie2`/`--hisat2`. 5-Base reads keep full
    /// complexity, so they align to the plain genome index (build it once with
    /// `bowtie2-build`/`hisat2-build genome.fa <basename>`). Without an engine flag,
    /// 5-Base uses minimap2 against the genome FASTA directly and this is not needed.
    #[arg(long = "five_base_index", value_name = "BASENAME")]
    pub five_base_index: Option<PathBuf>,
    /// `[#787]` Inline UMI length at the 5' of each 5-Base read (e.g. `8`, the 7 bp UMI
    /// plus 1 spacer of the Illumina 5-Base `OverrideCycles U7N1Y#`). When greater than
    /// zero, reads are deduplicated by (UMI, chromosome, position, strand): PCR/optical
    /// duplicates are dropped (the first survives), removing methylation bias. 0 = off
    /// (the default; the aligner soft-clips any UMI bases). Requires `--illumina_5base`.
    #[arg(long = "five_base_umi_len", value_name = "int", default_value_t = 0)]
    pub five_base_umi_len: usize,
    /// `[#787]` Minimum Phred base quality for a 5-Base methylation call. Read bases
    /// below this are ignored in the call (emitted as no-call), cutting the per-base
    /// sequencing-error noise floor that otherwise inflates non-CpG "methylation"
    /// (DRAGEN's `--methylation-baseq-threshold` precedent). 0 = off. The BAM SEQ is
    /// unchanged (only the methylation call is masked). Requires `--illumina_5base`.
    #[arg(long = "five_base_baseq", value_name = "PHRED", default_value_t = 0)]
    pub five_base_baseq: u8,
    /// `[#787 EXPERIMENTAL/PREVIEW]` After a `--illumina_5base` run, group the two strands
    /// of each original molecule into a DUPLEX family (DRAGEN `nonrandom-duplex`) and
    /// reconcile the 5mC->T signal PER MOLECULE, writing `<out>.5base_duplex.txt`. PE keys
    /// each family on the FRAGMENT span (POS + mate-pos + TLEN) + canonical dual UMI — the
    /// real workflow (Illumina 5-Base is paired-end). SE-duplex is a KNOWN LIMITATION:
    /// single-end OT/OB reads cover opposite fragment ends with different spans, so they do
    /// not pair on real data (SE is not a real 5-Base workflow). Use `--five_base_umi_qname`
    /// (real data) or `--five_base_umi_len` for the UMI key. EXPERIMENTAL: not gated.
    /// Requires `--illumina_5base`.
    #[arg(long = "five_base_duplex")]
    pub five_base_duplex: bool,
    /// `[#787 EXPERIMENTAL/PREVIEW]` COLLAPSE each duplex family to a consensus in
    /// `<out>.5base_consensus.bam` (DRAGEN-style duplex consensus). Implies
    /// `--five_base_duplex` (SE + PE; PE is the real workflow). Reconciled by MOLECULE strand
    /// (the OT molecule owns a `+` CpG, the OB molecule a `-` CpG); the opposite strand is the
    /// variant check (a cytosine gone on BOTH strands is masked to `N`). Emits a forward AND a
    /// reverse record per family, so BOTH strands of every CpG are scored. DRAGEN-validated on
    /// real NA12878 (both strands r ≈ 0.77). EXPERIMENTAL: not gated. Requires `--illumina_5base`.
    #[arg(long = "five_base_consensus")]
    pub five_base_consensus: bool,
    /// `[#787 EXPERIMENTAL/PREVIEW]` Take the duplex UMI from the READ NAME instead of
    /// inline read bases. Real Illumina 5-Base data carries a DUAL UMI as the tail
    /// `:`-field of the qname written `A+B` (e.g. `...:1070:ANCGTTG+NGGTGTA`), with the
    /// duplex partner's halves swapped (`B+A`); canonicalizing collapses the swap into one
    /// family key. Use this instead of `--five_base_umi_len` for real data (mutually
    /// exclusive). EXPERIMENTAL: not gated. Requires `--illumina_5base`.
    #[arg(long = "five_base_umi_qname")]
    pub five_base_umi_qname: bool,
    /// `[#787 EXPERIMENTAL/PREVIEW]` Run ONLY the duplex-consensus collapse over one or more
    /// EXISTING 5-Base `_pe.bam`/`.bam` files (repeat the flag per file) — no re-alignment.
    /// Families pair ACROSS all the given BAMs, so passing every lane's BAM yields a
    /// full-depth consensus. Writes `<output_dir>/five_base_consensus.bam`. Requires
    /// `--illumina_5base` + `--genome`; honours `--five_base_umi_qname`. When set, the normal
    /// align pipeline is skipped.
    #[arg(long = "five_base_consensus_from_bam", value_name = "BAM")]
    pub five_base_consensus_from_bam: Vec<std::path::PathBuf>,
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
    /// CT+GA index (`Bisulfite_Genome/Combined/BS_combined`, built by
    /// `bismark_genome_preparation --combined_genome`) in one both-strands pass per
    /// read-conversion instead of separate per-strand instances, recovering strand
    /// from the RNAME suffix × FLAG. Concordance-gated, NOT byte-identical to the
    /// faithful default (a small benign churn). Supports single-end + paired-end,
    /// Bowtie 2 + HISAT2, directional / non-directional / pbat. minimap2-combined and
    /// `--multicore` + combined are not supported (fail loud).
    #[arg(long = "combined_index")]
    pub combined_index: bool,

    /// EXPERIMENTAL (v2, opt-in, never-silent): the single-pass "model (b)"
    /// execution model for `--combined_index --non_directional`. Aligns ONE
    /// Bowtie 2 pass over conversion-tagged interleaved reads (one combined index
    /// load instead of two — lower peak RSS) instead of model (a)'s two parallel
    /// passes. Requires `--combined_index --non_directional`; single-end or
    /// paired-end; **Bowtie 2 only** (the qname-tag mechanism is Bowtie-2-specific).
    /// NOT byte-identical AND NOT decision-equivalent to model (a): the qname tag
    /// perturbs Bowtie 2's read-name-seeded RNG, so a tiny fraction of co-optimal
    /// reads get a different (validated-equally-accurate) alignment. Ground-truth
    /// validated against Sherman; never the default.
    #[arg(long = "combined_index_single_pass")]
    pub combined_index_single_pass: bool,

    /// EXPERIMENTAL (v2, opt-in, never-silent): the SEQUENTIAL low-memory
    /// execution model for `--combined_index --non_directional`. Runs model (a)'s
    /// two both-strands passes ONE AT A TIME (pass 1's aligner exits, freeing the
    /// index, before pass 2 starts) instead of concurrently — one combined index
    /// resident at a time (~half the peak RSS). BYTE-IDENTICAL to the default
    /// parallel path (the aligner's output is independent of when each pass runs);
    /// the trade is wall time (the passes no longer overlap). Requires
    /// `--combined_index --non_directional`; single-end or paired-end; Bowtie 2 or
    /// HISAT2; mutually exclusive with `--combined_index_single_pass`.
    #[arg(long = "combined_index_sequential")]
    pub combined_index_sequential: bool,

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
    /// Local-alignment mode (soft-clipped ends). Bowtie 2 (`--local` + `--score-min G,20,8`)
    /// and HISAT2 (drops `--no-softclip` + L-form `--score-min L,0,-0.2`, no `--local` flag).
    /// minimap2 rejects it (local by design); `--combined_index` rejects it too.
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
    /// Write BAM output — the DEFAULT. Accepted as a no-op for Bismark-CLI /
    /// pipeline compatibility (Perl `bismark` has `--bam`; nf-core/methylseq's
    /// `BISMARK_ALIGN` passes it). BAM is already the default here, so this just
    /// makes the flag accepted rather than rejected; `--sam`/`--cram` (below)
    /// still select those formats if given.
    #[arg(long)]
    pub bam: bool,
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

    // ---- cell barcode / UMI tags (SeekSoul-style single-cell input) -------
    /// Write the `CB:Z:` tag (cell barcode = field 0 of a
    /// `<barcode>_<umi>[_<alt>]_<name>` QNAME).
    #[arg(long = "add_barcode")]
    pub add_barcode: bool,
    /// Write the `UR:Z:` tag (raw UMI = field 1 of a
    /// `<barcode>_<umi>[_<alt>]_<name>` QNAME).
    #[arg(long = "add_umi")]
    pub add_umi: bool,

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
