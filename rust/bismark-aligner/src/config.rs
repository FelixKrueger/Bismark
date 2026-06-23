//! Resolved run configuration — the typed seam (`RunConfig`) that Phase 1
//! produces and Phases 2–10 consume.
//!
//! [`resolve`] mirrors Perl `process_command_line`'s precedence: aligner →
//! library type → format → genome/reads → discovery → aligner detection →
//! `aligner_options` → output target. Only the **v1 spine** is wired; HISAT2/
//! minimap2 and SAM/CRAM output fail loudly (deferred), while non-directional/
//! pbat/PE/FastA resolve (no alignment runs in Phase 1).

use std::path::{Path, PathBuf};

use crate::aligner::{self, DetectedAligner};
use crate::cli::Cli;
use crate::discovery::{self, GenomeIndexes};
use crate::error::{AlignerError, Result};
use crate::options;

/// Which external aligner. v1 ships Bowtie 2; the v1.x epic adds HISAT2 and
/// minimap2 (this phase — SE only; see [`resolve`] for the PE-minimap2 reject).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aligner {
    /// Bowtie 2 (default).
    Bowtie2,
    /// HISAT2 (v1.x; thin wrapper — same instance/strand model as Bowtie 2).
    Hisat2,
    /// minimap2 (v1.x; pure wrapper — clean-slate options + positional `.mmi`).
    /// Single-end only; the merge/MAPQ/XM core is reused unchanged (the within-
    /// instance second-best `s2:i:` is IGNORED by Bismark → `second_best=None`).
    Minimap2,
    /// rammap (v2; concordance-gated) — the pure-Rust minimap2 reimplementation
    /// (`jwanglab/rammap`), spawned as an external binary exactly like minimap2.
    /// "minimap-like": identical SAM tag set (incl. the ignored `s2:i:`), single
    /// `.mmi`, clean-slate `map-ont` options, SE-only, same `--local`/PE/
    /// `--combined_index` rejects. Opt-in `--rammap`, never-silent — NOT
    /// byte-identical to minimap2 (validated by concordance, not the gate).
    Rammap,
}

impl Aligner {
    /// The output-name token in `_bismark_<token>.bam` / `_<token>_SE_report.txt`
    /// (Perl `_bismark_bt2` / `_bismark_hisat2` / `_bismark_mm2`). Threaded ONLY
    /// into the derived-name path, never `--basename`/`_unmapped`/`_ambiguous`.
    pub fn token(self) -> &'static str {
        match self {
            Aligner::Bowtie2 => "bt2",
            Aligner::Hisat2 => "hisat2",
            Aligner::Minimap2 => "mm2",
            // Design#7: the full word `rammap` (NOT abbreviated like `mm2`) →
            // `_bismark_rammap.bam` / `_rammap_SE_report.txt`.
            Aligner::Rammap => "rammap",
        }
    }

    /// The human-readable name for the report "Bismark was run with …" line
    /// (Perl 1722/1725/1728) and detection diagnostics. **`minimap2` is lowercase**
    /// (Perl `elsif ($mm2)` 1725) — byte-significant in the SE report header.
    pub fn name(self) -> &'static str {
        match self {
            Aligner::Bowtie2 => "Bowtie 2",
            Aligner::Hisat2 => "HISAT2",
            Aligner::Minimap2 => "minimap2",
            // The report "Bismark was run with rammap against …" line.
            Aligner::Rammap => "rammap",
        }
    }
}

/// Bisulfite library type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryType {
    /// Directional (default) — 2 instances for SE.
    Directional,
    /// Non-directional — 4 instances.
    NonDirectional,
    /// PBAT.
    Pbat,
}

/// Input read format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadFormat {
    /// FASTQ (default).
    FastQ,
    /// FASTA.
    FastA,
}

/// Output container (v1 wires BAM only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// BAM (default).
    Bam,
}

/// Read layout + resolved file list.
#[derive(Debug, Clone)]
pub enum ReadLayout {
    /// Single-end reads.
    SingleEnd {
        /// One or more SE read files.
        reads: Vec<String>,
    },
    /// Paired-end reads (1:1 mate files).
    PairedEnd {
        /// Read-1 mate files.
        mates1: Vec<String>,
        /// Read-2 mate files.
        mates2: Vec<String>,
    },
}

impl ReadLayout {
    /// `true` for paired-end.
    pub fn is_paired(&self) -> bool {
        matches!(self, ReadLayout::PairedEnd { .. })
    }
}

/// Read/reference gap penalties (Bowtie 2 `--rdg`/`--rfg`; 5,3 defaults). Held
/// for the later MAPQ computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GapPenalties {
    /// Read-gap open (deletion in read).
    pub deletion_open: u32,
    /// Read-gap extend.
    pub deletion_extend: u32,
    /// Reference-gap open (insertion in read).
    pub insertion_open: u32,
    /// Reference-gap extend.
    pub insertion_extend: u32,
}

/// Resolved output target.
#[derive(Debug, Clone)]
pub struct OutputTarget {
    /// Output directory; empty = current directory (Perl default `''`).
    pub output_dir: PathBuf,
    /// Temp directory; empty = parent/CWD (Perl default `''`, independent of `output_dir`).
    pub temp_dir: PathBuf,
    /// `--basename` (fully overrides the derived output name).
    pub basename: Option<String>,
    /// `--prefix` prepended to the output name.
    pub prefix: Option<String>,
    /// Container format (BAM in v1).
    pub format: OutputFormat,
    /// Gzip text output.
    pub gzip: bool,
}

/// Read-processing options that shape the converted output (Phase 2+). Only the
/// fields not already on [`OutputTarget`] live here; `gzip`/`prefix`/`temp_dir`
/// are read from `output` (single source of truth).
#[derive(Debug, Clone)]
pub struct ReadProcessing {
    /// Skip the first N reads (`--skip`; `0`/None disables, Perl falsy).
    pub skip: Option<u64>,
    /// Stop after read N (`--upto`; `0`/None disables, Perl falsy).
    pub upto: Option<u64>,
    /// `--icpc`: truncate read IDs at the first space/tab (else underscore them).
    pub icpc: bool,
    /// minimap2-only maximum read length (`--mm2_maximum_length`); inert for Bowtie 2.
    pub maximum_length_cutoff: Option<u32>,
}

/// The fully-resolved configuration (the seam consumed by later phases).
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// Verbatim argv (program name excluded) for the `@PG` `CL:` line (Phase 5).
    pub command_line: String,
    /// Selected aligner (Bowtie 2, HISAT2, or minimap2).
    pub aligner: Aligner,
    /// `[v2/experimental]` `--rammap_inprocess`: OPT INTO the in-process `rammap-core`
    /// backend. `--rammap` defaults to the subprocess path (Phase-4, Option A); this flag
    /// selects the in-process path instead — lower RAM, slower (single-threaded),
    /// concordant-NOT-byte-identical. Effective only on a `--features rammap-inprocess`
    /// build (inert otherwise — the in-process path isn't compiled). Read on BOTH builds
    /// by `lib::use_se_inprocess_rammap` (always-compiled) so it is never a feature-off
    /// dead field. `resolve` requires `--rammap` whenever it is set.
    pub rammap_inprocess: bool,
    /// Illumina 5-Base (5mC->T) mode (#787): align to the UNCONVERTED genome and
    /// call methylation with inverted polarity. v1 = single-end + directional,
    /// minimap2 backend. Opt-in, never-silent, concordance-gated (no Perl oracle).
    pub five_base: bool,
    /// #787: run the post-alignment variant/methylation deconvolution pass + report.
    /// Requires `five_base` (guarded at resolve()).
    pub five_base_deconvolution: bool,
    /// #787: basename of the NORMAL (unconverted) bowtie2/hisat2 index for a 5-Base
    /// `--bowtie2`/`--hisat2` run (`None` ⇒ minimap2-on-FASTA default).
    pub five_base_index: Option<PathBuf>,
    /// Library type.
    pub library: LibraryType,
    /// Read layout + files.
    pub layout: ReadLayout,
    /// Input format.
    pub format: ReadFormat,
    /// Discovered genome indexes + FASTA inventory.
    pub genome: GenomeIndexes,
    /// Detected aligner binary + version (Bowtie 2, HISAT2, or minimap2).
    pub detected_aligner: DetectedAligner,
    /// Exact aligner option string (per-instance `--norc`/`--nofw` added later).
    pub aligner_options: String,
    /// Gap penalties (for later MAPQ).
    pub gap_penalties: GapPenalties,
    /// `--score_min` intercept (default `0.0`) — for `calc_mapq`.
    pub score_min_intercept: f64,
    /// `--score_min` slope (default `-0.2`) — for `calc_mapq`.
    pub score_min_slope: f64,
    /// `--local` mode (= `cli.local`): `calc_mapq` uses `ln(readLen)` scMin + the local
    /// MAPQ ladder. The `--score_min` defaults are aligner-dependent: Bowtie 2-local =
    /// `(20.0, 8.0)` (G-form); HISAT2-local = `(0.0, -0.2)` (L-form) — see `score_min_params`.
    pub score_min_local: bool,
    /// Perl's `$dovetail` variable (8047): `!--no_dovetail`, set for **every**
    /// aligner (the `if($bowtie2)` at 8051 only gates whether `--dovetail` is
    /// pushed to the *aligner options*, NOT this variable). Consumed by the PE
    /// TLEN sign computation (`output.rs`), where it must be aligner-INDEPENDENT —
    /// HISAT2 suppresses the `--dovetail` flag from `aligner_options` but still
    /// uses `$dovetail=1` for TLEN. Default `true`.
    pub dovetail: bool,
    /// `--phred64-quals`: input qualities are Phred+64; converted to Phred+33 on
    /// SAM output (Perl 4191). Default `false` (Phred+33). Phase 5.
    pub phred64: bool,
    /// `--unmapped`: write reads with no alignment to `<name>_unmapped_reads.fq.gz`. Phase 6.
    pub unmapped: bool,
    /// `--ambiguous`: write ambiguously-mapping reads to `<name>_ambiguous_reads.fq.gz`. Phase 6.
    pub ambiguous: bool,
    /// `--ambig_bam`: write the first ambiguous alignment to
    /// `<name>_bismark_<aligner>.ambig.bam`. Phase 6. For HISAT2 `--multicore N` (the
    /// `-p N` remap) this stays the single-instance path, so `--ambig_bam` works.
    pub ambig_bam: bool,
    /// Output target.
    pub output: OutputTarget,
    /// Read-processing options (skip/upto/icpc/max-len).
    pub read_processing: ReadProcessing,
    /// `--multicore`/`--parallel`: file-level worker count (Phase 9b). Resolved to
    /// `cli.multicore.unwrap_or(1)`; `1` = the single-core direct path, `>1` = the
    /// order-preserving contiguous-chunk fan-out. `validate_multicore` guarantees ≥ 1.
    /// For HISAT2 this is forced to `1` when `--multicore N` is remapped to `-p N`
    /// (see `hisat2_multicore_remap`) — the fork model is not faithful for HISAT2.
    pub multicore: u32,
    /// HISAT2 Approach B-faithful (`--hisat2 --multicore N`): when set, `--multicore N`
    /// was interpreted as a single HISAT2 instance with `-p N --reorder` (NOT the fork
    /// model — HISAT2 splice discovery is not chunk-invariant; the `-p N` threading is
    /// deterministic per N and byte-identical to Perl `--hisat2 -p N`). `Some(N)` here
    /// means the remap fired: `multicore` is `1`, `aligner_options` carries `-p N
    /// --reorder`, and the run emits a never-silent notice. `None` for every other case.
    pub hisat2_multicore_remap: Option<u32>,
    /// `--combined_index` (v2, opt-in, never-silent, concordance-gated): align
    /// against the single combined CT+GA index in one both-strands pass instead of
    /// the faithful per-strand instances. SE directional only this phase; the
    /// `reject_combined_index_unsupported` guard rejects every other combination,
    /// and `resolve` errors if the combined index is absent (it is `genome.
    /// combined_index_basename.is_some()` whenever this is `true`).
    pub combined_index: bool,
    /// `--combined_index_single_pass` (v2, opt-in): the single-pass "model (b)" exec
    /// model for `--combined_index --non_directional` — one Bowtie 2 pass over
    /// conversion-tagged interleaved reads (one index load instead of two). NOT
    /// byte-identical AND NOT decision-equivalent to model (a) (the qname tag
    /// perturbs Bowtie 2's read-name RNG); ground-truth-validated. The
    /// `reject_combined_index_unsupported` guard requires `combined_index &&
    /// non_directional` (SE Bowtie 2, single-core) whenever this is `true`.
    pub combined_index_single_pass: bool,
    /// `--combined_index_sequential` (v2, opt-in): the SEQUENTIAL low-memory exec
    /// model for `--combined_index --non_directional` — model (a)'s two both-strands
    /// passes run ONE AT A TIME (pass 1's Bowtie 2 exits, freeing the index, before
    /// pass 2 spawns), so only one combined index is resident at a time (~half the
    /// peak RSS). **BYTE-IDENTICAL** to the default parallel model (a) (each pass sees
    /// the same untagged converted file + index regardless of when it runs); the trade
    /// is wall time. The `reject_combined_index_unsupported` guard requires
    /// `combined_index && non_directional` (SE Bowtie 2, single-core) and rejects it
    /// together with `combined_index_single_pass` (competing exec models).
    pub combined_index_sequential: bool,
}

/// HISAT2 Approach B-faithful: for HISAT2, `--multicore N` (N > 1) is interpreted as a
/// single instance with `-p N` intra-instance threading (NOT the fork+chunk model — see
/// `resolve`). Returns the `-p` thread count to inject (`Some(N)`), or `None` for every
/// other aligner and for single-core. Pure (no I/O) so it is unit-testable fixture-free.
fn hisat2_multicore_threads(aligner: Aligner, cli_multicore: Option<u32>) -> Option<u32> {
    if aligner == Aligner::Hisat2 && cli_multicore.unwrap_or(1) > 1 {
        cli_multicore
    } else {
        None
    }
}

/// Resolve a parsed [`Cli`] + the verbatim command line into a [`RunConfig`].
pub fn resolve(cli: &Cli, command_line: String) -> Result<RunConfig> {
    let aligner = resolve_aligner(cli)?;
    // `--rammap_inprocess` (opt into the in-process rammap backend) is meaningful only
    // with `--rammap` — fail loud otherwise (never-silent; mirrors the rammap conflict
    // dies in `resolve_aligner`).
    if cli.rammap_inprocess && !cli.rammap {
        return Err(AlignerError::Validation(
            "--rammap_inprocess requires --rammap: it selects the in-process rammap backend, \
             which only applies to a --rammap run."
                .into(),
        ));
    }
    // #787: deconvolution is a post-pass over a 5-Base BAM — meaningless without it.
    if cli.five_base_deconvolution && !cli.illumina_5base {
        return Err(AlignerError::Validation(
            "--five_base_deconvolution requires --illumina_5base: it deconvolutes variant vs \
             methylation over a 5-Base run's output."
                .into(),
        ));
    }
    // The minimap2-only preset/length flags (Perl 8329-8356): outside minimap2
    // mode every `--mm2_*` flag dies (the `unless($mm2)` block); in minimap2 mode
    // `--mm2_maximum_length` is range-checked + defaults to 10000. Returns the
    // resolved cutoff to thread into `read_processing` (None for non-minimap2).
    let maximum_length_cutoff = resolve_mm2_max_length(cli, aligner)?;
    let library = resolve_library(cli)?;
    let format = resolve_format(cli)?;
    validate_multicore(cli)?;
    // HISAT2 + `--multicore N` (Approach B-faithful, plan 06132026_aligner-hisat2-multicore).
    // The fork+chunk model is NOT faithful for HISAT2 — splice-site discovery is not
    // chunk-invariant (Perl itself is not worker-invariant: single-core 1310 spliced vs
    // `--multicore 8` 1219 on the 1M oxy subset). The Phase-0 spike showed a SINGLE HISAT2
    // instance with `-p N` is deterministic per N (byte-identical run-to-run) though NOT
    // equal to single-core (records 844,267→844,316, spliced 1310→1298 as N grows — HISAT2
    // accumulates a splice-site DB in thread order). So `--hisat2 --multicore N` is
    // interpreted as ONE instance with `-p N --reorder` (Perl's `-p` mode, `bismark:7998-7999`):
    // deterministic and byte-identical to Perl `--hisat2 -p N`, lower memory than N forks.
    // This is a semantic remap (`--multicore`→`-p`, HISAT2 only) — announced loudly by the
    // run (`lib.rs`). Bowtie 2 `--multicore` (the fork path, Phase 9b) and single-core
    // HISAT2 are unaffected. `--ambig_bam` works under B (one instance, the single-instance
    // path — Perl's Bowtie-2-only multicore ambig temp machinery is never reached).
    let hisat2_multicore_remap = hisat2_multicore_threads(aligner, cli.multicore);
    // An explicit `-p M` AND the remapped `--multicore N` both set is ambiguous (both
    // would drive HISAT2's `-p`) → fail loud rather than silently pick one.
    if hisat2_multicore_remap.is_some() && cli.bowtie_threads.is_some() {
        return Err(AlignerError::Validation(
            "--hisat2 with both --multicore N and -p M is ambiguous: for HISAT2, --multicore is \
             interpreted as `-p` intra-instance threading, so it conflicts with an explicit -p. \
             Pass only one."
                .into(),
        ));
    }

    // `--local` scope. Bowtie 2 AND HISAT2 support `--local` (faithful to Perl v0.25.1):
    // Bowtie 2 pushes `--local` + G-form `--score-min G,20,8`; HISAT2-local instead drops
    // `--no-softclip` (allows soft-clipping) + emits L-form `--score-min L,0,-0.2` and does
    // NOT push `--local` (Perl 7904 "does not work with HISAT2" / 8309-8311 / 7946-7948).
    // minimap2 is REJECTED: it performs local (soft-clipping) alignment **by design** —
    // there is no end-to-end vs local distinction to toggle. Combined-index = a v2 mode.
    if cli.local {
        // minimap2 AND rammap (minimap-like) are both rejected: both perform local
        // (soft-clipping) alignment by design. The error names the actual engine
        // (`aligner.name()`) so a `--rammap` run reads "--rammap", not "--minimap2".
        if matches!(aligner, Aligner::Minimap2 | Aligner::Rammap) {
            return Err(AlignerError::Unsupported(format!(
                "--local is not supported with --{0}: {0} performs local \
                 (soft-clipping) alignment by design — there is no end-to-end vs local \
                 distinction to toggle. Use --bowtie2 or --hisat2 for --local.",
                aligner.name()
            )));
        }
        if cli.combined_index || cli.combined_index_sequential || cli.combined_index_single_pass {
            return Err(AlignerError::Unsupported(
                "--local is not supported with --combined_index (a separate alignment model); \
                 run --local against the faithful per-strand index."
                    .into(),
            ));
        }
    }

    let (genome_arg, reads_positional) = resolve_genome_and_positional(cli)?;
    let layout = resolve_layout(cli, &reads_positional)?;

    // --illumina_5base (#787) v1 scope guards. The 5-Base path is single-end +
    // directional, FASTQ, single-instance only; everything else is a deferred
    // follow-up phase. Reject loudly (never silently degrade) BEFORE the generic
    // minimap2 guards so the error names --illumina_5base, not --minimap2.
    if cli.illumina_5base {
        if cli.non_directional || cli.pbat {
            return Err(AlignerError::Unsupported(
                "--illumina_5base is directional only in v1 (drop --non_directional/--pbat): \
                 the 5-Base library is directional."
                    .into(),
            ));
        }
        if cli.slam {
            return Err(AlignerError::Unsupported(
                "--illumina_5base is not supported with --slam.".into(),
            ));
        }
        if cli.fasta {
            return Err(AlignerError::Unsupported(
                "--illumina_5base requires FASTQ input in v1 (drop --fasta).".into(),
            ));
        }
        if cli.multicore.unwrap_or(1) > 1 {
            return Err(AlignerError::Unsupported(
                "--illumina_5base does not support --multicore in v1 (single instance only)."
                    .into(),
            ));
        }
        if cli.combined_index || cli.combined_index_sequential || cli.combined_index_single_pass {
            return Err(AlignerError::Unsupported(
                "--illumina_5base is not supported with --combined_index (a separate bisulfite \
                 alignment model)."
                    .into(),
            ));
        }
    }

    // PE-minimap2 is NOT byte-identity-reachable and is deferred out of v1.x
    // (Felix decision 2026-06-05): the Perl minimap2 paired-end path
    // (`paired_end_…_minimap2` 6697-6708) is unfinished WIP (`# TODO` +
    // `warn`+`sleep(1)` twice per read pair) AND the PE report writer (1845-1850)
    // has no `$mm2` branch, so it mislabels minimap2 PE as "HISAT2" — there is no
    // trustworthy oracle to byte-match. Fail loudly (Bowtie 2 + HISAT2 cover PE).
    // minimap2 AND rammap (minimap-like) are SE-only. The error names the actual
    // engine (`aligner.name()`) so a `--rammap` run reads "--rammap".
    // 5-Base PE (#787) is its own path (run_pe_five_base) — unconverted minimap2 PE,
    // concordance-gated, NOT the (rejected) bisulfite minimap2 PE — so it is exempt.
    if matches!(aligner, Aligner::Minimap2 | Aligner::Rammap)
        && layout.is_paired()
        && !cli.illumina_5base
    {
        return Err(AlignerError::Unsupported(format!(
            "paired-end alignment with --{0} is not supported: the Perl Bismark minimap2 \
             paired-end path is unfinished/experimental and has no trustworthy byte-identity \
             reference. Use --{0} for single-end reads, or --bowtie2/--hisat2 for paired-end.",
            aligner.name()
        )));
    }

    // --combined_index (v2) scope guard: SE (directional OR non-directional)
    // Bowtie 2 only. Reject every not-yet-supported combination loudly (never
    // silently fall back to the faithful path — that would mask which strands the
    // combined search omits). PLAN §3.1 + phase 5.
    reject_combined_index_unsupported(cli, aligner, library, &layout)?;

    let genome = discovery::discover_genome(aligner, &genome_arg)?;
    // --combined_index requires the combined index to be present (built by
    // `bismark_genome_preparation --combined_genome`); fail loudly if absent.
    if cli.combined_index && genome.combined_index_basename.is_none() {
        return Err(AlignerError::Validation(format!(
            "--combined_index was requested but no combined index was found at \
             {}/Bisulfite_Genome/Combined/BS_combined.* — build it with \
             `bismark_genome_preparation --combined_genome <genome>`.",
            genome.genome_dir.display()
        )));
    }
    let path_to_aligner = match aligner {
        Aligner::Bowtie2 => cli.path_to_bowtie2.as_deref(),
        Aligner::Hisat2 => cli.path_to_hisat2.as_deref(),
        Aligner::Minimap2 => cli.path_to_minimap2.as_deref(),
        Aligner::Rammap => cli.path_to_rammap.as_deref(),
    };
    let detected_aligner = aligner::detect_aligner(aligner, path_to_aligner)?;
    let (aligner_options, gap_penalties) = options::build_aligner_options(
        cli,
        aligner,
        format,
        layout.is_paired(),
        hisat2_multicore_remap,
    )?;
    let (score_min_intercept, score_min_slope) = options::score_min_params(cli, aligner)?;
    let score_min_local = cli.local; // --local: ln() scMin + the local MAPQ ladder
    reject_unsupported_output_flags(cli)?;
    let output = resolve_output(cli)?;
    let read_processing = ReadProcessing {
        skip: cli.skip,
        upto: cli.upto,
        icpc: cli.icpc,
        // Resolved above (minimap2: Some(value-or-default-10000); else None).
        maximum_length_cutoff,
    };

    Ok(RunConfig {
        command_line,
        aligner,
        // Phase 4 (epic 06152026): in-process opt-in flag (guarded above: requires --rammap).
        rammap_inprocess: cli.rammap_inprocess,
        // #787 5-Base mode (guarded above: SE + directional, minimap2 unconverted path).
        five_base: cli.illumina_5base,
        five_base_deconvolution: cli.five_base_deconvolution,
        five_base_index: cli.five_base_index.clone(),
        library,
        layout,
        format,
        genome,
        detected_aligner,
        aligner_options,
        gap_penalties,
        score_min_intercept,
        score_min_slope,
        score_min_local,
        // Perl 8047: `$dovetail = 1 unless $no_dovetail` — independent of the aligner.
        dovetail: !cli.no_dovetail,
        phred64: cli.phred64,
        unmapped: cli.unmapped,
        ambiguous: cli.ambiguous,
        ambig_bam: cli.ambig_bam,
        output,
        read_processing,
        // Phase 9b: file-level worker count. `validate_multicore` (above) already
        // rejected `0`; absent flag = single-core (1). For HISAT2 the `--multicore N`
        // remap routes to ONE instance with `-p N` (the threading lives in
        // `aligner_options`), so force single-core dispatch (`lib.rs` takes the
        // `run_se`/`run_pe` direct path, NOT `parallel::run_*_multicore`).
        multicore: if hisat2_multicore_remap.is_some() {
            1
        } else {
            cli.multicore.unwrap_or(1)
        },
        hisat2_multicore_remap,
        // v2 combined-index mode (guarded above; the combined index is present).
        combined_index: cli.combined_index,
        // v2 model-(b) single-pass tagged exec model (guarded above: requires
        // combined_index && non_directional, SE Bowtie 2, single-core).
        combined_index_single_pass: cli.combined_index_single_pass,
        // v2 sequential low-memory exec model (guarded above: requires
        // combined_index && non_directional, SE Bowtie 2, single-core; mutually
        // exclusive with combined_index_single_pass).
        combined_index_sequential: cli.combined_index_sequential,
    })
}

/// `--combined_index` (v2) scope guard (PLAN §3.1 + phases 5–7). The combined-index
/// path is **single-end Bowtie 2 only, for all three library types** (directional,
/// non-directional, pbat); every other combination (PE, `--multicore`, non-Bowtie2)
/// is rejected loudly so the run never silently falls back to a path that omits the
/// strands the combined search would have covered. Each rejection names the
/// conflicting flag.
fn reject_combined_index_unsupported(
    cli: &Cli,
    aligner: Aligner,
    library: LibraryType,
    layout: &ReadLayout,
) -> Result<()> {
    // --combined_index_single_pass (model (b), the single-pass tagged exec model) is
    // ONLY meaningful for the non-directional combined path (the sole mode that
    // loads the combined index twice). Require --combined_index --non_directional;
    // checked BEFORE the !combined_index early return so `--combined_index_single_pass`
    // alone is also rejected loudly. Single-core follows from --combined_index (which
    // rejects --multicore below); v2.x Phase 6 lifted this to PE non-dir too (SE + PE
    // both valid). Bowtie 2 is required by the explicit guard below (v2.x lifted
    // --combined_index itself to HISAT2, but this exec model stays Bowtie-2-specific).
    if cli.combined_index_single_pass {
        if !cli.combined_index {
            return Err(AlignerError::Unsupported(
                "--combined_index_single_pass requires --combined_index: it is the single-pass \
                 execution model for the non-directional combined-index path, not a standalone \
                 mode."
                    .into(),
            ));
        }
        if library != LibraryType::NonDirectional {
            return Err(AlignerError::Unsupported(
                "--combined_index_single_pass is the single-pass NON-DIRECTIONAL execution model \
                 (model b); it requires --non_directional. Directional and pbat combined-index \
                 are already single-pass (one index load), so the tagged model offers no benefit \
                 there. Drop --combined_index_single_pass."
                    .into(),
            ));
        }
        // v2.x Phase 6: PE non-directional combined now supports this low-RAM exec model
        // (the `layout.is_paired()` reject that used to live here is lifted — both SE and
        // PE non-dir single-pass are valid targets). It stays Bowtie 2-only (next guard);
        // PE-HISAT2 + this flag is therefore rejected by the aligner guard below.
        if aligner != Aligner::Bowtie2 {
            return Err(AlignerError::Unsupported(
                "--combined_index_single_pass requires Bowtie 2: it is the Bowtie-2-specific \
                 single-pass tagged execution model. HISAT2 non-directional combined-index uses \
                 the default parallel model (a) — drop --combined_index_single_pass."
                    .into(),
            ));
        }
    }
    // --combined_index_sequential (the faithful sequential low-memory exec model) is,
    // like model (b), ONLY meaningful for the non-directional combined path (the sole
    // mode that loads the combined index twice) — but unlike model (b) it is
    // BYTE-IDENTICAL to the default parallel model (a) (a pure RSS/wall trade). It is
    // mutually exclusive with --combined_index_single_pass (competing exec models for
    // the same mode). Checked BEFORE the !combined_index early return so the flag alone
    // is also rejected loudly. Single-core follows from --combined_index; v2.x Phase 6
    // lifted this to PE non-dir too (SE + PE both valid), and v2.x Phase 7 lifted it to
    // HISAT2 (the sequential model is faithful + aligner-agnostic — it just runs the two
    // passes serially). Bowtie 2 OR HISAT2 (the explicit guard below); minimap2 stays
    // rejected. (--combined_index_single_pass, model (b), stays Bowtie-2-specific — its
    // qname-tag RNG mechanism is Bowtie-2-only.)
    if cli.combined_index_sequential {
        if cli.combined_index_single_pass {
            return Err(AlignerError::Unsupported(
                "--combined_index_sequential and --combined_index_single_pass are competing \
                 execution models for --combined_index --non_directional (faithful sequential vs \
                 single-pass tagged); choose at most one."
                    .into(),
            ));
        }
        if !cli.combined_index {
            return Err(AlignerError::Unsupported(
                "--combined_index_sequential requires --combined_index: it is the sequential \
                 low-memory execution model for the non-directional combined-index path, not a \
                 standalone mode."
                    .into(),
            ));
        }
        if library != LibraryType::NonDirectional {
            return Err(AlignerError::Unsupported(
                "--combined_index_sequential is the sequential NON-DIRECTIONAL execution model; it \
                 requires --non_directional. Directional and pbat combined-index are already \
                 single-pass (one index load), so the sequential model offers no benefit there. \
                 Drop --combined_index_sequential."
                    .into(),
            ));
        }
        // v2.x Phase 6: PE non-directional combined supports this faithful sequential
        // low-RAM exec model (the `layout.is_paired()` reject is lifted — SE + PE both
        // valid). v2.x Phase 7: HISAT2 supports it too (the sequential model is faithful +
        // aligner-agnostic — it spawns whichever `config.aligner` resolves and just runs
        // the two passes serially). minimap2 stays rejected (next guard + the global
        // minimap2 reject below). (--combined_index_single_pass stays Bowtie-2-only — its
        // own guard above; the tag-RNG mechanism is Bowtie-2-specific.)
        if !matches!(aligner, Aligner::Bowtie2 | Aligner::Hisat2) {
            return Err(AlignerError::Unsupported(format!(
                "--combined_index_sequential requires Bowtie 2 or HISAT2: it is the faithful \
                 sequential low-memory execution model for the non-directional combined-index path. \
                 {0} combined-index is not supported — drop --combined_index_sequential.",
                aligner.name()
            )));
        }
    }
    if !cli.combined_index {
        return Ok(());
    }
    // minimap2 AND rammap (minimap-like) are both rejected: a single both-strands
    // minimap-family pass cannot reproduce Bismark's per-strand model. The error
    // names the actual engine (`aligner.name()`) so a `--rammap` run reads "--rammap".
    if matches!(aligner, Aligner::Minimap2 | Aligner::Rammap) {
        return Err(AlignerError::Unsupported(format!(
            "--combined_index is not supported with --{0}: a single both-strands {0} \
             pass cannot reproduce Bismark's per-strand model, and {0} paired-end has no \
             trustworthy oracle. Use Bowtie 2 or HISAT2 (both build a combined index via \
             `bismark_genome_preparation --combined_genome`).",
            aligner.name()
        )));
    }
    // v2.x: paired-end combined-index is lifted for **Bowtie 2 AND HISAT2** across ALL
    // THREE library types — **directional** (Phase 2, one both-strands C->T pass -> OT/OB),
    // **non-directional** (Phase 3, two both-strands passes C->T + G->A -> 4 strands,
    // parallel model (a)), and **pbat** (Phase 4, one both-strands G->A pass -> CTOT/CTOB,
    // the non-dir G->A half standalone). HISAT2 PE combined (Phase 5) reuses the identical
    // PE machinery — `PairedAlignerStream::spawn` runs whichever `detected_aligner` binary
    // is resolved, and the per-pair classify/select/route is aligner-agnostic. The
    // `matches!(aligner, Bowtie2 | Hisat2)` conjunct is load-bearing, NOT redundant: it
    // keeps **minimap2** PE combined rejected (the Minimap2-only reject above only covers
    // SE fall-through; a single both-strands minimap2 pass cannot do the 4-strand dispatch,
    // and PE-minimap2 has no trustworthy oracle). minimap2 PE is also double-rejected at the
    // global `aligner == Minimap2 && layout.is_paired()` guard above.
    if layout.is_paired()
        && !(matches!(aligner, Aligner::Bowtie2 | Aligner::Hisat2)
            && matches!(
                library,
                LibraryType::Directional | LibraryType::NonDirectional | LibraryType::Pbat
            ))
    {
        return Err(AlignerError::Unsupported(
            "paired-end --combined_index is supported with Bowtie 2 or HISAT2 (directional, \
             non-directional, and pbat). It is not supported with minimap2 (a single \
             both-strands minimap2 pass cannot reproduce Bismark's per-strand model, and \
             minimap2 paired-end has no trustworthy oracle). Use Bowtie 2 / HISAT2, single-end \
             reads, or drop --combined_index for the faithful paired-end path."
                .into(),
        ));
    }
    // All three SE library types are supported (Phase 7 added pbat): directional
    // (one C→T pass → OT/OB), non-directional (two passes → OT/OB/CTOT/CTOB union),
    // pbat (one G→A pass → CTOT/CTOB). No library-type rejection — PE / --multicore /
    // non-Bowtie2 are the only combined-index restrictions.
    match library {
        LibraryType::Directional | LibraryType::NonDirectional | LibraryType::Pbat => {}
    }
    if cli.multicore.unwrap_or(1) > 1 {
        return Err(AlignerError::Unsupported(
            "--combined_index is not supported with --multicore/--parallel: forked chunking \
             re-loads the large combined index once per chunk (the inefficient pattern combined \
             mode exists to avoid). Run --combined_index single-core (the default)."
                .into(),
        ));
    }
    Ok(())
}

/// Hard-reject the output-affecting options that are out of the v1 byte-identity
/// scope (Phase 5). These alter the SAM record/tag set or the header, are not
/// covered by the gate, and so must fail loudly rather than silently no-op
/// (rev-1 plan-review finding — fail-loud, not defer).
fn reject_unsupported_output_flags(cli: &Cli) -> Result<()> {
    if cli.slam {
        return Err(AlignerError::Unsupported(
            "--slam (SLAM-seq methylation call) is not yet supported in v1.".into(),
        ));
    }
    if cli.non_bs_mm {
        return Err(AlignerError::Unsupported(
            "--non_bs_mm (extra XA non-bisulfite-mismatch tag) is not yet supported in v1.".into(),
        ));
    }
    if cli.rg_tag {
        return Err(AlignerError::Unsupported(
            "--rg_tag/--rg_id/--rg_sample (read-group @RG/RG:Z) is not yet supported in v1.".into(),
        ));
    }
    if cli.sam_no_hd {
        return Err(AlignerError::Unsupported(
            "--sam-no-hd (omit the SAM header) is not supported in v1 (a header-less BAM is invalid).".into(),
        ));
    }
    Ok(())
}

fn resolve_aligner(cli: &Cli) -> Result<Aligner> {
    if cli.hisat2 && cli.bowtie2 {
        return Err(AlignerError::Validation(
            "You may not select both --hisat2 and --bowtie2. Make your pick! [default is Bowtie 2]"
                .into(),
        ));
    }
    if cli.hisat2 && cli.minimap2 {
        return Err(AlignerError::Validation(
            "You may not select both --hisat2 and --minimap2. Make your pick!".into(),
        ));
    }
    if cli.minimap2 && cli.bowtie2 {
        return Err(AlignerError::Validation(
            "You may not select both --minimap2 and --bowtie2. Make your pick! [default is Bowtie 2]".into(),
        ));
    }
    // rammap conflicts (ordered BEFORE every `Ok` below so the conflict fires, never
    // a silent precedence pick): --rammap is mutually exclusive with the other engines.
    if cli.rammap && cli.bowtie2 {
        return Err(AlignerError::Validation(
            "You may not select both --rammap and --bowtie2. Make your pick! [default is Bowtie 2]"
                .into(),
        ));
    }
    if cli.rammap && cli.hisat2 {
        return Err(AlignerError::Validation(
            "You may not select both --rammap and --hisat2. Make your pick!".into(),
        ));
    }
    if cli.rammap && cli.minimap2 {
        return Err(AlignerError::Validation(
            "You may not select both --rammap and --minimap2. Make your pick!".into(),
        ));
    }
    // --illumina_5base (5-Base, #787) aligns to the UNCONVERTED genome with minimap2
    // (v1). It is mutually exclusive with the bisulfite engines that have no
    // unconverted-index path yet; `--minimap2` may co-occur (5-Base IS a minimap2
    // run). Ordered before the engine `Ok`s so the conflict fires, never a silent pick.
    if cli.illumina_5base {
        if cli.rammap {
            return Err(AlignerError::Validation(
                "--illumina_5base is not supported with --rammap. Use --bowtie2/--hisat2 (with \
                 --five_base_index) or the default minimap2 (genome FASTA)."
                    .into(),
            ));
        }
        // bowtie2/hisat2 5-Base align the RAW reads to a user-provided UNCONVERTED
        // index (5-Base keeps full complexity, so a normal index works). minimap2 (the
        // default) reads the genome FASTA directly and needs no index.
        if cli.bowtie2 || cli.hisat2 {
            if cli.five_base_index.is_none() {
                return Err(AlignerError::Validation(
                    "--illumina_5base with --bowtie2/--hisat2 requires --five_base_index \
                     <basename>: a NORMAL (unconverted) index of the genome, built once with \
                     bowtie2-build/hisat2-build. (Without an engine flag, 5-Base uses minimap2 \
                     against the genome FASTA directly.)"
                        .into(),
                ));
            }
            return Ok(if cli.hisat2 {
                Aligner::Hisat2
            } else {
                Aligner::Bowtie2
            });
        }
        if cli.five_base_index.is_some() {
            return Err(AlignerError::Validation(
                "--five_base_index only applies to --illumina_5base --bowtie2/--hisat2; the \
                 default minimap2 5-Base path reads the genome FASTA directly."
                    .into(),
            ));
        }
        return Ok(Aligner::Minimap2);
    }
    if cli.five_base_index.is_some() {
        return Err(AlignerError::Validation(
            "--five_base_index only applies to --illumina_5base --bowtie2/--hisat2.".into(),
        ));
    }
    if cli.hisat2 {
        return Ok(Aligner::Hisat2);
    }
    if cli.minimap2 {
        return Ok(Aligner::Minimap2);
    }
    if cli.rammap {
        return Ok(Aligner::Rammap);
    }
    Ok(Aligner::Bowtie2)
}

/// Validate the minimap2-only preset/length flags and resolve the maximum-length
/// cutoff (Perl `process_command_line` 8329-8356).
///
/// - **Outside minimap2 mode** (the `unless($mm2)` block, 8329-8341): each of
///   `--mm2_short_reads`, `--mm2_maximum_length`, `--mm2_pacbio`, `--mm2_nanopore`
///   dies — they are only valid with `--minimap2`. Returns `None` (the convert-side
///   length guard stays inert for Bowtie 2 / HISAT2).
/// - **In minimap2 mode** (the `if($mm2)` block, 8344-8356): `--mm2_maximum_length`
///   must be in `100..=100_000` (else die), and defaults to `10000` when absent.
///   Returns `Some(value)`.
///
/// (Preset *selection* + the preset-conflict dies live in `options::minimap2_options`,
/// mirroring Perl's `if($mm2)` option-assembly block 8358-8413.)
fn resolve_mm2_max_length(cli: &Cli, aligner: Aligner) -> Result<Option<u32>> {
    // rammap is minimap-like — it honors the SAME `--mm2_*` knobs + length cutoff
    // (design#3, for apples-to-apples fairness vs `--minimap2`). So the
    // "outside minimap-family mode" die-block applies only when NEITHER is selected.
    if !matches!(aligner, Aligner::Minimap2 | Aligner::Rammap) {
        if cli.mm2_short_read {
            return Err(AlignerError::Validation(
                "You cannot specify minimap2 options (--mm2_short_reads) unless you also use \
                 --minimap2. Please respecify!"
                    .into(),
            ));
        }
        if cli.maximum_length_cutoff.is_some() {
            return Err(AlignerError::Validation(
                "You cannot specify minimap2 options (--mm2_maximum_length) unless you also use \
                 --minimap2. Please respecify!"
                    .into(),
            ));
        }
        if cli.mm2_pacbio {
            return Err(AlignerError::Validation(
                "You cannot specify minimap2 options (--pacbio) unless you also use --minimap2. \
                 Please respecify!"
                    .into(),
            ));
        }
        if cli.mm2_nanopore {
            return Err(AlignerError::Validation(
                "You cannot specify minimap2 options (--nanopore) unless you also use --minimap2. \
                 Please respecify!"
                    .into(),
            ));
        }
        return Ok(None);
    }

    match cli.maximum_length_cutoff {
        Some(v) => {
            if !(100..=100_000).contains(&v) {
                return Err(AlignerError::Validation(
                    "Please select a sensible maximum sequence length cutoff (currently \
                     100-100,000 bp)"
                        .into(),
                ));
            }
            Ok(Some(v))
        }
        // Perl 8354: default cutoff when --mm2_maximum_length is absent.
        None => Ok(Some(10000)),
    }
}

fn resolve_library(cli: &Cli) -> Result<LibraryType> {
    if cli.non_directional {
        if cli.pbat {
            return Err(AlignerError::Validation(
                "A library can only be specified to be either non-directional or a PBAT-Seq library. \
                 Please respecify!"
                    .into(),
            ));
        }
        return Ok(LibraryType::NonDirectional);
    }
    if cli.pbat {
        // Perl 8155–8156: --pbat is incompatible with --gzip and with -f (FastA).
        if cli.gzip {
            return Err(AlignerError::Validation(
                "The option --pbat is currently not compatible with --gzip. Please run alignments with \
                 uncompressed temporary files, i.e. lose the option --gzip"
                    .into(),
            ));
        }
        if cli.fasta {
            return Err(AlignerError::Validation(
                "The option --pbat is currently only working with FastQ files. Please respecify (i.e. \
                 lose the option -f)!"
                    .into(),
            ));
        }
        return Ok(LibraryType::Pbat);
    }
    Ok(LibraryType::Directional)
}

/// Validate `--multicore`/`--parallel` (file-level). Perl 8244: must be ≥ 1.
/// (The feature itself is wired in Phase 9; this guards the input value now.)
fn validate_multicore(cli: &Cli) -> Result<()> {
    if let Some(m) = cli.multicore
        && m < 1
    {
        return Err(AlignerError::Validation(format!(
            "Core usage needs to be set to 1 or more (currently selected {m}). Please respecify!"
        )));
    }
    Ok(())
}

/// Flags that are recognised (parsed) but not yet *active* in this build — they
/// take effect in a later phase. Returned so the caller can warn the user
/// rather than silently accepting and ignoring them (code-review finding).
pub fn deferred_flags(cli: &Cli) -> Vec<&'static str> {
    let mut v = Vec::new();
    let mut push = |cond: bool, name: &'static str| {
        if cond {
            v.push(name);
        }
    };
    // NB: --skip/--upto/--gzip/--prefix ACTIVE as of Phase 2; --basename as of
    // Phase 5; --unmapped/--ambiguous/--ambig_bam as of Phase 6; --multicore/
    // --parallel as of Phase 9b — none listed here. --rg_tag/--slam/--non_bs_mm/
    // --sam-no-hd are HARD-REJECTED (see reject_unsupported_output_flags).
    // --nucleotide_coverage is wired in a later phase (reports).
    push(cli.nucleotide_coverage, "--nucleotide_coverage");
    push(cli.old_flag, "--old_flag");
    v
}

fn resolve_format(cli: &Cli) -> Result<ReadFormat> {
    if cli.fasta && cli.fastq {
        return Err(AlignerError::Validation(
            "Please specify either -q/--fastq OR -f/--fasta, not both.".into(),
        ));
    }
    Ok(if cli.fasta {
        ReadFormat::FastA
    } else {
        ReadFormat::FastQ
    })
}

/// Resolve the genome argument and the positional read files. The genome is
/// `--genome` if given, otherwise the first positional (Perl `shift @ARGV`,
/// 7604–7612). Returns `(genome_path, remaining_positional_reads)`.
fn resolve_genome_and_positional(cli: &Cli) -> Result<(PathBuf, Vec<String>)> {
    match &cli.genome {
        Some(g) => Ok((g.clone(), cli.positional.clone())),
        None => {
            let mut it = cli.positional.iter();
            let genome = it.next().ok_or_else(|| {
                AlignerError::Validation(
                    "No genome folder specified! USAGE: bismark_rs [options] <genome_folder> \
                     {-1 <mates1> -2 <mates2> | <singles>}"
                        .into(),
                )
            })?;
            Ok((PathBuf::from(genome), it.cloned().collect()))
        }
    }
}

fn resolve_layout(cli: &Cli, reads_positional: &[String]) -> Result<ReadLayout> {
    if let Some(m1) = &cli.mates1 {
        // Paired-end.
        if cli.single_end.is_some() {
            return Err(AlignerError::Validation(
                "You cannot set --single_end and supply files in paired-end format (-1 <mates1> -2 <mates2>). Please respecify!".into(),
            ));
        }
        let m2 = cli.mates2.as_ref().ok_or_else(|| {
            AlignerError::Validation(
                "Paired-end mapping requires the format: -1 <mates1> -2 <mates2>, please respecify!".into(),
            )
        })?;
        let mates1: Vec<String> = m1.split(',').map(str::to_string).collect();
        let mates2: Vec<String> = m2.split(',').map(str::to_string).collect();
        if mates1.len() != mates2.len() {
            return Err(AlignerError::Validation(
                "Paired-end mapping requires the same amount of mate1 and mate2 files, please respecify! (format: -1 <mates1> -2 <mates2>)".into(),
            ));
        }
        for (a, b) in mates1.iter().zip(&mates2) {
            if a == b {
                return Err(AlignerError::Validation(format!(
                    "[FATAL ERROR]: Read 1 ({a}) and Read 2 ({b}) files were specified as the exact same file, which is almost certainly unintentional (and wrong). Please re-specify!"
                )));
            }
            check_exists(a)?;
            check_exists(b)?;
        }
        return Ok(ReadLayout::PairedEnd { mates1, mates2 });
    }

    if cli.mates2.is_some() {
        return Err(AlignerError::Validation(
            "Paired-end mapping requires the format: -1 <mates1> -2 <mates2>, please respecify!"
                .into(),
        ));
    }

    // Single-end: explicit --single_end (`:`→`,`), else the positional reads.
    let reads: Vec<String> = if let Some(se) = &cli.single_end {
        se.replace(':', ",")
            .split(',')
            .map(str::to_string)
            .collect()
    } else {
        reads_positional.to_vec()
    };
    if reads.is_empty() || reads.iter().all(String::is_empty) {
        return Err(AlignerError::Validation(
            "No filename supplied! Please specify one or more files for single-end Bismark mapping!".into(),
        ));
    }
    for r in &reads {
        check_exists(r)?;
    }
    Ok(ReadLayout::SingleEnd { reads })
}

fn check_exists(file: &str) -> Result<()> {
    if Path::new(file).exists() {
        Ok(())
    } else {
        Err(AlignerError::InputFileMissing(file.to_string()))
    }
}

fn resolve_output(cli: &Cli) -> Result<OutputTarget> {
    if cli.sam {
        return Err(AlignerError::Unsupported(
            "SAM output is not yet supported in v1 (BAM only).".into(),
        ));
    }
    if cli.cram {
        return Err(AlignerError::Unsupported(
            "CRAM output is not yet supported in v1 (BAM only).".into(),
        ));
    }
    Ok(OutputTarget {
        output_dir: cli.output_dir.clone().unwrap_or_default(),
        temp_dir: cli.temp_dir.clone().unwrap_or_default(),
        basename: cli.basename.clone(),
        // Perl 8238: `$prefix =~ s/\.+$//` — strip trailing dots (the `.` joining
        // prefix to the file name is added at use-time).
        prefix: cli
            .prefix
            .clone()
            .map(|p| p.trim_end_matches('.').to_string()),
        format: OutputFormat::Bam,
        gzip: cli.gzip,
    })
}

impl RunConfig {
    /// A human-readable resolved-config summary (STDERR; not byte-gated).
    pub fn summary(&self) -> String {
        let library = match self.library {
            LibraryType::Directional => "directional",
            LibraryType::NonDirectional => "non-directional",
            LibraryType::Pbat => "pbat",
        };
        let (layout, files) = match &self.layout {
            ReadLayout::SingleEnd { reads } => ("single-end", reads.join(", ")),
            ReadLayout::PairedEnd { mates1, mates2 } => {
                let pairs: Vec<String> = mates1
                    .iter()
                    .zip(mates2)
                    .map(|(a, b)| format!("{a}+{b}"))
                    .collect();
                ("paired-end", pairs.join(", "))
            }
        };
        let format = match self.format {
            ReadFormat::FastQ => "FASTQ",
            ReadFormat::FastA => "FASTA",
        };
        format!(
            "Bismark aligner (Rust) — resolved configuration\n\
               aligner:        {} {} ({})\n\
               library:        {library}\n\
               layout:         {layout} [{format}]\n\
               reads:          {files}\n\
               genome:         {}\n\
               CT index:       {}\n\
               GA index:       {}\n\
               large index:    {}\n\
               FASTA(s):       {} file(s) ({:?})\n\
               aligner_options: {}\n\
               output:         BAM, dir={:?}, basename={:?}",
            self.aligner.name(),
            self.detected_aligner.version,
            self.detected_aligner.path.display(),
            self.genome.genome_dir.display(),
            self.genome.ct_index_basename.display(),
            self.genome.ga_index_basename.display(),
            self.genome.large_index,
            self.genome.fastas.len(),
            self.genome.fasta_kind,
            self.aligner_options,
            self.output.output_dir,
            self.output.basename,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    fn cli_from(args: &[&str]) -> Cli {
        let mut v = vec!["bismark_rs"];
        v.extend_from_slice(args);
        Cli::parse_from(v)
    }

    #[test]
    fn resolve_aligner_defaults_to_bowtie2() {
        assert_eq!(resolve_aligner(&cli_from(&[])).unwrap(), Aligner::Bowtie2);
    }

    #[test]
    fn bam_flag_is_accepted_as_default_confirming_noop() {
        // Perl `bismark` has `--bam`; nf-core/methylseq's BISMARK_ALIGN passes it.
        // The modernized CLI makes BAM the default, so `--bam` must be ACCEPTED
        // (not rejected) — a no-op. Without the flag defined, clap would error here.
        assert!(cli_from(&["--bam"]).bam);
        // and it doesn't perturb the format default (no --sam/--cram → BAM).
        let c = cli_from(&["--bam"]);
        assert!(!c.sam && !c.cram);
    }

    #[test]
    fn resolve_aligner_selects_hisat2() {
        assert_eq!(
            resolve_aligner(&cli_from(&["--hisat2"])).unwrap(),
            Aligner::Hisat2
        );
    }

    // ---- HISAT2 `--multicore N` → `-p N` remap (Approach B-faithful) -----------

    #[test]
    fn hisat2_multicore_threads_maps_only_for_hisat2_with_n_gt_1() {
        // HISAT2 + multicore>1 → Some(N) (the `-p N` threads to inject).
        assert_eq!(hisat2_multicore_threads(Aligner::Hisat2, Some(4)), Some(4));
        assert_eq!(hisat2_multicore_threads(Aligner::Hisat2, Some(2)), Some(2));
        // single-core / absent → None (no remap).
        assert_eq!(hisat2_multicore_threads(Aligner::Hisat2, Some(1)), None);
        assert_eq!(hisat2_multicore_threads(Aligner::Hisat2, None), None);
        // never for Bowtie 2 (keeps the fork model, Phase 9b) or minimap2.
        assert_eq!(hisat2_multicore_threads(Aligner::Bowtie2, Some(4)), None);
        assert_eq!(hisat2_multicore_threads(Aligner::Minimap2, Some(4)), None);
    }

    #[test]
    fn hisat2_multicore_plus_explicit_p_is_rejected() {
        // The remap drives HISAT2's `-p`, so an explicit `-p M` alongside is ambiguous.
        let cli = cli_from(&[
            "reads.fq.gz",
            "--genome",
            "idx",
            "--bam",
            "--hisat2",
            "--multicore",
            "4",
            "-p",
            "2",
        ]);
        let err = resolve(&cli, "cmd".to_string())
            .expect_err("--hisat2 --multicore N + -p M must be rejected as ambiguous");
        assert!(err.to_string().contains("ambiguous"), "got: {err}");
    }

    #[test]
    fn bowtie2_multicore_plus_p_is_not_rejected_by_the_hisat2_guard() {
        // The ambiguity guard is HISAT2-only: Bowtie 2 `--multicore` (fork) + `-p`
        // (per-instance threads) is a legitimate combination and must not trip it.
        // (Resolve will still fail later on the fake `idx` genome dir — but NOT with
        // the "ambiguous" message.)
        let cli = cli_from(&[
            "reads.fq.gz",
            "--genome",
            "idx",
            "--bam",
            "--multicore",
            "4",
            "-p",
            "2",
        ]);
        if let Err(e) = resolve(&cli, "cmd".to_string()) {
            assert!(
                !e.to_string().contains("ambiguous"),
                "Bowtie 2 --multicore + -p must NOT trip the HISAT2 ambiguity guard; got: {e}"
            );
        }
    }

    /// V11: `--minimap2` is no longer deferred — it now resolves to the Minimap2
    /// backend (Phase 4 un-deferral).
    #[test]
    fn resolve_aligner_selects_minimap2() {
        assert_eq!(
            resolve_aligner(&cli_from(&["--minimap2"])).unwrap(),
            Aligner::Minimap2
        );
    }

    #[test]
    fn minimap2_token_and_name() {
        assert_eq!(Aligner::Minimap2.token(), "mm2");
        // lowercase — byte-significant in the SE report "run with minimap2" line.
        assert_eq!(Aligner::Minimap2.name(), "minimap2");
    }

    /// Phase 3 (T1, design#7): rammap's token is the full word `rammap` (NOT
    /// abbreviated like `mm2`) → `_bismark_rammap`; name = `rammap` (report line).
    #[test]
    fn rammap_token_and_name() {
        assert_eq!(Aligner::Rammap.token(), "rammap");
        assert_eq!(Aligner::Rammap.name(), "rammap");
    }

    #[test]
    fn resolve_aligner_rejects_conflicting_selections() {
        assert!(resolve_aligner(&cli_from(&["--hisat2", "--bowtie2"])).is_err());
        assert!(resolve_aligner(&cli_from(&["--hisat2", "--minimap2"])).is_err());
        assert!(resolve_aligner(&cli_from(&["--minimap2", "--bowtie2"])).is_err());
    }

    /// Phase 3 (T2): `--rammap` selects [`Aligner::Rammap`].
    #[test]
    fn resolve_aligner_selects_rammap() {
        assert_eq!(
            resolve_aligner(&cli_from(&["--rammap"])).unwrap(),
            Aligner::Rammap
        );
    }

    /// Phase 3 (T2): `--rammap` with any other engine dies (ordered before the
    /// `Ok`s so the conflict fires, never a silent precedence pick).
    #[test]
    fn resolve_aligner_rejects_rammap_with_other_engines() {
        assert!(resolve_aligner(&cli_from(&["--rammap", "--bowtie2"])).is_err());
        assert!(resolve_aligner(&cli_from(&["--rammap", "--hisat2"])).is_err());
        assert!(resolve_aligner(&cli_from(&["--rammap", "--minimap2"])).is_err());
    }

    // ---- --local scope (Bowtie 2 only; rejected for HISAT2/minimap2 + combined-index) ----
    // The rejects fire early in `resolve` (before genome discovery), so they're
    // testable without an on-disk index.
    #[test]
    fn resolve_local_aligner_scope() {
        // HISAT2 --local is now SUPPORTED — resolve no longer rejects it on the --local scope
        // gate. With these fixture-free args it falls through to a *different* pre-I/O error
        // ("No genome folder specified"), NOT a --local reject (the GAP would be the reject).
        if let Err(e) = resolve(&cli_from(&["--local", "--hisat2"]), "cmd".into()) {
            let m = e.to_string();
            assert!(
                !m.contains("not supported with --minimap2") && !m.contains("local alignment"),
                "HISAT2 --local must not be rejected by the --local scope gate; got: {m}"
            );
        }
        // minimap2 --local stays REJECTED, stating minimap2 is local "by design" (Q3).
        let err = resolve(&cli_from(&["--local", "--minimap2"]), "cmd".into()).unwrap_err();
        let m = err.to_string();
        assert!(
            m.contains("--minimap2") && m.contains("by design"),
            "minimap2 --local must reject with the 'local by design' rationale; got: {m}"
        );
    }

    /// Phase 3 (T3): `--rammap --local` is rejected (minimap-like, local by design).
    /// The reject is reachable via `resolve` (it precedes `resolve_layout`/`check_exists`),
    /// and the message names `--rammap` (NOT `--minimap2`) via `aligner.name()`.
    #[test]
    fn rammap_rejects_local() {
        // The --local scope gate fires before genome discovery, so no on-disk index
        // is needed (mirrors `resolve_local_aligner_scope`).
        let err = resolve(&cli_from(&["--rammap", "--local"]), "cmd".into()).unwrap_err();
        let m = err.to_string();
        assert!(
            m.contains("--rammap") && m.contains("by design") && !m.contains("--minimap2"),
            "rammap --local must reject naming --rammap with the 'by design' rationale; got: {m}"
        );
    }

    /// Epic 06152026 Phase 4: `--rammap_inprocess` requires `--rammap`. The guard fires
    /// right after `resolve_aligner` (before genome discovery), so no on-disk index is
    /// needed (mirrors `rammap_rejects_local`).
    #[test]
    fn rammap_inprocess_requires_rammap() {
        let err = resolve(&cli_from(&["--rammap_inprocess"]), "cmd".into()).unwrap_err();
        assert!(
            err.to_string()
                .contains("--rammap_inprocess requires --rammap"),
            "got: {err}"
        );
    }

    /// Epic 06152026 Phase 4: the `--rammap_inprocess` flag parses and is off by default.
    #[test]
    fn rammap_inprocess_flag_parses() {
        assert!(cli_from(&["--rammap", "--rammap_inprocess"]).rammap_inprocess);
        assert!(!cli_from(&["--rammap"]).rammap_inprocess);
    }

    // ---- #787 Illumina 5-Base mode guards ----------------------------------

    /// The `--illumina_5base` flag (and its `--five_base` alias) parses, off by default.
    #[test]
    fn illumina_5base_flag_parses() {
        assert!(cli_from(&["--illumina_5base"]).illumina_5base);
        assert!(cli_from(&["--five_base"]).illumina_5base);
        assert!(!cli_from(&["--minimap2"]).illumina_5base);
    }

    /// `--illumina_5base` resolves to the minimap2 (unconverted) backend (fires in
    /// `resolve_aligner`, before genome discovery — no on-disk index needed).
    #[test]
    fn illumina_5base_resolves_to_minimap2() {
        assert_eq!(
            resolve_aligner(&cli_from(&["--illumina_5base"])).unwrap(),
            Aligner::Minimap2
        );
        // `--minimap2` may co-occur (5-Base IS a minimap2 run).
        assert_eq!(
            resolve_aligner(&cli_from(&["--illumina_5base", "--minimap2"])).unwrap(),
            Aligner::Minimap2
        );
    }

    /// `--five_base_deconvolution` requires `--illumina_5base` (it post-processes a
    /// 5-Base run). Fires early in resolve() (before genome discovery).
    #[test]
    fn five_base_deconvolution_requires_illumina_5base() {
        let err = resolve(&cli_from(&["--five_base_deconvolution"]), "cmd".into()).unwrap_err();
        assert!(
            err.to_string()
                .contains("--five_base_deconvolution requires --illumina_5base"),
            "got: {err}"
        );
        // accepted together (parses; resolves to minimap2).
        assert!(
            cli_from(&["--illumina_5base", "--five_base_deconvolution"]).five_base_deconvolution
        );
    }

    /// `--illumina_5base` engine selection: `--rammap` is rejected; `--bowtie2`/
    /// `--hisat2` need `--five_base_index` (a normal unconverted index) and then resolve
    /// to that engine; no engine flag → minimap2 (genome FASTA).
    #[test]
    fn illumina_5base_engine_selection() {
        // rammap: rejected outright.
        let err = resolve_aligner(&cli_from(&["--illumina_5base", "--rammap"])).unwrap_err();
        assert!(
            err.to_string().contains("not supported with --rammap"),
            "{err}"
        );
        // bowtie2/hisat2 without an index: fail loud.
        for flag in ["--bowtie2", "--hisat2"] {
            let err = resolve_aligner(&cli_from(&["--illumina_5base", flag])).unwrap_err();
            assert!(
                err.to_string().contains("requires --five_base_index"),
                "{flag}: {err}"
            );
        }
        // bowtie2/hisat2 WITH an index: resolve to that engine.
        assert_eq!(
            resolve_aligner(&cli_from(&[
                "--illumina_5base",
                "--bowtie2",
                "--five_base_index",
                "idx"
            ]))
            .unwrap(),
            Aligner::Bowtie2
        );
        assert_eq!(
            resolve_aligner(&cli_from(&[
                "--illumina_5base",
                "--hisat2",
                "--five_base_index",
                "idx"
            ]))
            .unwrap(),
            Aligner::Hisat2
        );
        // --five_base_index without an engine flag is rejected (minimap2 needs no index).
        let err = resolve_aligner(&cli_from(&["--illumina_5base", "--five_base_index", "idx"]))
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("--five_base_index only applies to"),
            "{err}"
        );
    }

    #[test]
    fn resolve_rejects_local_with_combined_index() {
        for flag in [
            "--combined_index",
            "--combined_index_sequential",
            "--combined_index_single_pass",
        ] {
            let err = resolve(&cli_from(&["--local", flag]), "cmd".into()).unwrap_err();
            assert!(
                err.to_string()
                    .contains("--local is not supported with --combined_index"),
                "{flag}: {err}"
            );
        }
    }

    // ---- minimap2 preset/length flag gating (Phase 4) ----------------------

    /// V11: the four `--mm2_*` flags die outside minimap2 mode (Perl 8329-8341).
    #[test]
    fn mm2_flags_require_minimap2_mode() {
        for arg in ["--mm2_short_reads", "--mm2_pacbio", "--mm2_nanopore"] {
            assert!(
                resolve_mm2_max_length(&cli_from(&[arg]), Aligner::Bowtie2).is_err(),
                "{arg} should die without --minimap2"
            );
        }
        assert!(
            resolve_mm2_max_length(
                &cli_from(&["--mm2_maximum_length", "5000"]),
                Aligner::Bowtie2
            )
            .is_err()
        );
        // …and none are an error in minimap2 mode.
        assert!(
            resolve_mm2_max_length(&cli_from(&["--mm2_short_reads"]), Aligner::Minimap2).is_ok()
        );
    }

    /// V7: `--mm2_maximum_length` range-die (`<100` / `>100000`); boundaries OK;
    /// absent → default 10000 in minimap2 mode (Perl 8344-8356).
    #[test]
    fn mm2_maximum_length_range_and_default() {
        let lower = resolve_mm2_max_length(
            &cli_from(&["--mm2_maximum_length", "99"]),
            Aligner::Minimap2,
        );
        assert!(lower.is_err(), "<100 must die");
        let upper = resolve_mm2_max_length(
            &cli_from(&["--mm2_maximum_length", "100001"]),
            Aligner::Minimap2,
        );
        assert!(upper.is_err(), ">100000 must die");
        // boundaries are valid.
        assert_eq!(
            resolve_mm2_max_length(
                &cli_from(&["--mm2_maximum_length", "100"]),
                Aligner::Minimap2
            )
            .unwrap(),
            Some(100)
        );
        assert_eq!(
            resolve_mm2_max_length(
                &cli_from(&["--mm2_maximum_length", "100000"]),
                Aligner::Minimap2
            )
            .unwrap(),
            Some(100000)
        );
        // absent → default 10000.
        assert_eq!(
            resolve_mm2_max_length(&cli_from(&[]), Aligner::Minimap2).unwrap(),
            Some(10000)
        );
        // non-minimap2 → None (the convert guard stays inert).
        assert_eq!(
            resolve_mm2_max_length(&cli_from(&[]), Aligner::Bowtie2).unwrap(),
            None
        );
    }

    /// Phase 3 (T3, design#3): rammap honors the SAME `--mm2_maximum_length` cutoff
    /// (explicit value passes through; absent → the default 10000) — for fairness vs
    /// `--minimap2`, NOT the "unless --minimap2" die.
    #[test]
    fn rammap_honors_mm2_max_length() {
        assert_eq!(
            resolve_mm2_max_length(
                &cli_from(&["--rammap", "--mm2_maximum_length", "50000"]),
                Aligner::Rammap
            )
            .unwrap(),
            Some(50000)
        );
        assert_eq!(
            resolve_mm2_max_length(&cli_from(&["--rammap"]), Aligner::Rammap).unwrap(),
            Some(10000)
        );
    }

    // ---- --combined_index (v2) scope guard (Phase 2; §3.1) -----------------

    fn se() -> ReadLayout {
        ReadLayout::SingleEnd {
            reads: vec!["r.fq".into()],
        }
    }
    fn pe() -> ReadLayout {
        ReadLayout::PairedEnd {
            mates1: vec!["r1.fq".into()],
            mates2: vec!["r2.fq".into()],
        }
    }

    /// SE directional Bowtie 2 is the one supported combination → guard passes.
    #[test]
    fn combined_index_allows_se_directional_bowtie2() {
        let cli = cli_from(&["--combined_index"]);
        assert!(
            reject_combined_index_unsupported(
                &cli,
                Aligner::Bowtie2,
                LibraryType::Directional,
                &se()
            )
            .is_ok()
        );
    }

    /// Phase 3 (T3): `--rammap --combined_index` is rejected. Tested via the unit fn
    /// directly (NOT `resolve`, whose `resolve_layout`/`check_exists` runs first), and
    /// the message names `--rammap` (NOT `--minimap2`) via `aligner.name()`.
    #[test]
    fn rammap_rejects_combined_index() {
        let cli = cli_from(&["--rammap", "--combined_index"]);
        let err = reject_combined_index_unsupported(
            &cli,
            Aligner::Rammap,
            LibraryType::Directional,
            &se(),
        )
        .unwrap_err();
        let m = err.to_string();
        assert!(
            m.contains("--rammap") && !m.contains("--minimap2"),
            "rammap combined-index reject must name --rammap; got: {m}"
        );
    }

    /// Phase 2 (v2.x): paired-end directional Bowtie 2 combined-index is ACCEPTED
    /// (one both-strands C→T pass → OT/OB). PE non-dir/pbat/HISAT2 stay rejected
    /// (see `combined_index_rejects_unsupported_combinations`).
    #[test]
    fn combined_index_allows_pe_directional_and_nondir_bowtie2() {
        // Phase 2: directional PE Bowtie 2 combined.
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index"]),
                Aligner::Bowtie2,
                LibraryType::Directional,
                &pe()
            )
            .is_ok()
        );
        // Phase 3: non-directional PE Bowtie 2 combined (parallel model (a)).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--non_directional"]),
                Aligner::Bowtie2,
                LibraryType::NonDirectional,
                &pe()
            )
            .is_ok()
        );
    }

    /// Without the flag the guard is inert (every combination passes).
    #[test]
    fn combined_index_guard_inert_when_flag_absent() {
        let cli = cli_from(&[]);
        assert!(
            reject_combined_index_unsupported(&cli, Aligner::Hisat2, LibraryType::Pbat, &pe())
                .is_ok()
        );
    }

    /// SE Bowtie 2 is supported for ALL three library types (directional phase 4,
    /// non-directional phase 5, pbat phase 7).
    #[test]
    fn combined_index_allows_all_se_library_types() {
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index"]),
                Aligner::Bowtie2,
                LibraryType::Directional,
                &se()
            )
            .is_ok()
        );
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--non_directional"]),
                Aligner::Bowtie2,
                LibraryType::NonDirectional,
                &se()
            )
            .is_ok()
        );
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--pbat"]),
                Aligner::Bowtie2,
                LibraryType::Pbat,
                &se()
            )
            .is_ok()
        );
        // Phase 1 (v2.x): HISAT2 SE combined-index accepted for all 3 library types.
        for (args, lib) in [
            (
                vec!["--combined_index", "--hisat2"],
                LibraryType::Directional,
            ),
            (
                vec!["--combined_index", "--hisat2", "--non_directional"],
                LibraryType::NonDirectional,
            ),
            (
                vec!["--combined_index", "--hisat2", "--pbat"],
                LibraryType::Pbat,
            ),
        ] {
            assert!(
                reject_combined_index_unsupported(&cli_from(&args), Aligner::Hisat2, lib, &se())
                    .is_ok(),
                "HISAT2 SE combined should be accepted: {args:?}"
            );
        }
    }

    /// Every not-yet-supported combination is rejected loudly (never-silent).
    #[test]
    fn combined_index_rejects_unsupported_combinations() {
        // HISAT2 paired-end combined directional → ACCEPTED (Phase 5; reuses the PE machinery
        // over the combined `.ht2` index — all 3 HISAT2 PE library types are now lifted).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--hisat2"]),
                Aligner::Hisat2,
                LibraryType::Directional,
                &pe()
            )
            .is_ok()
        );
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--minimap2"]),
                Aligner::Minimap2,
                LibraryType::Directional,
                &se()
            )
            .is_err()
        );
        // paired-end pbat Bowtie 2 → ACCEPTED (Phase 4; one both-strands G→A pass →
        // CTOT/CTOB — all three PE Bowtie 2 library types are now lifted, Phases 2-4).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--pbat"]),
                Aligner::Bowtie2,
                LibraryType::Pbat,
                &pe()
            )
            .is_ok()
        );
        // paired-end pbat HISAT2 → ACCEPTED (Phase 5).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--hisat2", "--pbat"]),
                Aligner::Hisat2,
                LibraryType::Pbat,
                &pe()
            )
            .is_ok()
        );
        // paired-end non-directional HISAT2 → ACCEPTED (Phase 5; parallel model (a),
        // two both-strands HISAT2 PE passes). (PE single-pass / sequential stay Bowtie-2-
        // only via the SE-only C2 guard — covered by `combined_index_{single_pass,
        // sequential}_requires_*` below.)
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--hisat2", "--non_directional"]),
                Aligner::Hisat2,
                LibraryType::NonDirectional,
                &pe()
            )
            .is_ok()
        );
        // paired-end minimap2 combined → still rejected (no 4-strand dispatch / no PE oracle).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--minimap2"]),
                Aligner::Minimap2,
                LibraryType::Directional,
                &pe()
            )
            .is_err()
        );
        // multicore
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--multicore", "4"]),
                Aligner::Bowtie2,
                LibraryType::Directional,
                &se()
            )
            .is_err()
        );
    }

    /// `--combined_index_single_pass` (model b) requires `--combined_index --non_directional`,
    /// Bowtie 2 (SE or PE — v2.x Phase 6 lifted PE). Every other combination is rejected
    /// loudly (never-silent).
    #[test]
    fn combined_index_single_pass_requires_combined_index_and_non_directional() {
        // OK: combined_index + non_directional, SE Bowtie 2.
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&[
                    "--combined_index",
                    "--non_directional",
                    "--combined_index_single_pass"
                ]),
                Aligner::Bowtie2,
                LibraryType::NonDirectional,
                &se()
            )
            .is_ok()
        );
        // --combined_index_single_pass WITHOUT --combined_index → reject (checked before
        // the !combined_index early return).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--non_directional", "--combined_index_single_pass"]),
                Aligner::Bowtie2,
                LibraryType::NonDirectional,
                &se()
            )
            .is_err()
        );
        // directional / pbat → reject (model b is non-dir-only; the others are
        // already single-pass).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--combined_index_single_pass"]),
                Aligner::Bowtie2,
                LibraryType::Directional,
                &se()
            )
            .is_err()
        );
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--pbat", "--combined_index_single_pass"]),
                Aligner::Bowtie2,
                LibraryType::Pbat,
                &se()
            )
            .is_err()
        );
        // v2.x Phase 6: PE non-dir Bowtie 2 + single_pass is now ACCEPTED (the SE-only
        // reject was lifted); multicore still inherits the --combined_index reject.
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&[
                    "--combined_index",
                    "--non_directional",
                    "--combined_index_single_pass"
                ]),
                Aligner::Bowtie2,
                LibraryType::NonDirectional,
                &pe()
            )
            .is_ok()
        );
        // PE directional + single_pass → reject (non-dir-only; the library guard fires
        // regardless of layout).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--combined_index_single_pass"]),
                Aligner::Bowtie2,
                LibraryType::Directional,
                &pe()
            )
            .is_err()
        );
        // SE HISAT2 → reject: single-pass is the Bowtie-2-specific tagged exec model (v2.x
        // C2 guard; HISAT2 non-dir combined uses the default parallel model (a)).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&[
                    "--combined_index",
                    "--non_directional",
                    "--combined_index_single_pass"
                ]),
                Aligner::Hisat2,
                LibraryType::NonDirectional,
                &se()
            )
            .is_err()
        );
        // PE HISAT2 + single_pass → reject too (Bowtie 2-only — the aligner guard fires
        // after the lifted PE check, so PE-HISAT2 does not silently route to the BT2 driver).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&[
                    "--combined_index",
                    "--non_directional",
                    "--combined_index_single_pass"
                ]),
                Aligner::Hisat2,
                LibraryType::NonDirectional,
                &pe()
            )
            .is_err()
        );
    }

    /// `--combined_index_sequential` (the faithful sequential exec model) requires
    /// `--combined_index --non_directional`, Bowtie 2 or HISAT2 (SE or PE — v2.x Phase 6
    /// lifted PE, Phase 7 lifted HISAT2), and is mutually exclusive with
    /// `--combined_index_single_pass`. Every other combination is rejected loudly
    /// (never-silent). Mirrors the model-(b) guard test above.
    #[test]
    fn combined_index_sequential_requires_combined_index_and_non_directional() {
        // OK: combined_index + non_directional, SE Bowtie 2.
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&[
                    "--combined_index",
                    "--non_directional",
                    "--combined_index_sequential"
                ]),
                Aligner::Bowtie2,
                LibraryType::NonDirectional,
                &se()
            )
            .is_ok()
        );
        // --combined_index_sequential WITHOUT --combined_index → reject (checked
        // before the !combined_index early return).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--non_directional", "--combined_index_sequential"]),
                Aligner::Bowtie2,
                LibraryType::NonDirectional,
                &se()
            )
            .is_err()
        );
        // directional / pbat → reject (sequential is non-dir-only; the others are
        // already single-pass / one index load).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--combined_index_sequential"]),
                Aligner::Bowtie2,
                LibraryType::Directional,
                &se()
            )
            .is_err()
        );
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--pbat", "--combined_index_sequential"]),
                Aligner::Bowtie2,
                LibraryType::Pbat,
                &se()
            )
            .is_err()
        );
        // mutual exclusion: --combined_index_sequential + --combined_index_single_pass
        // → reject (competing execution models), regardless of the rest being valid.
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&[
                    "--combined_index",
                    "--non_directional",
                    "--combined_index_sequential",
                    "--combined_index_single_pass"
                ]),
                Aligner::Bowtie2,
                LibraryType::NonDirectional,
                &se()
            )
            .is_err()
        );
        // v2.x Phase 6: PE non-dir Bowtie 2 + sequential is now ACCEPTED (the SE-only
        // reject was lifted).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&[
                    "--combined_index",
                    "--non_directional",
                    "--combined_index_sequential"
                ]),
                Aligner::Bowtie2,
                LibraryType::NonDirectional,
                &pe()
            )
            .is_ok()
        );
        // --multicore still inherits the --combined_index reject (SE or PE).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&[
                    "--combined_index",
                    "--non_directional",
                    "--combined_index_sequential",
                    "--multicore",
                    "4"
                ]),
                Aligner::Bowtie2,
                LibraryType::NonDirectional,
                &se()
            )
            .is_err()
        );
        // PE pbat + sequential → reject (non-dir-only; library guard, layout-independent).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--pbat", "--combined_index_sequential"]),
                Aligner::Bowtie2,
                LibraryType::Pbat,
                &pe()
            )
            .is_err()
        );
        // v2.x Phase 7: HISAT2 non-dir sequential is now ACCEPTED — SE and PE (the
        // sequential model is faithful + aligner-agnostic).
        for layout in [se(), pe()] {
            assert!(
                reject_combined_index_unsupported(
                    &cli_from(&[
                        "--combined_index",
                        "--non_directional",
                        "--combined_index_sequential"
                    ]),
                    Aligner::Hisat2,
                    LibraryType::NonDirectional,
                    &layout
                )
                .is_ok()
            );
        }
        // minimap2 sequential → still REJECTED (combined-index is Bowtie 2 / HISAT2 only).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&[
                    "--combined_index",
                    "--non_directional",
                    "--combined_index_sequential"
                ]),
                Aligner::Minimap2,
                LibraryType::NonDirectional,
                &se()
            )
            .is_err()
        );
        // HISAT2 directional / pbat + sequential → REJECT (non-dir-only; the library guard
        // fires before the aligner guard, layout-independent).
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--combined_index_sequential"]),
                Aligner::Hisat2,
                LibraryType::Directional,
                &pe()
            )
            .is_err()
        );
        assert!(
            reject_combined_index_unsupported(
                &cli_from(&["--combined_index", "--pbat", "--combined_index_sequential"]),
                Aligner::Hisat2,
                LibraryType::Pbat,
                &pe()
            )
            .is_err()
        );
    }
}
