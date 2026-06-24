//! `bismark-aligner` — Rust port of the Perl `bismark` aligner **wrapper**.
//!
//! `bismark` is not an aligner: it converts reads (C→T, plus the G→A complement
//! for non-directional), drives 2–4 external **Bowtie 2** instances against the
//! bisulfite-converted indexes, merges/scores their SAM in read-ID lockstep,
//! performs the bisulfite best-alignment selection + strand assignment + the
//! `XM`/`XR`/`XG` methylation call, and writes the Bismark BAM + reports.
//!
//! **Acceptance gate:** byte-identical *decompressed* SAM content (`samtools
//! view` + `-H`) vs Perl Bismark v0.25.1 driving the pinned Bowtie 2 2.5.5
//! (Phase-0 spike confirmed the premise; raw BGZF bytes are NOT gated since the
//! Rust path writes via noodles, not samtools).
//!
//! **This crate is built phase by phase** (see `plans/05312026_bismark-aligner/`).
//! Implemented so far: Phase 1 CLI/discovery/detection, Phase 2 read conversion,
//! Phase 3 single-instance stream, Phase 4 N-way lockstep merge + scoring + MAPQ,
//! Phase 5 genomic-seq + `XM`/`XR`/`XG` call + BAM output (the first byte-identity
//! gate, passed on oxy), Phase 6 the alignment report + `--unmapped`/`--ambiguous`
//! FastQ + `--ambig_bam`, Phase 7 paired-end (directional), Phase 8 the
//! non-directional + pbat library types, and Phase 9a FastA input. **FastQ AND
//! FastA, single-end + paired-end, all library types (directional/non-directional/
//! pbat), run end to end**, and Phase 9b order-preserving `--multicore`/`--parallel`
//! threading (worker-count-invariant output — `parallel`). The full-scale real-data
//! gate (Phase 10) lands later.

pub mod align;
pub mod aligner;
pub mod aux_out;
pub mod cli;
pub mod combined;
pub mod config;
pub mod convert;
pub mod discovery;
pub mod error;
pub mod five_base_deconv;
pub mod five_base_duplex;
pub mod genome;
pub mod inprocess;
pub mod mapq;

// Phase 1 (epic 06152026): re-export the `rammap-core` crate (its lib name is
// `rammap`) under the `rammap-inprocess` feature so integration tests (a separate
// crate that does NOT inherit the parent crate's normal deps) can construct an
// `Arc<rammap::Aligner>` for the record-level cross-check. Default/Mac build never
// sees this. The extern crate is `rammap` (the package `rammap-core`'s lib name).
#[cfg(feature = "rammap-inprocess")]
pub use ::rammap;
pub mod merge;
pub mod methylation;
pub mod options;
pub mod output;
pub mod parallel;
pub mod report;

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use flate2::Compression;
use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;

use bismark_io::BamWriter;

use crate::align::{
    AlignerStream, FileSamStream, Orientation, PairedAlignerStream, PairedFileSamStream,
    PairedSamStream, SamRecord, SamStream,
};
use crate::aux_out::AuxKind;
use crate::config::{Aligner, LibraryType, ReadFormat, ReadLayout};
use crate::genome::{Genome, read_genome_into_memory};
use crate::merge::{
    BestAlignment, BestAlignmentPaired, Counters, Decision, DecisionPaired,
    check_results_paired_end, check_results_single_end,
};
use crate::methylation::{
    extract_corresponding_genomic_sequence_paired_end,
    extract_corresponding_genomic_sequence_single_end, methylation_call,
};
use crate::output::{
    build_refid, generate_sam_header, paired_end_sam_output, single_end_sam_output,
    write_raw_pe_ambig_lines, write_raw_sam_line_to_bam, write_record,
};
use crate::report::ReportHeader;

pub use config::{RunConfig, resolve};
pub use error::{AlignerError, Result};

/// The Bismark version this port reproduces in `@PG`/reports/banners.
pub const BISMARK_VERSION: &str = "v0.25.1";

/// `--version` banner (reports the SUITE version via `bismark_meta`; not byte-gated).
pub fn version_string() -> String {
    format!(
        "\n          Bismark - Bisulfite Mapper and Methylation Caller.\n\n          \
         Bismark Aligner (Rust port) Version: {}\n        \
         Copyright 2010-25, Felix Krueger, Altos Bioinformatics\n\n               \
         https://github.com/FelixKrueger/Bismark\n",
        bismark_meta::SUITE_VERSION
    )
}

/// Never-silent notice for the HISAT2 `--multicore N` → `-p N` semantic remap (Approach
/// B-faithful). Pure so it is unit-testable; emitted by `run` when the remap fires.
fn hisat2_multicore_remap_notice(n: u32) -> String {
    format!(
        "Note: --hisat2 with --multicore {n} is interpreted as a single HISAT2 instance with \
         -p {n} threading (--reorder), NOT the fork model: HISAT2 splice-site discovery is not \
         chunk-invariant. This is deterministic and byte-identical to Perl `--hisat2 -p {n}`, but \
         the result depends on the thread count (it is NOT identical to single-core HISAT2)."
    )
}

/// Entry point: resolve the config, then run the pipeline. `command_line` is the
/// verbatim argv (program name excluded), for the eventual `@PG` `CL:` line.
pub fn run(cli: &cli::Cli, command_line: String) -> Result<()> {
    let config = resolve(cli, command_line)?;
    let deferred = config::deferred_flags(cli);
    if !deferred.is_empty() {
        eprintln!(
            "Note: these options are recognised but not yet active in this build \
             (wired in a later phase): {}",
            deferred.join(", ")
        );
    }
    if let Some(n) = config.hisat2_multicore_remap {
        eprintln!("{}", hisat2_multicore_remap_notice(n));
    }
    // Never-silent opt-in notice (#787): --illumina_5base is the inverse-of-bisulfite
    // 5-Base (5mC->T) mode — it aligns the RAW reads to the UNCONVERTED genome with
    // minimap2 and calls methylation with inverted polarity. There is no Perl oracle
    // for 5-Base, so this is NOT byte-identical; it is concordance-validated vs DRAGEN.
    if config.five_base {
        eprintln!(
            "Note: --illumina_5base is the Illumina 5-Base (5mC->T) mode: reads are aligned \
             with minimap2 to the UNCONVERTED genome and methylation is called with inverted \
             polarity (a read T at a genomic C = methylated). Directional only (SE + PE). \
             NOT byte-identical (Perl Bismark has no 5-Base oracle); concordance-validated vs DRAGEN."
        );
    }
    // Never-silent opt-in notice (Phase 3, design#5): --rammap is the pure-Rust
    // minimap2 reimplementation — concordance-validated, NOT byte-identical to
    // minimap2. Emitted here (the `hisat2_multicore_remap_notice` precedent), NOT in
    // `resolve()` (which would spam every rammap unit test).
    if config.aligner == Aligner::Rammap {
        // Name the active backend by reusing the ONE selection predicate, so the
        // printed backend can never disagree with the path actually taken (the
        // in-process branch in `process_se_chunk` uses the same `use_se_inprocess_rammap`).
        // It reads `config.rammap_inprocess` on both builds (feature-off → always false →
        // "subprocess"), so the config field is never a feature-off dead field.
        let inprocess = use_se_inprocess_rammap(&config);
        // Name the active backend (the in-process backend's thread count = --multicore, #995).
        let backend = if inprocess {
            let n = config.multicore.max(1);
            if n > 1 {
                format!("in-process rammap-core backend: lower RAM, {n}-thread")
            } else {
                "in-process rammap-core backend: lower RAM, slower — single-threaded".to_string()
            }
        } else {
            "subprocess rammap binary".to_string()
        };
        eprintln!(
            "Note: --rammap uses the rammap pure-Rust minimap2 reimplementation \
             ({backend}). Alignments are concordance-validated, NOT byte-identical to minimap2."
        );
        // Never-silent FastA fallback: the user OPTED IN (`--rammap_inprocess`) but the
        // in-process stream is FastQ-only, so a FastA run uses the subprocess path instead.
        // (#995: `--multicore N` no longer forces a fallback — the in-process backend is now
        // N-threaded; that former branch is deleted.) Feature-gated. A plain subprocess-default
        // `--rammap` run never reaches here (it never "fell back").
        #[cfg(feature = "rammap-inprocess")]
        if !inprocess && config.rammap_inprocess && !matches!(config.format, ReadFormat::FastQ) {
            eprintln!(
                "Note: --rammap_inprocess supports FastQ input only; this FastA run \
                 uses the subprocess rammap backend."
            );
        }
    }
    eprintln!("{}", config.summary());
    pipeline(&config)?;
    Ok(())
}

/// Dispatch the convert→align→merge pipeline. SE and PE each fold all library
/// types (directional/non-directional/pbat) AND both input formats (FastQ +
/// FastA, Phase 9a) into the generalized `run_se`/`run_pe`. Phase 9b routes
/// `--multicore`/`--parallel N`: N > 1 → the order-preserving contiguous-chunk
/// fan-out (`parallel::run_*_multicore`); N == 1 (default) → the direct path here.
fn pipeline(config: &RunConfig) -> Result<()> {
    // Phase 9b: `--multicore`/`--parallel N` (N > 1) takes the order-preserving
    // contiguous-chunk fan-out; N == 1 (the default) takes the proven single-core
    // direct path — byte-identical by construction (PLAN §3.1).
    let n = config.multicore;
    // #787 Illumina 5-Base: a distinct path that aligns to the UNCONVERTED genome
    // (no C->T read conversion) and inverts the methylation call. Guarded at resolve()
    // to directional + minimap2 + single-instance, so it short-circuits here (SE + PE).
    if config.five_base {
        match &config.layout {
            ReadLayout::SingleEnd { reads } => return run_se_five_base(config, reads),
            ReadLayout::PairedEnd { mates1, mates2 } => {
                return run_pe_five_base(config, mates1, mates2);
            }
        }
    }
    match &config.layout {
        ReadLayout::SingleEnd { reads } => {
            if config.combined_index {
                // v2 opt-in combined-index path (SE only; --multicore rejected at
                // resolve, so this is always the single-core branch). Directional =
                // one C→T pass (OT/OB); non-dir = two passes (C→T + G→A) + 4-strand
                // union; pbat = one G→A pass (CTOT/CTOB).
                if config.combined_index_single_pass {
                    // model (b): single-pass tagged non-dir. The scope guard
                    // (`reject_combined_index_unsupported`) guarantees the library is
                    // NonDirectional whenever this flag is set.
                    run_se_combined_nondir_tagged(config, reads)
                } else if config.combined_index_sequential {
                    // model (a) SEQUENTIAL low-RSS variant: two passes one at a time.
                    // The scope guard guarantees NonDirectional (and rejects it
                    // together with --combined_index_single_pass) whenever this is set.
                    run_se_combined_nondir_sequential(config, reads)
                } else {
                    match config.library {
                        LibraryType::NonDirectional => run_se_combined_nondir(config, reads),
                        LibraryType::Pbat => run_se_combined_pbat(config, reads),
                        LibraryType::Directional => run_se_combined(config, reads),
                    }
                }
            } else if use_se_inprocess_rammap(config) {
                // #995: the in-process rammap backend short-circuits to the SINGLE-process
                // `run_se` regardless of `--multicore N`. The parallelism is intra-process (a
                // shared rayon pool sized by N, inside the stream); routing it through the
                // fork model (`parallel::run_se_multicore`) would reload the index per worker
                // — the RAM blowup Phase 2 avoided. Reuses the ONE selection predicate (so the
                // routing, the notice, and the `process_se_chunk` branch can't disagree).
                run_se(config, reads)
            } else if n > 1 {
                parallel::run_se_multicore(config, reads, n)
            } else {
                run_se(config, reads)
            }
        }
        ReadLayout::PairedEnd { mates1, mates2 } => {
            if config.combined_index {
                // v2.x opt-in PE combined-index path. The scope guard
                // (`reject_combined_index_unsupported`) restricts --multicore to the
                // default parallel model (a); the low-RAM flags are NON-DIRECTIONAL-only.
                // single_pass (model b) is Bowtie 2-only (tag-RNG); sequential is Bowtie 2
                // OR HISAT2 (v2.x Phase 7 — faithful + aligner-agnostic). As in the SE arm,
                // the low-RAM flags are checked BEFORE the library match (Phase 6):
                //  - single_pass  → ONE PE pass over conversion-tagged interleaved reads
                //    (model b; non-faithful, one index load). Guaranteed non-dir Bowtie 2.
                //  - sequential   → model (a)'s two PE passes run ONE AT A TIME (faithful,
                //    byte-identical to model (a); one index resident at a time). Non-dir
                //    Bowtie 2 or HISAT2 — spawns whichever `config.aligner` resolves.
                //  - else by library: Directional → one both-strands C→T pass → OT/OB;
                //    NonDirectional → two both-strands passes (C→T + G→A) → 4 strands,
                //    parallel model (a); Pbat → one both-strands G→A pass → CTOT/CTOB (the
                //    non-dir G→A half standalone).
                if config.combined_index_single_pass {
                    run_pe_combined_nondir_tagged(config, mates1, mates2)
                } else if config.combined_index_sequential {
                    run_pe_combined_nondir_sequential(config, mates1, mates2)
                } else {
                    match config.library {
                        LibraryType::Directional => run_pe_combined(config, mates1, mates2),
                        LibraryType::NonDirectional => {
                            run_pe_combined_nondir(config, mates1, mates2)
                        }
                        LibraryType::Pbat => run_pe_combined_pbat(config, mates1, mates2),
                    }
                }
            } else if n > 1 {
                parallel::run_pe_multicore(config, mates1, mates2, n)
            } else {
                run_pe(config, mates1, mates2)
            }
        }
    }
}

/// Which bisulfite index a spawned instance reads (`BS_CT` vs `BS_GA`).
#[derive(Clone, Copy)]
enum IndexChoice {
    Ct,
    Ga,
}

/// SE C→T conversion, format-dispatched (FastQ 4-line vs FastA 2-line, Phase 9a).
fn convert_se_ct(
    fasta: bool,
    path: &Path,
    td: &Path,
    opts: &convert::ConvertOptions,
) -> Result<convert::ConvertedReads> {
    if fasta {
        convert::bisulfite_convert_fasta_se(path, td, opts)
    } else {
        convert::bisulfite_convert_fastq_se(path, td, opts)
    }
}

/// SE G→A conversion, format-dispatched.
fn convert_se_ga(
    fasta: bool,
    path: &Path,
    td: &Path,
    opts: &convert::ConvertOptions,
) -> Result<convert::ConvertedReads> {
    if fasta {
        convert::bisulfite_convert_fasta_se_ga(path, td, opts)
    } else {
        convert::bisulfite_convert_fastq_se_ga(path, td, opts)
    }
}

/// Library-aware PE per-mate conversion, format-dispatched (Phase 9a).
fn convert_pe_kind(
    fasta: bool,
    path: &Path,
    td: &Path,
    opts: &convert::ConvertOptions,
    read_number: u8,
    kind: convert::ConvKind,
) -> Result<convert::ConvertedReads> {
    if fasta {
        convert::bisulfite_convert_fasta_pe_kind(path, td, opts, read_number, kind)
    } else {
        convert::bisulfite_convert_fastq_pe_kind(path, td, opts, read_number, kind)
    }
}

/// Convert the per-mode SE temp file(s) (Perl `biTransformFastQFiles` 5489–5651 /
/// `biTransformFastAFiles` 5169–5306): directional = `[C→T]`, pbat = `[G→A]`,
/// non-directional = `[C→T, G→A]` (in that order — the [`se_instance_plan`] file
/// indices key off it). Format-dispatched FastQ vs FastA (Phase 9a).
fn convert_se_files(
    config: &RunConfig,
    read_file: &str,
    opts: &convert::ConvertOptions,
) -> Result<Vec<convert::ConvertedReads>> {
    let path = Path::new(read_file);
    let td = &config.output.temp_dir;
    let fasta = matches!(config.format, ReadFormat::FastA);
    Ok(match config.library {
        LibraryType::Directional => vec![convert_se_ct(fasta, path, td, opts)?],
        LibraryType::Pbat => vec![convert_se_ga(fasta, path, td, opts)?],
        LibraryType::NonDirectional => vec![
            convert_se_ct(fasta, path, td, opts)?, // file 0 = C→T
            convert_se_ga(fasta, path, td, opts)?, // file 1 = G→A
        ],
    })
}

/// The per-mode SE instance plan (Perl `@fhs` templates `reset_counters_and_fhs`
/// 7153–7242 + the input assignment 519–546 + the `--norc`/`--nofw` name rule
/// 6873). Each tuple is `(orientation, index, converted-file-index)` in **Bismark
/// slot order** so the merge's `enumerate` index equals the Perl `@fhs` index.
/// The file index points into [`convert_se_files`]'s output.
fn se_instance_plan(library: LibraryType) -> Vec<(Orientation, IndexChoice, usize)> {
    use IndexChoice::{Ct, Ga};
    use Orientation::{Nofw, Norc};
    match library {
        // directional: s0 CTreadCTgenome (CT/--norc), s1 CTreadGAgenome (GA/--nofw);
        // both read the C→T file. pbat=false; reject gated off.
        LibraryType::Directional => vec![(Norc, Ct, 0), (Nofw, Ga, 0)],
        // pbat: s0 GAreadCTgenome (CT/--nofw), s1 GAreadGAgenome (GA/--norc); both
        // read the G→A file. The +2 index modifier (extraction) lifts slots 0/1 →
        // effective 2/3 (CTOT/CTOB). Orientation FLIPS vs directional.
        LibraryType::Pbat => vec![(Nofw, Ct, 0), (Norc, Ga, 0)],
        // non-dir: s0 CT/--norc & s1 GA/--nofw read the C→T file (idx 0); s2 CT/--nofw
        // & s3 GA/--norc read the G→A file (idx 1). All four kept (no rejection).
        LibraryType::NonDirectional => {
            vec![(Norc, Ct, 0), (Nofw, Ga, 0), (Nofw, Ct, 1), (Norc, Ga, 1)]
        }
    }
}

/// The conversion banner label for a converted temp file (`C->T`/`G->A`), derived
/// from its filename stem. STDERR only (not byte-gated).
fn conv_label(name: &str) -> &'static str {
    if name.contains("_G_to_A") {
        "G->A"
    } else {
        "C->T"
    }
}

/// Process one SE input — a whole read file (single-core / `--parallel 1`) or one
/// contiguous chunk subset (`--parallel N`, Phase 9b): convert the per-mode temp
/// file(s) (1 for directional/pbat, 2 for non-directional), spawn the 2/4 Bowtie 2
/// instances per the [`se_instance_plan`], and drive the lockstep merge into the
/// (already-open) `sinks`, accumulating `counters`. Returns the converted temp
/// file(s) so the caller can clean them up. The report is **not** written here —
/// the caller (`run_se` for N==1, or [`parallel`]'s ordered merge for N>1) owns it.
/// `genome`/`refid` are borrowed read-only (so a Phase-9b worker can share them
/// across `std::thread::scope` without `Arc`).
fn process_se_chunk(
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    input: &Path,
    opts: &convert::ConvertOptions,
    sinks: &mut Sinks,
    counters: &mut Counters,
) -> Result<Vec<convert::ConvertedReads>> {
    let bt2 = &config.detected_aligner.path;
    let read_file = input.to_string_lossy();
    // Phase 2/8: convert the per-mode temp file(s). Both/all instances read this set.
    let converted = convert_se_files(config, &read_file, opts)?;
    for cr in &converted {
        eprintln!(
            "Created {} converted version of {read_file} -> {} ({} sequences)",
            conv_label(&cr.name),
            cr.path.display(),
            cr.count
        );
    }
    // Phase 2 (epic 06152026; #995): when the in-process rammap backend is compiled in
    // and selected (SE FastQ `--rammap_inprocess`, not force-subprocessed), drive the
    // merge over `Arc<rammap::Aligner>`-backed streams (each converted `.mmi` loaded
    // ONCE, shared across the 2/4 instances) instead of spawning 2/4 rammap subprocesses.
    // The streams map their reads in PARALLEL on a shared pool (sized by `--multicore`,
    // #995); `drive_merge` is generic over `SamStream`; the merge / XM / output arm is
    // byte-frozen. Reached only via the single-process `run_se` (the `pipeline()`
    // short-circuit routes `--rammap_inprocess` here for ANY `--multicore N`). Only
    // FastA falls through to the subprocess path (the stream is FastQ-only; see
    // `use_se_inprocess_rammap` + the never-silent notice in `run`).
    #[cfg(feature = "rammap-inprocess")]
    if use_se_inprocess_rammap(config) {
        let pbat = matches!(config.library, LibraryType::Pbat);
        let mut streams = build_se_inprocess_streams(config, &converted)?;
        drive_merge(
            input,
            &mut streams,
            config,
            genome,
            refid,
            pbat,
            sinks,
            counters,
        )?;
        // In-process streams own no subprocess — nothing to `finish()`; they drop here.
        return Ok(converted);
    }

    // Phase 3/8: spawn the instances per the per-mode plan, in Bismark slot order
    // so the merge's `enumerate` index == the Perl `@fhs` index.
    let mut streams = Vec::with_capacity(2);
    for (orientation, index_choice, file_idx) in se_instance_plan(config.library) {
        let index_basename = match index_choice {
            IndexChoice::Ct => &config.genome.ct_index_basename,
            IndexChoice::Ga => &config.genome.ga_index_basename,
        };
        streams.push(AlignerStream::spawn(
            config.aligner,
            bt2,
            &config.aligner_options,
            orientation,
            index_basename,
            &converted[file_idx].path,
        )?);
    }

    // Phase 4–6: drive the merge, routing each read to its sink.
    let pbat = matches!(config.library, LibraryType::Pbat);
    drive_merge(
        input,
        &mut streams,
        config,
        genome,
        refid,
        pbat,
        sinks,
        counters,
    )?;
    for s in streams {
        s.finish()?;
    }
    Ok(converted)
}

/// Whether the SE in-process rammap path drives this run (epic 06152026; #995).
///
/// Unconditional + `cfg!`-gated (NOT `#[cfg]`-gated) on purpose, so it (a) returns
/// `false` on the feature-off build, (b) is callable from `run`'s backend notice AND the
/// `pipeline()` routing on BOTH builds — reusing ONE predicate so the printed backend, the
/// routing, and the path actually taken can never disagree, and (c) reads
/// `config.rammap_inprocess` in always-compiled code so the field is never a feature-off
/// dead field (`-D warnings`).
///
/// **Phase 4 (Option A): `--rammap` defaults to the SUBPROCESS path; the in-process path is
/// the explicit `--rammap_inprocess` OPT-IN.** Taken iff `config.rammap_inprocess` (+ rammap
/// + FastQ + feature-on).
///
/// **#995: NO `multicore` gate.** The in-process path is now N-threaded (a shared rayon pool
/// sized by `--multicore`), so it runs for ANY N. A parallel FORK worker reaching this branch
/// is prevented by TWO independent mechanisms: (1) `pipeline()` short-circuits
/// `--rammap_inprocess` to the single-process `run_se` (reusing THIS predicate), so
/// `parallel::run_se_multicore`/`se_chunk_job` is never invoked for it; (2) the
/// `aligner == Rammap` conjunct — a fork worker only runs non-rammap engines, so even if
/// reached it returns `false`. FastQ-only because the `InProcessAlignerStream` reads 4-line
/// FastQ (FastA falls back to subprocess).
fn use_se_inprocess_rammap(config: &RunConfig) -> bool {
    inprocess_rammap_selected(config.aligner, config.rammap_inprocess, config.format)
}

/// The pure selection logic behind [`use_se_inprocess_rammap`], over primitives so it
/// is unit-testable without a full `RunConfig`. `cfg!(feature = "rammap-inprocess")` is
/// the first conjunct → always `false` on the feature-off build (so `--rammap_inprocess`
/// is inert there — the in-process path isn't compiled). No `multicore` term (#995: the
/// in-process path is N-threaded; fork-worker safety is via the `pipeline()` short-circuit
/// + the `aligner == Rammap` conjunct — see [`use_se_inprocess_rammap`]).
fn inprocess_rammap_selected(aligner: Aligner, inprocess_opt_in: bool, format: ReadFormat) -> bool {
    cfg!(feature = "rammap-inprocess")
        && aligner == Aligner::Rammap
        && inprocess_opt_in
        && matches!(format, ReadFormat::FastQ)
}

/// Build the SE in-process rammap streams (epic 06152026 Phase 2): load each converted
/// `.mmi` the per-mode [`se_instance_plan`] references EXACTLY ONCE into an
/// `Arc<rammap::Aligner>` and `Arc::clone` it into one [`InProcessAlignerStream`] per
/// instance — in Bismark slot order, so the merge's `enumerate` index matches the
/// subprocess path. Each stream reads the SAME converted temp bytes the subprocess CLI
/// reads (with a `.gz` sniff). Orientation is ignored (rammap takes no `--norc`/`--nofw`;
/// strand is classified by instance index in the merge).
///
/// All three library types reference BOTH indexes (directional: OT→CT + CTOB→GA; pbat:
/// CTOT→CT + CTOB→GA; non-directional: all four), so this loads two indexes for every
/// library — the EPIC's "construct 2 `Arc<Aligner>`". The per-`IndexChoice` gating below
/// is future-proofing (load only what the plan references), NOT an RSS optimisation for
/// any current library (PLAN rev 1, correcting the plan-review premise that directional
/// needs only CT).
#[cfg(feature = "rammap-inprocess")]
fn build_se_inprocess_streams(
    config: &RunConfig,
    converted: &[convert::ConvertedReads],
) -> Result<Vec<crate::inprocess::InProcessAlignerStream<Box<dyn BufRead>>>> {
    use std::sync::Arc;

    let plan = se_instance_plan(config.library);

    // Load each `.mmi` the plan references, exactly once. `from_index` takes a `&str`
    // path (cf. the Phase-1 cross-check); map non-UTF-8 / load failure to a validation
    // error (fail-loud, never-silent).
    let load = |basename: &Path| -> Result<Arc<::rammap::Aligner>> {
        let mut mmi = basename.as_os_str().to_owned();
        mmi.push(".mmi");
        let mmi_str = mmi.to_str().ok_or_else(|| {
            AlignerError::Validation(format!(
                "rammap index path is not valid UTF-8: {}",
                Path::new(&mmi).display()
            ))
        })?;
        let aligner =
            ::rammap::Aligner::from_index(mmi_str, ::rammap::Preset::MapOnt).map_err(|e| {
                AlignerError::Validation(format!("failed to load rammap index {mmi_str}: {e}"))
            })?;
        Ok(Arc::new(aligner))
    };

    let needs_ct = plan.iter().any(|(_, ic, _)| matches!(ic, IndexChoice::Ct));
    let needs_ga = plan.iter().any(|(_, ic, _)| matches!(ic, IndexChoice::Ga));
    let ct = if needs_ct {
        Some(load(&config.genome.ct_index_basename)?)
    } else {
        None
    };
    let ga = if needs_ga {
        Some(load(&config.genome.ga_index_basename)?)
    } else {
        None
    };

    // ONE shared rayon pool, sized by --multicore (#995), `Arc::clone`d into EVERY stream
    // (NOT one pool per stream): the lockstep merge drains+refills one stream at a time, so
    // refills never overlap → a shared pool of N gives the same throughput with N total
    // threads, not instances×N. N == 1 (the default) → a 1-thread pool == the former
    // one-at-a-time path (byte-identical output).
    let threads = config.multicore.max(1) as usize;
    let pool = Arc::new(
        ::rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|e| {
                AlignerError::Validation(format!("failed to build rammap thread pool: {e}"))
            })?,
    );

    let mut streams = Vec::with_capacity(plan.len());
    for (_orientation, index_choice, file_idx) in plan {
        let aligner = match index_choice {
            IndexChoice::Ct => Arc::clone(
                ct.as_ref()
                    .expect("CT index loaded because the plan references it"),
            ),
            IndexChoice::Ga => Arc::clone(
                ga.as_ref()
                    .expect("GA index loaded because the plan references it"),
            ),
        };
        // The converted temp the subprocess CLI would have read (`.gz` when `--gzip`).
        let path = &converted[file_idx].path;
        let f = File::open(path)?;
        let reader: Box<dyn BufRead> = if path.to_string_lossy().ends_with(".gz") {
            Box::new(BufReader::new(MultiGzDecoder::new(f)))
        } else {
            Box::new(BufReader::new(f))
        };
        streams.push(crate::inprocess::InProcessAlignerStream::new(
            aligner,
            reader,
            Arc::clone(&pool),
        )?);
    }
    Ok(streams)
}

/// SE pipeline (single-core / `--parallel 1`, all library types): load the genome
/// once, then per read file open the sinks + report header, run [`process_se_chunk`]
/// against the whole file, write the final analysis + wall-clock line, finalise the
/// sinks, and clean up the converted temp file(s). (The `--parallel N` path lives in
/// [`parallel::run_se_multicore`], which fans [`process_se_chunk`] over contiguous
/// chunks and merges in order.)
fn run_se(config: &RunConfig, reads: &[String]) -> Result<()> {
    let started = Instant::now();
    let opts = convert::ConvertOptions::from_config(config);

    // Load the raw genome once (Perl 273–277), consuming Phase 1's ordered FASTA
    // list — the single source of truth for the `@SQ` order.
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    // The header is identical for every read file (Bismark `@PG` reconstructed
    // from argv; samtools `@PG` normalised out per gate policy P1).
    let header = generate_sam_header(&genome, &config.command_line);
    let directional = matches!(config.library, LibraryType::Directional);
    // The report's genome path is the absolute path WITH a trailing `/` (Perl
    // forces it, 7619–7629); `genome_dir` is absolute (canonicalize) but slashless.
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    // The `_bismark_<token>` output-name token (Perl `_bismark_bt2`/`_bismark_hisat2`),
    // threaded ONLY into the derived-name (`default_suffix`) path — never the
    // `--basename` / `_unmapped` / `_ambiguous` names (no token in Perl).
    let tok = config.aligner.token();

    for read_file in reads {
        // Open the BAM + optional --ambig_bam / --unmapped / --ambiguous sinks.
        let bam_path =
            derive_output_path(read_file, config, &format!("_bismark_{tok}.bam"), ".bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_sinks(read_file, config, &header, &bam_path)?;

        // Open + write the alignment report header (Perl 1641–1729).
        let report_path = derive_output_path(
            read_file,
            config,
            &format!("_bismark_{tok}_SE_report.txt"),
            "_SE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_file,
                sequence_file2: None,
                genome_folder: &genome_folder,
                aligner_options: &config.aligner_options,
                aligner: config.aligner,
                library: config.library,
            },
        )?;

        let mut counters = Counters::default();
        let converted = process_se_chunk(
            config,
            &genome,
            &refid,
            Path::new(read_file),
            &opts,
            &mut sinks,
            &mut counters,
        )?;

        // Final analysis + the trailing wall-clock line (Perl 1964–2144 + 926–927).
        report::print_final_analysis_report_single_end(&mut report, &counters, directional)?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;

        sinks.finish()?;

        // Per-mode temp cleanup (rev1 A): delete EVERY converted temp file for this
        // read — 1 for directional/pbat, 2 (C→T + G→A) for non-directional. Byte-
        // invisible, so no gate/diff catches an omission. Best-effort (Perl warns,
        // never dies, 1974–1981).
        for cr in &converted {
            let _ = std::fs::remove_file(&cr.path);
        }

        eprintln!("{}", counters_summary(read_file, &counters));
    }
    Ok(())
}

// ===========================================================================
// #787 Illumina 5-Base single-end driver. Opt-in, never-silent, concordance-gated
// (NOT byte-identical — Perl Bismark has no 5-Base oracle). Unlike the bisulfite
// spine it does NOT convert the reads: it aligns the RAW reads to the UNCONVERTED
// genome with ONE minimap2 instance, derives the strand from the SAM FLAG, and
// reuses the byte-frozen genomic-extraction + (polarity-INVERTED) methylation_call
// + single_end_sam_output. v1 = single-end + directional (guarded at resolve()).
// ===========================================================================

/// The aligner option string for a 5-Base run. minimap2 (the default) uses the
/// resolved `-x sr …` options against the genome FASTA. bowtie2/hisat2 align the RAW
/// reads to the user's NORMAL (unconverted) index with a permissive `--score-min`
/// (the sparse C→T conversions are well within default scoring) — NOT the bisulfite
/// option string. Used for BOTH the spawn argv and the report's "was run with" line.
///
/// Threads: the bisulfite minimap2 path hardcodes `-t 2` (Perl faithfulness, byte-
/// frozen). The 5-Base path is NOT byte-identical, so there is no reason to throttle
/// it. The aligner thread count is Bismark's `-p` (threads-to-aligner) knob, falling
/// back to `--multicore`, then all logical CPUs — applied as minimap2 `-t N` /
/// bowtie2,hisat2 `-p N`. (`-p` and `--multicore`/`--parallel` are different axes:
/// `-p` = threads inside one aligner instance, `--multicore` = the fork model; for the
/// single-instance 5-Base path `-p` is the right knob.)
fn five_base_aligner_options(config: &RunConfig) -> String {
    let n = config
        .bowtie_threads
        .filter(|&p| p > 0)
        .or(if config.multicore > 1 {
            Some(config.multicore)
        } else {
            None
        })
        .map(|p| p as usize)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|x| x.get())
                .unwrap_or(2)
        });
    match config.aligner {
        Aligner::Bowtie2 => format!("-q --score-min L,0,-0.6 -p {n}"),
        Aligner::Hisat2 => format!("-q --no-spliced-alignment --score-min L,0,-0.6 -p {n}"),
        // minimap2 (default 5-Base engine): reuse the resolved options but lift the
        // faithful `-t 2` to `-t {n}` (all cores) for this non-byte-identical path.
        _ => config.aligner_options.replace("-t 2", &format!("-t {n}")),
    }
}

/// Build the per-engine spawn argv for a 5-Base run. minimap2: `<opts> <genome.fa>
/// <reads…>` (positional FASTA + reads). bowtie2/hisat2: `<opts> -x <index> {-U r |
/// -1 r1 -2 r2}` against the user's unconverted index. `reads` is 1 (SE) or 2 (PE).
fn five_base_build_argv(
    config: &RunConfig,
    opts: &str,
    ref_path: &Path,
    reads: &[&Path],
) -> Vec<std::ffi::OsString> {
    use std::ffi::OsString;
    let mut args: Vec<OsString> = opts.split_whitespace().map(OsString::from).collect();
    match config.aligner {
        Aligner::Bowtie2 | Aligner::Hisat2 => {
            let idx = config
                .five_base_index
                .as_ref()
                .expect("five_base_index is required for bowtie2/hisat2 5-Base (guarded)");
            args.push("-x".into());
            args.push(idx.as_os_str().to_owned());
            if reads.len() == 2 {
                args.push("-1".into());
                args.push(reads[0].as_os_str().to_owned());
                args.push("-2".into());
                args.push(reads[1].as_os_str().to_owned());
            } else {
                args.push("-U".into());
                args.push(reads[0].as_os_str().to_owned());
            }
        }
        _ => {
            // minimap2: positional <genome.fa> then the read file(s).
            args.push(ref_path.as_os_str().to_owned());
            for r in reads {
                args.push(r.as_os_str().to_owned());
            }
        }
    }
    args
}

/// Drive the 5-Base SE run: one aligner instance per read file against the
/// unconverted reference/index, lockstep with the original FastQ, emitting a Bismark
/// BAM and SE report. Mirrors `run_se`'s setup (genome / header / sinks / report) but
/// swaps the convert+2-instance+merge core for [`five_base_align_and_call`].
fn run_se_five_base(config: &RunConfig, reads: &[String]) -> Result<()> {
    let started = Instant::now();
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    let tok = config.aligner.token(); // minimap2 → "mm2"
    let fb_opts = five_base_aligner_options(config); // engine option string (report + spawn)
    // The single reference FASTA minimap2 aligns against (the UNCONVERTED genome).
    let (ref_path, ref_tmp) = five_base_reference_fasta(config)?;

    for read_file in reads {
        let bam_path =
            derive_output_path(read_file, config, &format!("_bismark_{tok}.bam"), ".bam");
        eprintln!(
            ">>> Writing 5-Base mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_sinks(read_file, config, &header, &bam_path)?;

        let report_path = derive_output_path(
            read_file,
            config,
            &format!("_bismark_{tok}_SE_report.txt"),
            "_SE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_file,
                sequence_file2: None,
                genome_folder: &genome_folder,
                aligner_options: &fb_opts,
                aligner: config.aligner,
                library: config.library,
            },
        )?;

        let mut counters = Counters::default();
        five_base_align_and_call(
            config,
            &genome,
            &refid,
            Path::new(read_file),
            &ref_path,
            &mut sinks,
            &mut counters,
        )?;

        // 5-Base is directional (the report's strand block is OT/OB only).
        report::print_final_analysis_report_single_end(&mut report, &counters, true)?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;
        sinks.finish()?;
        eprintln!("{}", counters_summary(read_file, &counters));

        // #787 optional variant/methylation deconvolution over the just-written BAM.
        if config.five_base_deconvolution {
            let deconv_path = derive_output_path(
                read_file,
                config,
                &format!("_bismark_{tok}.5base_deconvolution.txt"),
                ".5base_deconvolution.txt",
            );
            run_five_base_deconvolution(&genome, &bam_path, &deconv_path)?;
        }

        // #787 optional duplex-consensus family pass over the just-written BAM.
        if config.five_base_duplex {
            let duplex_path = derive_output_path(
                read_file,
                config,
                &format!("_bismark_{tok}.5base_duplex.txt"),
                ".5base_duplex.txt",
            );
            run_five_base_duplex(&genome, &bam_path, &duplex_path, config.five_base_umi_len)?;
        }

        // #787 optional duplex-consensus COLLAPSE → one consensus read per family.
        if config.five_base_consensus {
            let consensus_path = derive_output_path(
                read_file,
                config,
                &format!("_bismark_{tok}.5base_consensus.bam"),
                ".5base_consensus.bam",
            );
            run_five_base_consensus(
                &genome,
                &refid,
                &bam_path,
                &consensus_path,
                &header,
                config.five_base_umi_len,
            )?;
        }
    }

    // Delete the concatenated-reference temp, if one was written (multi-FASTA genome).
    if let Some(tmp) = ref_tmp {
        let _ = std::fs::remove_file(tmp);
    }
    Ok(())
}

/// Resolve the reference FASTA minimap2 will align against. A single-FASTA genome
/// is passed through directly (no copy); a multi-FASTA genome is decompressed +
/// concatenated once into a temp plain FASTA (so minimap2 sees one file, with the
/// same contig names — hence the same RNAMEs the in-memory `genome`/`refid` use).
/// Returns `(reference_path, Some(temp_to_delete))` for the multi-FASTA case.
fn five_base_reference_fasta(config: &RunConfig) -> Result<(PathBuf, Option<PathBuf>)> {
    let fastas = &config.genome.fastas;
    if fastas.len() == 1 {
        return Ok((fastas[0].clone(), None));
    }
    let tmp = config
        .output
        .output_dir
        .join(".bismark_5base_concat_ref.fa");
    let mut out = BufWriter::new(File::create(&tmp)?);
    for f in fastas {
        let file = File::open(f)?;
        let mut r: Box<dyn BufRead> = if f.to_string_lossy().ends_with(".gz") {
            Box::new(BufReader::new(MultiGzDecoder::new(file)))
        } else {
            Box::new(BufReader::new(file))
        };
        std::io::copy(&mut r, &mut out)?;
        out.write_all(b"\n")?; // guard against a missing trailing newline between files
    }
    out.flush()?;
    Ok((tmp.clone(), Some(tmp)))
}

/// Spawn ONE minimap2 against `ref_path` (unconverted) with the RAW reads, then walk
/// the original FastQ in lockstep with minimap2's primary SAM records (input order),
/// emitting one Bismark record per read. `skip`/`upto` mirror `drive_merge`.
fn five_base_align_and_call(
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    read_file: &Path,
    ref_path: &Path,
    sinks: &mut Sinks,
    counters: &mut Counters,
) -> Result<()> {
    // Per-engine argv against the UNCONVERTED reference/index (no read conversion):
    // minimap2 reads the FASTA directly, bowtie2/hisat2 use the user's normal index.
    let bin = &config.detected_aligner.path;
    let opts = five_base_aligner_options(config);
    let args = five_base_build_argv(config, &opts, ref_path, &[read_file]);
    let mut child = std::process::Command::new(bin)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map_err(|e| {
            AlignerError::Validation(format!(
                "failed to spawn {} ({}): {e}",
                config.aligner.name(),
                bin.display()
            ))
        })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AlignerError::Validation("minimap2 stdout was not captured".into()))?;
    let mut sam = BufReader::new(stdout);
    let mut sam_line = String::new();

    // Walk the original FastQ (4-line records) in lockstep with minimap2's primary
    // records. minimap2 emits output in input order (one primary per read), so the
    // pairing is positional; a qname mismatch is a hard desync error (never a silent
    // miscall). `--secondary=no` + skipping supplementary keeps it 1:1.
    let file = File::open(read_file)?;
    let mut reader: Box<dyn BufRead> = if read_file.to_string_lossy().ends_with(".gz") {
        Box::new(BufReader::new(MultiGzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };
    // 5-Base forces whitespace-truncated qnames (the aligner truncates), so `icpc` is unused.
    let (skip, upto) = (config.read_processing.skip, config.read_processing.upto);
    let (mut id, mut seq, mut plus, mut qual) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut count: u64 = 0;
    // #787 UMI dedup: drop reads sharing (UMI, chrom, pos, strand). 0 ⇒ off.
    let umi_len = config.five_base_umi_len;
    let mut seen: std::collections::HashSet<(Vec<u8>, String, u32, u16)> =
        std::collections::HashSet::new();
    let mut dups: u64 = 0;
    loop {
        id.clear();
        seq.clear();
        plus.clear();
        qual.clear();
        let n1 = reader.read_until(b'\n', &mut id)?;
        let n2 = reader.read_until(b'\n', &mut seq)?;
        let n3 = reader.read_until(b'\n', &mut plus)?;
        let n4 = reader.read_until(b'\n', &mut qual)?;
        if n1 == 0 || n2 == 0 || n3 == 0 || n4 == 0 {
            break;
        }
        count += 1;
        if let Some(s) = skip
            && s > 0
            && count <= s
        {
            // skipped reads still consume their SAM primary to stay in lockstep.
            let _ = five_base_next_primary(&mut sam, &mut sam_line)?;
            continue;
        }
        if let Some(u) = upto
            && u > 0
            && count > u
        {
            break;
        }

        // #787: the 5-Base path hands the RAW FastQ to the aligner, which truncates the
        // SAM QNAME at the first whitespace (SAM forbids spaces) — so a real Illumina
        // header `@<name> 1:N:0:<index>` becomes just `<name>` in minimap2's output.
        // Force whitespace truncation (icpc semantics) regardless of `--icpc` so the
        // lockstep qname check matches; underscoring the comment would desync every read.
        let fixed = convert::fix_id(convert::chomp_newline(&id), true);
        let id_bytes = fixed.strip_prefix(b"@").unwrap_or(&fixed);
        let identifier = String::from_utf8_lossy(id_bytes).into_owned();
        let seq_uc: Vec<u8> = convert::chomp_newline(&seq).to_ascii_uppercase();
        let qual_bytes: Vec<u8> = convert::chomp_newline(&qual).to_vec();

        let rec = five_base_next_primary(&mut sam, &mut sam_line)?.ok_or_else(|| {
            AlignerError::Validation(format!(
                "minimap2 produced fewer records than reads (desync at read {identifier})"
            ))
        })?;
        if rec.qname != identifier {
            return Err(AlignerError::Validation(format!(
                "5-Base SAM/FastQ desync: expected read {identifier}, minimap2 emitted {}",
                rec.qname
            )));
        }

        // UMI dedup (mapped reads only): drop a read whose (UMI, chrom, pos, strand)
        // was already seen (a PCR/optical duplicate); the first survives.
        if umi_len > 0 && rec.flag & 0x4 == 0 {
            let umi: Vec<u8> = seq_uc.iter().take(umi_len).copied().collect();
            if !seen.insert((umi, rec.rname.clone(), rec.pos, rec.flag & 0x10)) {
                dups += 1;
                continue;
            }
        }
        counters.sequences_count += 1;
        if let Some(mut record) = five_base_emit_record(
            &rec,
            &identifier,
            &seq_uc,
            &qual_bytes,
            genome,
            refid,
            config.phred64,
            config.five_base_baseq,
            counters,
        )? {
            // #787 duplex: carry the raw UMI to the BAM (RX:Z:) so the duplex pass
            // that re-reads the BAM can key families by it. Only when UMIs are in use.
            if umi_len > 0 {
                let umi: Vec<u8> = seq_uc.iter().take(umi_len).copied().collect();
                record.set_rx(&umi);
            }
            write_record(&mut sinks.bam, &record)?;
        } else if rec.flag & 0x4 != 0 {
            // unmapped → optional --unmapped FastQ (mirrors Decision::NoAlignment).
            if let Some(w) = sinks.unmapped.as_mut() {
                let seq_orig = convert::chomp_newline(&seq).to_vec();
                write_se_aux_record(
                    w,
                    false,
                    identifier.as_bytes(),
                    &seq_orig,
                    &plus,
                    &qual_bytes,
                )?;
            }
        }
    }

    let status = child.wait()?;
    if !status.success() {
        return Err(AlignerError::Validation(format!(
            "minimap2 exited with status {status}"
        )));
    }
    if umi_len > 0 {
        eprintln!("5-Base UMI dedup (umi_len {umi_len}): removed {dups} duplicate read(s).");
    }
    Ok(())
}

/// Read the next PRIMARY SAM record from minimap2's stdout, skipping `@` headers
/// and secondary (0x100) / supplementary (0x800) lines. `None` at EOF.
fn five_base_next_primary<R: BufRead>(
    reader: &mut R,
    line: &mut String,
) -> Result<Option<SamRecord>> {
    loop {
        line.clear();
        let n = reader.read_line(line)?;
        if n == 0 {
            return Ok(None);
        }
        if line.starts_with('@') {
            continue;
        }
        let rec = SamRecord::parse(line)?;
        if rec.flag & 0x100 != 0 || rec.flag & 0x800 != 0 {
            continue; // secondary / supplementary — not the primary for this read
        }
        return Ok(Some(rec));
    }
}

/// Mask read bases below the Phred base-quality threshold to `b'N'` for the methylation
/// call ONLY (the BAM SEQ keeps the original read). `baseq == 0` ⇒ no masking (returns a
/// plain copy). `offset` is 33 (Sanger/Phred+33) or 64 (`--phred64`). An `N` matches no
/// genomic base, so `methylation_call` emits a no-call (`.`) there, cutting the per-base
/// sequencing-error noise floor (#787 real-data finding; DRAGEN `--methylation-baseq`).
fn mask_low_quality(seq: &[u8], qual: &[u8], baseq: u8, offset: u8) -> Vec<u8> {
    if baseq == 0 {
        return seq.to_vec();
    }
    let min = offset.saturating_add(baseq);
    seq.iter()
        .enumerate()
        .map(|(i, &b)| match qual.get(i) {
            Some(&q) if q < min => b'N',
            _ => b,
        })
        .collect()
}

/// Turn one minimap2 primary [`SamRecord`] + the original read into a Bismark
/// record with INVERTED (5-Base) methylation polarity, or `None` (unmapped, or the
/// chromosome-edge length guard fired). Pure (no I/O) so it is unit-testable with a
/// canned `SamRecord` + tiny in-memory genome. Increments the alignment-outcome and
/// (via `methylation_call`/extraction) the methylation/strand counters.
#[allow(clippy::too_many_arguments)]
fn five_base_emit_record(
    rec: &SamRecord,
    identifier: &str,
    seq_uc: &[u8],
    qual_bytes: &[u8],
    genome: &Genome,
    refid: &HashMap<String, usize>,
    phred64: bool,
    baseq: u8,
    counters: &mut Counters,
) -> Result<Option<bismark_io::BismarkRecord>> {
    if rec.flag & 0x4 != 0 {
        counters.no_single_alignment_found += 1;
        return Ok(None);
    }
    counters.unique_best_alignment_count += 1;
    // Strand from the FLAG: forward (0) → OT (index 0); reverse (0x10) → OB (index 1).
    // Directional only in v1, so only the two original strands occur.
    let index = if rec.flag & 0x10 != 0 { 1 } else { 0 };
    let best = BestAlignment {
        chromosome: rec.rname.clone(), // unconverted genome → no _CT_/_GA_ suffix
        position: rec.pos,
        index,
        alignment_score: rec.alignment_score.unwrap_or(0),
        alignment_score_second_best: rec.second_best,
        md_tag: rec.md_tag.clone().unwrap_or_default(),
        cigar: rec.cigar.clone(),
        bowtie_sequence: rec.seq.clone(),
        mapq: rec.mapq,
    };
    let ext = extract_corresponding_genomic_sequence_single_end(&best, genome, false, counters)?;
    // Length guard (Perl 3127): the window must be read_len + 2; a shorter one means
    // a chromosome-edge guard fired → skip (counted, not written).
    if ext.unmodified_genomic_sequence.len() != seq_uc.len() + 2 {
        counters.genomic_sequence_could_not_be_extracted_count += 1;
        return Ok(None);
    }
    // THE inversion: five_base = true (5mC->T, the chemical inverse of bisulfite).
    // Base-quality masking applies to the CALL ONLY; the BAM SEQ keeps the original read.
    let call_seq = mask_low_quality(seq_uc, qual_bytes, baseq, if phred64 { 64 } else { 33 });
    let methcall = methylation_call(
        &call_seq,
        &ext.unmodified_genomic_sequence,
        ext.read_conversion,
        true,
        counters,
    );
    let record = single_end_sam_output(
        identifier, seq_uc, qual_bytes, &best, &ext, &methcall, refid, phred64,
    )?;
    Ok(Some(record))
}

// ===========================================================================
// #787 Illumina 5-Base PAIRED-END driver. Same model as the SE path, but minimap2
// is run in PE mode (`<opts> ref.fa r1.fq r2.fq`) against the unconverted genome;
// the PE index (0 OT / 3 OB — directional only) comes from R1's strand, and the
// byte-frozen PE extract + paired_end_sam_output are reused with the inverted call.
// ===========================================================================

/// Drive the 5-Base PE run: one minimap2 PE instance per mate-pair file against the
/// unconverted reference, lockstep with the original FastQ pair, emitting a Bismark
/// `_pe.bam` + PE report. Mirrors `run_pe`'s setup.
fn run_pe_five_base(config: &RunConfig, mates1: &[String], mates2: &[String]) -> Result<()> {
    let started = Instant::now();
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    let tok = config.aligner.token();
    let fb_opts = five_base_aligner_options(config);
    let (ref_path, ref_tmp) = five_base_reference_fasta(config)?;

    for (read_1, read_2) in mates1.iter().zip(mates2) {
        let bam_path =
            derive_output_path(read_1, config, &format!("_bismark_{tok}_pe.bam"), "_pe.bam");
        eprintln!(
            ">>> Writing 5-Base mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_pe_sinks(read_1, read_2, config, &header, &bam_path)?;

        let report_path = derive_output_path(
            read_1,
            config,
            &format!("_bismark_{tok}_PE_report.txt"),
            "_PE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_1,
                sequence_file2: Some(read_2),
                genome_folder: &genome_folder,
                aligner_options: &fb_opts,
                aligner: config.aligner,
                library: config.library,
            },
        )?;

        let mut counters = Counters::default();
        five_base_align_and_call_pe(
            config,
            &genome,
            &refid,
            Path::new(read_1),
            Path::new(read_2),
            &ref_path,
            &mut sinks,
            &mut counters,
        )?;

        report::print_final_analysis_report_paired_ends(&mut report, &counters, true)?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;
        sinks.finish()?;
        eprintln!("{}", counters_summary_pe(read_1, read_2, &counters));

        // #787 optional variant/methylation deconvolution over the just-written BAM.
        if config.five_base_deconvolution {
            let deconv_path = derive_output_path(
                read_1,
                config,
                &format!("_bismark_{tok}_pe.5base_deconvolution.txt"),
                "_pe.5base_deconvolution.txt",
            );
            run_five_base_deconvolution(&genome, &bam_path, &deconv_path)?;
        }
    }

    if let Some(tmp) = ref_tmp {
        let _ = std::fs::remove_file(tmp);
    }
    Ok(())
}

/// Spawn ONE minimap2 PE instance (`<opts> ref.fa r1.fq r2.fq`) and walk both FastQ
/// files in lockstep with minimap2's primary records (two per pair, in input order).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn five_base_align_and_call_pe(
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    read_1: &Path,
    read_2: &Path,
    ref_path: &Path,
    sinks: &mut PeSinks,
    counters: &mut Counters,
) -> Result<()> {
    let bin = &config.detected_aligner.path;
    let opts = five_base_aligner_options(config);
    let args = five_base_build_argv(config, &opts, ref_path, &[read_1, read_2]);
    let mut child = std::process::Command::new(bin)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map_err(|e| {
            AlignerError::Validation(format!(
                "failed to spawn {} ({}): {e}",
                config.aligner.name(),
                bin.display()
            ))
        })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AlignerError::Validation("minimap2 stdout was not captured".into()))?;
    let mut sam = BufReader::new(stdout);
    let mut sam_line = String::new();

    let mut r1 = open_maybe_gz(read_1)?;
    let mut r2 = open_maybe_gz(read_2)?;
    let (skip, upto) = (config.read_processing.skip, config.read_processing.upto);
    let mut count: u64 = 0;
    // #787 UMI dedup (PE): key on both mates' UMIs + the R1 chrom/pos/strand.
    let umi_len = config.five_base_umi_len;
    let mut seen: std::collections::HashSet<(Vec<u8>, Vec<u8>, String, u32, u16)> =
        std::collections::HashSet::new();
    let mut dups: u64 = 0;
    while let Some((id1, seq1, _p1, qual1)) = read_fastq_record(&mut r1)? {
        let Some((_id2, seq2, _p2, qual2)) = read_fastq_record(&mut r2)? else {
            break;
        };
        count += 1;
        // Each pair always yields TWO minimap2 primaries (mapped or unmapped); consume
        // them to stay in lockstep even for skipped reads.
        let a = five_base_next_primary(&mut sam, &mut sam_line)?;
        let b = five_base_next_primary(&mut sam, &mut sam_line)?;
        if let Some(s) = skip
            && s > 0
            && count <= s
        {
            continue;
        }
        if let Some(u) = upto
            && u > 0
            && count > u
        {
            break;
        }
        let (Some(a), Some(b)) = (a, b) else {
            return Err(AlignerError::Validation(
                "minimap2 produced fewer PE records than read pairs (desync)".into(),
            ));
        };
        // Identify R1 (FLAG 0x40) and R2 (FLAG 0x80) within the pair.
        let (rec1, rec2) = if a.flag & 0x40 != 0 {
            (&a, &b)
        } else {
            (&b, &a)
        };

        // #787: force whitespace truncation (see the SE path) — minimap2 PE truncates the
        // QNAME at the first space, so the real Illumina header's `1:N:0:` comment must be
        // dropped here too or every pair desyncs.
        let fixed = convert::fix_id(convert::chomp_newline(&id1), true);
        let id_bytes = fixed.strip_prefix(b"@").unwrap_or(&fixed);
        let identifier = strip_mate_suffix(&String::from_utf8_lossy(id_bytes));
        let seq1_uc: Vec<u8> = convert::chomp_newline(&seq1).to_ascii_uppercase();
        let seq2_uc: Vec<u8> = convert::chomp_newline(&seq2).to_ascii_uppercase();
        let qual1_bytes: Vec<u8> = convert::chomp_newline(&qual1).to_vec();
        let qual2_bytes: Vec<u8> = convert::chomp_newline(&qual2).to_vec();

        // UMI dedup (proper pairs only): drop a pair whose (R1 UMI, R2 UMI, chrom,
        // R1 pos, R1 strand) was already seen.
        if umi_len > 0 && rec1.flag & 0x2 != 0 && rec1.flag & 0x4 == 0 {
            let u1: Vec<u8> = seq1_uc.iter().take(umi_len).copied().collect();
            let u2: Vec<u8> = seq2_uc.iter().take(umi_len).copied().collect();
            if !seen.insert((u1, u2, rec1.rname.clone(), rec1.pos, rec1.flag & 0x10)) {
                dups += 1;
                continue;
            }
        }
        counters.sequences_count += 1;
        if let Some((mut out1, mut out2)) = five_base_emit_pe_record(
            rec1,
            rec2,
            &identifier,
            &seq1_uc,
            &qual1_bytes,
            &seq2_uc,
            &qual2_bytes,
            genome,
            refid,
            config.phred64,
            config.dovetail,
            config.five_base_baseq,
            counters,
        )? {
            // #787 carry each mate's raw UMI (RX:Z:) for inspectability / future PE duplex.
            if umi_len > 0 {
                out1.set_rx(&seq1_uc.iter().take(umi_len).copied().collect::<Vec<u8>>());
                out2.set_rx(&seq2_uc.iter().take(umi_len).copied().collect::<Vec<u8>>());
            }
            write_record(&mut sinks.bam, &out1)?;
            write_record(&mut sinks.bam, &out2)?;
        }
    }

    let status = child.wait()?;
    if !status.success() {
        return Err(AlignerError::Validation(format!(
            "minimap2 exited with status {status}"
        )));
    }
    if umi_len > 0 {
        eprintln!("5-Base UMI dedup (umi_len {umi_len}): removed {dups} duplicate pair(s).");
    }
    Ok(())
}

/// Build the two Bismark records for one 5-Base read pair, or `None` (not a proper
/// pair / a mate unmapped / chromosome-edge guard). Directional: the PE index is OT
/// (0) when R1 maps forward, OB (3) when R1 maps reverse — the only two orientations
/// a directional library produces. Pure (unit-testable), inverted polarity.
#[allow(clippy::too_many_arguments)]
fn five_base_emit_pe_record(
    rec1: &SamRecord,
    rec2: &SamRecord,
    identifier: &str,
    seq1_uc: &[u8],
    qual1: &[u8],
    seq2_uc: &[u8],
    qual2: &[u8],
    genome: &Genome,
    refid: &HashMap<String, usize>,
    phred64: bool,
    dovetail: bool,
    baseq: u8,
    counters: &mut Counters,
) -> Result<Option<(bismark_io::BismarkRecord, bismark_io::BismarkRecord)>> {
    // Require a proper pair with both mates mapped (5-Base v1: concordant pairs only).
    let proper = rec1.flag & 0x2 != 0 && rec1.flag & 0x4 == 0 && rec2.flag & 0x4 == 0;
    if !proper {
        counters.no_single_alignment_found += 1;
        return Ok(None);
    }
    counters.unique_best_alignment_count += 1;
    let index = if rec1.flag & 0x10 != 0 { 3 } else { 0 }; // R1 reverse → OB, else OT
    let best = BestAlignmentPaired {
        chromosome: rec1.rname.clone(),
        index,
        position_1: rec1.pos,
        position_2: rec2.pos,
        cigar_1: rec1.cigar.clone(),
        cigar_2: rec2.cigar.clone(),
        md_tag_1: rec1.md_tag.clone().unwrap_or_default(),
        md_tag_2: rec2.md_tag.clone().unwrap_or_default(),
        bowtie_sequence_1: rec1.seq.clone(),
        bowtie_sequence_2: rec2.seq.clone(),
        flag_1: rec1.flag,
        flag_2: rec2.flag,
        sum_of_alignment_scores: rec1.alignment_score.unwrap_or(0)
            + rec2.alignment_score.unwrap_or(0),
        sum_of_alignment_scores_second_best: None,
        mapq: rec1.mapq.min(rec2.mapq),
    };
    let ext = extract_corresponding_genomic_sequence_paired_end(&best, genome, counters)?;
    if ext.unmodified_genomic_sequence_1.len() != seq1_uc.len() + 2
        || ext.unmodified_genomic_sequence_2.len() != seq2_uc.len() + 2
    {
        counters.genomic_sequence_could_not_be_extracted_count += 1;
        return Ok(None);
    }
    let off = if phred64 { 64 } else { 33 };
    let call1 = mask_low_quality(seq1_uc, qual1, baseq, off);
    let call2 = mask_low_quality(seq2_uc, qual2, baseq, off);
    let mc1 = methylation_call(
        &call1,
        &ext.unmodified_genomic_sequence_1,
        ext.read_conversion_1,
        true,
        counters,
    );
    let mc2 = methylation_call(
        &call2,
        &ext.unmodified_genomic_sequence_2,
        ext.read_conversion_2,
        true,
        counters,
    );
    let (out1, out2) = paired_end_sam_output(
        identifier, seq1_uc, seq2_uc, qual1, qual2, &best, &ext, &mc1, &mc2, refid, phred64,
        dovetail,
    )?;
    Ok(Some((out1, out2)))
}

/// #787 deconvolution pass: read back the just-written 5-Base BAM, build the per-CpG
/// two-strand pileup against the genome, classify each cytosine (variant vs 5mC) and
/// write `<report_path>`. Reads are in SAM forward orientation; OT reads sequence the
/// `+` strand, OB reads the `-` strand (in forward orientation). At a genomic `C` (CpG)
/// the own strand is OT and the T-equivalent allele is `T`; at a genomic `G` the own
/// strand is OB and the T-equivalent allele is `A`.
fn run_five_base_deconvolution(genome: &Genome, bam_path: &Path, report_path: &Path) -> Result<()> {
    use crate::five_base_deconv::{DEFAULT_MIN_OPP_DEPTH, DEFAULT_VARIANT_OPP_FRAC, Deconvoluter};
    use noodles_sam::alignment::record::cigar::op::Kind;

    let mut reader = bismark_io::BamReader::from_path_without_sort_check(bam_path)
        .map_err(|e| AlignerError::Validation(format!("deconvolution: open BAM: {e}")))?;
    let mut dec = Deconvoluter::default();

    for rec in reader.records() {
        let rec = rec.map_err(|e| AlignerError::Validation(format!("deconvolution: {e}")))?;
        let inner = rec.inner();
        if u16::from(inner.flags()) & 0x4 != 0 {
            continue; // unmapped
        }
        let xr = bismark_io::tags::xr(inner.data())
            .map_err(|e| AlignerError::Validation(format!("deconvolution: XR: {e}")))?;
        let xg = bismark_io::tags::xg(inner.data())
            .map_err(|e| AlignerError::Validation(format!("deconvolution: XG: {e}")))?;
        let strand = bismark_io::BismarkStrand::from_xr_xg(xr, xg)
            .map_err(|e| AlignerError::Validation(format!("deconvolution: strand: {e}")))?;
        let (is_ot, is_ob) = (
            strand == bismark_io::BismarkStrand::OT,
            strand == bismark_io::BismarkStrand::OB,
        );
        if !is_ot && !is_ob {
            continue; // directional 5-Base only ever yields OT/OB
        }
        let Some(ref_id) = inner.reference_sequence_id() else {
            continue;
        };
        let Some(chrom) = genome.sq_order.get(ref_id) else {
            continue;
        };
        let Some(g) = genome.get(chrom) else { continue };
        let seq = inner.sequence().as_ref();
        let Some(start) = inner.alignment_start() else {
            continue;
        };
        let ref_start = usize::from(start) - 1; // 1-based POS → 0-based

        let (mut ri, mut rp) = (0usize, ref_start);
        for op in inner.cigar().as_ref().iter() {
            let len = op.len();
            match op.kind() {
                Kind::Match | Kind::SequenceMatch | Kind::SequenceMismatch => {
                    for _ in 0..len {
                        if let (Some(&base), Some(&gb)) = (seq.get(ri), g.get(rp)) {
                            // CpG `+`-strand cytosine: genomic C followed by G.
                            if gb == b'C' && g.get(rp + 1) == Some(&b'G') {
                                dec.observe(
                                    chrom,
                                    rp as u32,
                                    true,
                                    is_ot,
                                    base.eq_ignore_ascii_case(&b'T'),
                                );
                            } else if gb == b'G' && rp > 0 && g.get(rp - 1) == Some(&b'C') {
                                // CpG `-`-strand cytosine: genomic G preceded by C.
                                dec.observe(
                                    chrom,
                                    rp as u32,
                                    false,
                                    is_ob,
                                    base.eq_ignore_ascii_case(&b'A'),
                                );
                            }
                        }
                        ri += 1;
                        rp += 1;
                    }
                }
                Kind::Insertion | Kind::SoftClip => ri += len,
                Kind::Deletion | Kind::Skip => rp += len,
                _ => {}
            }
        }
    }

    let mut w = BufWriter::new(File::create(report_path)?);
    let s = dec.write_report(&mut w, DEFAULT_MIN_OPP_DEPTH, DEFAULT_VARIANT_OPP_FRAC)?;
    w.flush()?;
    eprintln!(
        "5-Base deconvolution: {} variant CpG(s), {} methylation site(s), {} undetermined \
         ({}/{} methylated calls kept). Report: {}",
        s.variants,
        s.methylation_sites,
        s.undetermined,
        s.methylated_calls,
        s.total_calls,
        report_path.display()
    );
    Ok(())
}

/// #787 DUPLEX-consensus pass: re-read the 5-Base BAM, group reads into duplex families
/// (same genomic span + canonical swap-collapsed UMI, one OT + one OB member), and
/// reconcile the 5mC->T signal per molecule into `<out>.5base_duplex.txt`. The UMI is
/// read from the `RX:Z:` tag the emit path wrote when `--five_base_umi_len > 0`; without
/// it, families key on span alone (a never-silent collision notice fires).
fn run_five_base_duplex(
    genome: &Genome,
    bam_path: &Path,
    report_path: &Path,
    umi_len: usize,
) -> Result<()> {
    use crate::five_base_duplex::{DuplexFamilies, DuplexKey, SiteObs, UmiSwap, canonical_umi};
    use noodles_sam::alignment::record::cigar::op::Kind;

    let mut reader = bismark_io::BamReader::from_path_without_sort_check(bam_path)
        .map_err(|e| AlignerError::Validation(format!("duplex: open BAM: {e}")))?;
    let mut fams = DuplexFamilies::default();
    let mut missing_umi = false;

    for rec in reader.records() {
        let rec = rec.map_err(|e| AlignerError::Validation(format!("duplex: {e}")))?;
        let inner = rec.inner();
        if u16::from(inner.flags()) & 0x4 != 0 {
            continue; // unmapped
        }
        let xr = bismark_io::tags::xr(inner.data())
            .map_err(|e| AlignerError::Validation(format!("duplex: XR: {e}")))?;
        let xg = bismark_io::tags::xg(inner.data())
            .map_err(|e| AlignerError::Validation(format!("duplex: XG: {e}")))?;
        let strand = bismark_io::BismarkStrand::from_xr_xg(xr, xg)
            .map_err(|e| AlignerError::Validation(format!("duplex: strand: {e}")))?;
        let is_ot = strand == bismark_io::BismarkStrand::OT;
        let is_ob = strand == bismark_io::BismarkStrand::OB;
        if !is_ot && !is_ob {
            continue; // directional 5-Base only ever yields OT/OB
        }
        let Some(ref_id) = inner.reference_sequence_id() else {
            continue;
        };
        let Some(chrom) = genome.sq_order.get(ref_id) else {
            continue;
        };
        let Some(g) = genome.get(chrom) else { continue };
        let seq = inner.sequence().as_ref();
        let Some(start) = inner.alignment_start() else {
            continue;
        };
        let ref_start = usize::from(start) - 1; // 1-based POS → 0-based

        // Canonical UMI from the RX tag (swap-collapsed so both members hash equal).
        let canon_umi: Vec<u8> = if umi_len > 0 {
            match bismark_io::tags::rx(inner.data()) {
                Ok(Some(raw)) => canonical_umi(raw, UmiSwap::RevComp),
                _ => {
                    missing_umi = true;
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        // One CIGAR walk: collect CpG-cytosine observations AND the reference end.
        let (mut ri, mut rp) = (0usize, ref_start);
        let mut obs: Vec<SiteObs> = Vec::new();
        for op in inner.cigar().as_ref().iter() {
            let len = op.len();
            match op.kind() {
                Kind::Match | Kind::SequenceMatch | Kind::SequenceMismatch => {
                    for _ in 0..len {
                        if let (Some(&base), Some(&gb)) = (seq.get(ri), g.get(rp)) {
                            if gb == b'C' && g.get(rp + 1) == Some(&b'G') {
                                obs.push(SiteObs {
                                    pos0: rp as u32,
                                    plus: true,
                                    t_equivalent: base.eq_ignore_ascii_case(&b'T'),
                                });
                            } else if gb == b'G' && rp > 0 && g.get(rp - 1) == Some(&b'C') {
                                obs.push(SiteObs {
                                    pos0: rp as u32,
                                    plus: false,
                                    t_equivalent: base.eq_ignore_ascii_case(&b'A'),
                                });
                            }
                        }
                        ri += 1;
                        rp += 1;
                    }
                }
                Kind::Insertion | Kind::SoftClip => ri += len,
                Kind::Deletion | Kind::Skip => rp += len,
                _ => {}
            }
        }
        let key = DuplexKey {
            chrom: chrom.clone(),
            start: ref_start as u32,
            end: rp as u32,
            canon_umi,
        };
        fams.add_read(key, is_ot, obs);
    }

    if umi_len == 0 || missing_umi {
        eprintln!(
            "Note: --five_base_duplex without per-read UMIs (use --five_base_umi_len) keys \
             families on genomic span alone; reads from DIFFERENT molecules sharing a span \
             may be merged into one family."
        );
    }

    let mut w = BufWriter::new(File::create(report_path)?);
    let s = w_duplex_report(&mut w, &fams)?;
    w.flush()?;
    eprintln!(
        "5-Base duplex: {} family(ies), {} duplex-paired, {} singleton(s); {} methylation \
         site(s), {} variant site(s), {} undetermined. Report: {}",
        s.total_families,
        s.duplex_paired,
        s.singletons,
        s.methylation_sites,
        s.variant_sites,
        s.undetermined_sites,
        report_path.display()
    );
    Ok(())
}

/// Thin wrapper so the duplex report uses the module's default thresholds.
fn w_duplex_report<W: std::io::Write>(
    w: &mut W,
    fams: &crate::five_base_duplex::DuplexFamilies,
) -> std::io::Result<crate::five_base_duplex::DuplexSummary> {
    use crate::five_base_deconv::DEFAULT_VARIANT_OPP_FRAC;
    use crate::five_base_duplex::DUPLEX_MIN_OPP_DEPTH;
    fams.write_report(w, DUPLEX_MIN_OPP_DEPTH, DEFAULT_VARIANT_OPP_FRAC)
}

/// #787 duplex CONSENSUS collapse: re-read the 5-Base BAM, group reads into duplex
/// families (same as the duplex pass), and emit ONE consensus read per paired family
/// into a separate `<out>.5base_consensus.bam`. The consensus uses the asymmetric 5mC>T
/// rule ([`crate::five_base_duplex::consensus_base`]): at a CpG the own strand carries
/// the call and the opposite strand is the variant check (a cytosine gone on both
/// strands is masked to `N`, so the methylation call becomes `.`); other positions
/// reconcile by agreement/quality. The consensus carries a standard single-strand
/// Bismark `XM`/`XR`/`XG`. Each member is mapped to reference coordinates by a CIGAR
/// walk, so soft-clips (e.g. an inline UMI prefix) and indels are handled; only singleton
/// (unpaired) families are skipped.
fn run_five_base_consensus(
    genome: &Genome,
    refid: &HashMap<String, usize>,
    bam_path: &Path,
    consensus_bam_path: &Path,
    header: &noodles_sam::Header,
    umi_len: usize,
) -> Result<()> {
    use crate::five_base_duplex::{DuplexKey, SiteKind, UmiSwap, canonical_umi, consensus_base};
    use noodles_sam::alignment::record::cigar::op::Kind;
    use std::collections::BTreeMap;

    /// One member's aligned read, as a reference-position → `(base, phred)` map built by
    /// walking the CIGAR. This handles soft-clips (e.g. an inline UMI prefix) and indels
    /// uniformly: clipped/inserted read bases are dropped, deleted reference positions are
    /// simply absent. `start`/`end` bound the aligned reference span.
    struct Member {
        start: u32,
        end: u32,
        covered: std::collections::HashMap<u32, (u8, u8)>,
        mapq: u8,
    }
    impl Member {
        fn at(&self, p: u32) -> Option<(u8, u8)> {
            self.covered.get(&p).copied()
        }
    }

    let mut reader = bismark_io::BamReader::from_path_without_sort_check(bam_path)
        .map_err(|e| AlignerError::Validation(format!("consensus: open BAM: {e}")))?;
    let mut fams: BTreeMap<DuplexKey, (Option<Member>, Option<Member>)> = BTreeMap::new();

    for rec in reader.records() {
        let rec = rec.map_err(|e| AlignerError::Validation(format!("consensus: {e}")))?;
        let inner = rec.inner();
        if u16::from(inner.flags()) & 0x4 != 0 {
            continue;
        }
        let xr = bismark_io::tags::xr(inner.data())
            .map_err(|e| AlignerError::Validation(format!("consensus: XR: {e}")))?;
        let xg = bismark_io::tags::xg(inner.data())
            .map_err(|e| AlignerError::Validation(format!("consensus: XG: {e}")))?;
        let strand = bismark_io::BismarkStrand::from_xr_xg(xr, xg)
            .map_err(|e| AlignerError::Validation(format!("consensus: strand: {e}")))?;
        let is_ot = strand == bismark_io::BismarkStrand::OT;
        let is_ob = strand == bismark_io::BismarkStrand::OB;
        if !is_ot && !is_ob {
            continue;
        }
        let Some(ref_id) = inner.reference_sequence_id() else {
            continue;
        };
        let Some(chrom) = genome.sq_order.get(ref_id) else {
            continue;
        };
        let Some(start) = inner.alignment_start() else {
            continue;
        };
        let ref_start = (usize::from(start) - 1) as u32;

        // Walk the CIGAR → per-reference-position (base, phred) map + the aligned span end.
        let seq = inner.sequence().as_ref();
        let quals = inner.quality_scores().as_ref();
        let mut covered: std::collections::HashMap<u32, (u8, u8)> =
            std::collections::HashMap::new();
        let (mut ri, mut rp) = (0usize, ref_start);
        for op in inner.cigar().as_ref().iter() {
            let len = op.len();
            match op.kind() {
                Kind::Match | Kind::SequenceMatch | Kind::SequenceMismatch => {
                    for _ in 0..len {
                        if let Some(&b) = seq.get(ri) {
                            let q = quals.get(ri).copied().unwrap_or(40);
                            covered.insert(rp, (b.to_ascii_uppercase(), q));
                        }
                        ri += 1;
                        rp += 1;
                    }
                }
                Kind::Insertion | Kind::SoftClip => ri += len,
                Kind::Deletion | Kind::Skip => rp += len as u32,
                Kind::HardClip | Kind::Pad => {}
            }
        }
        let end = rp;
        let canon_umi: Vec<u8> = if umi_len > 0 {
            match bismark_io::tags::rx(inner.data()) {
                Ok(Some(raw)) => canonical_umi(raw, UmiSwap::RevComp),
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        let member = Member {
            start: ref_start,
            end,
            covered,
            mapq: inner.mapping_quality().map(u8::from).unwrap_or(255),
        };
        let key = DuplexKey {
            chrom: chrom.clone(),
            start: ref_start,
            end,
            canon_umi,
        };
        let entry = fams.entry(key).or_insert((None, None));
        let slot = if is_ot { &mut entry.0 } else { &mut entry.1 };
        if slot.is_none() {
            *slot = Some(member); // first read on this strand represents the family
        }
    }

    let mut writer = bismark_io::BamWriter::from_path(consensus_bam_path, header.clone())
        .map_err(|e| AlignerError::Validation(format!("consensus: create BAM: {e}")))?;
    let (mut emitted, mut skipped) = (0u64, 0u64);

    for (key, (ot, ob)) in &fams {
        let (Some(ot), Some(ob)) = (ot, ob) else {
            continue; // singleton family: no duplex partner to reconcile
        };
        let Some(g) = genome.get(&key.chrom) else {
            skipped += 1;
            continue;
        };
        let start = ot.start.min(ob.start);
        let end = ot.end.max(ob.end);
        let mut cons_seq: Vec<u8> = Vec::with_capacity((end - start) as usize);
        let mut cons_qual: Vec<u8> = Vec::with_capacity((end - start) as usize);
        for p in start..end {
            let pu = p as usize;
            let kind = if g.get(pu) == Some(&b'C') && g.get(pu + 1) == Some(&b'G') {
                SiteKind::PlusCpG
            } else if g.get(pu) == Some(&b'G') && pu > 0 && g.get(pu - 1) == Some(&b'C') {
                SiteKind::MinusCpG
            } else {
                SiteKind::Other
            };
            let (b, q) = consensus_base(kind, ot.at(p), ob.at(p));
            cons_seq.push(b);
            cons_qual.push(q.saturating_add(33)); // phred → ASCII for the SAM QUAL field
        }

        // Synthesize a forward single-end SAM record for the consensus and run it through
        // the same inverted-call emit path as a normal 5-Base read.
        let umi_str = if key.canon_umi.is_empty() {
            "NA".to_string()
        } else {
            String::from_utf8_lossy(&key.canon_umi).into_owned()
        };
        let qname = format!("dpx:{}:{}-{}:{}", key.chrom, start, end, umi_str);
        let sam = SamRecord {
            qname: qname.clone(),
            flag: 0,
            rname: key.chrom.clone(),
            pos: start + 1,
            mapq: ot.mapq.min(ob.mapq),
            cigar: format!("{}M", cons_seq.len()),
            seq: String::from_utf8_lossy(&cons_seq).into_owned(),
            qual: String::from_utf8_lossy(&cons_qual).into_owned(),
            alignment_score: Some(0),
            second_best: None,
            md_tag: None,
            raw_line: String::new(),
        };
        let mut counters = Counters::default();
        if let Some(record) = five_base_emit_record(
            &sam,
            &qname,
            &cons_seq,
            &cons_qual,
            genome,
            refid,
            false, // consensus QUAL is phred33
            0,     // no base-quality masking on the consensus
            &mut counters,
        )? {
            write_record(&mut writer, &record)?;
            emitted += 1;
        } else {
            skipped += 1; // chromosome-edge guard, etc.
        }
    }

    writer
        .finish()
        .map_err(|e| AlignerError::Validation(format!("consensus: finalise BAM: {e}")))?;
    eprintln!(
        "5-Base duplex consensus: {emitted} consensus read(s) emitted, {skipped} family(ies) \
         skipped (chromosome-edge guard). BAM: {}",
        consensus_bam_path.display()
    );
    Ok(())
}

/// Open a (optionally gzipped) reader.
fn open_maybe_gz(path: &Path) -> Result<Box<dyn BufRead>> {
    let file = File::open(path)?;
    if path.to_string_lossy().ends_with(".gz") {
        Ok(Box::new(BufReader::new(MultiGzDecoder::new(file))))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

/// Read one 4-line FastQ record (owned id/seq/plus/qual lines incl. trailing `\n`),
/// or `None` at EOF / a truncated final record.
#[allow(clippy::type_complexity)]
fn read_fastq_record<R: BufRead>(
    reader: &mut R,
) -> Result<Option<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)>> {
    let (mut id, mut seq, mut plus, mut qual) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let n1 = reader.read_until(b'\n', &mut id)?;
    let n2 = reader.read_until(b'\n', &mut seq)?;
    let n3 = reader.read_until(b'\n', &mut plus)?;
    let n4 = reader.read_until(b'\n', &mut qual)?;
    if n1 == 0 || n2 == 0 || n3 == 0 || n4 == 0 {
        return Ok(None);
    }
    Ok(Some((id, seq, plus, qual)))
}

/// Strip a trailing `/1` or `/2` mate suffix from a read id (minimap2 reports the
/// bare name for both mates; the PE record name carries no mate suffix).
fn strip_mate_suffix(id: &str) -> String {
    id.strip_suffix("/1")
        .or_else(|| id.strip_suffix("/2"))
        .unwrap_or(id)
        .to_string()
}

/// A per-record aux writer (`--unmapped`/`--ambiguous`). The single-core path
/// writes gzip inline (`Gz`); a Phase-9b chunk worker writes **plain** (`Plain`)
/// to a temp file that the ordered merge re-emits through ONE `GzEncoder`
/// (`parallel::merge_aux_gz`) — a single-member gz stream raw-identical to
/// `--parallel 1` (PLAN §3.5). Both variants implement [`Write`] so the
/// per-read routing in `drive_merge`/`drive_merge_pe` is writer-agnostic.
// The `Gz` variant (a `GzEncoder` compression state) is larger than `Plain`, but an
// `AuxWriter` is held singly (one per sink, ≤4 per `PeSinks`), never in a hot
// collection, so the size difference is irrelevant — boxing would add pointless
// indirection.
#[allow(clippy::large_enum_variant)]
enum AuxWriter {
    /// Inline gzip (single-core / `--parallel 1`).
    Gz(GzEncoder<BufWriter<File>>),
    /// Plain bytes (a Phase-9b chunk worker; gz happens at the merge).
    Plain(BufWriter<File>),
}

impl Write for AuxWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            AuxWriter::Gz(w) => w.write(buf),
            AuxWriter::Plain(w) => w.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            AuxWriter::Gz(w) => w.flush(),
            AuxWriter::Plain(w) => w.flush(),
        }
    }
}

impl AuxWriter {
    /// Finalise: gz writes its trailer (`finish`); plain only flushes. Neither
    /// path flushes mid-stream — a `flush()` would force a deflate block boundary
    /// and break the merge's raw-byte identity (PLAN §3.5 invariant).
    fn finish(self) -> std::io::Result<()> {
        match self {
            AuxWriter::Gz(w) => {
                w.finish()?;
                Ok(())
            }
            AuxWriter::Plain(mut w) => w.flush(),
        }
    }
}

/// The per-read output sinks for one read file (or one Phase-9b chunk): the
/// Bismark BAM plus the optional `--ambig_bam` and the `--unmapped`/`--ambiguous`
/// aux writers (gzip for single-core, plain for a chunk worker — see [`AuxWriter`]).
struct Sinks {
    bam: BamWriter<BufWriter<File>>,
    ambig_bam: Option<BamWriter<BufWriter<File>>>,
    unmapped: Option<AuxWriter>,
    ambiguous: Option<AuxWriter>,
}

impl Sinks {
    /// Finalise every open sink (BGZF EOF markers + gzip trailers).
    fn finish(self) -> Result<()> {
        self.bam
            .finish()
            .map_err(|e| AlignerError::Validation(format!("failed to finalise BAM: {e}")))?;
        if let Some(ab) = self.ambig_bam {
            ab.finish().map_err(|e| {
                AlignerError::Validation(format!("failed to finalise ambig BAM: {e}"))
            })?;
        }
        if let Some(u) = self.unmapped {
            u.finish()?;
        }
        if let Some(a) = self.ambiguous {
            a.finish()?;
        }
        Ok(())
    }
}

/// Open the BAM and the optional `--ambig_bam` / `--unmapped` / `--ambiguous` sinks.
fn open_sinks(
    read_file: &str,
    config: &RunConfig,
    header: &noodles_sam::Header,
    bam_path: &Path,
) -> Result<Sinks> {
    let bam = BamWriter::from_path(bam_path, header.clone())
        .map_err(|e| AlignerError::Validation(format!("failed to open BAM {bam_path:?}: {e}")))?;

    let ambig_bam = if config.ambig_bam {
        let p = derive_output_path(
            read_file,
            config,
            &format!("_bismark_{}.ambig.bam", config.aligner.token()),
            ".ambig.bam",
        );
        eprintln!("Ambiguous BAM output: {}", p.display());
        Some(BamWriter::from_path(&p, header.clone()).map_err(|e| {
            AlignerError::Validation(format!("failed to open ambig BAM {p:?}: {e}"))
        })?)
    } else {
        None
    };

    let fasta = matches!(config.format, ReadFormat::FastA);
    let filename = basename(read_file);
    let prefix = config.output.prefix.as_deref();
    let base = config.output.basename.as_deref();
    let open_gz = |kind: AuxKind| -> Result<GzEncoder<BufWriter<File>>> {
        let name = aux_out::aux_filename(&filename, prefix, base, kind, fasta, None);
        let p = config.output.output_dir.join(name);
        Ok(GzEncoder::new(
            BufWriter::new(File::create(&p)?),
            Compression::default(),
        ))
    };
    let unmapped = if config.unmapped {
        Some(AuxWriter::Gz(open_gz(AuxKind::Unmapped)?))
    } else {
        None
    };
    let ambiguous = if config.ambiguous {
        Some(AuxWriter::Gz(open_gz(AuxKind::Ambiguous)?))
    } else {
        None
    };

    Ok(Sinks {
        bam,
        ambig_bam,
        unmapped,
        ambiguous,
    })
}

/// Read-file basename (the component after the last `/`).
fn basename(read_file: &str) -> String {
    Path::new(read_file)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| read_file.to_string())
}

/// Derive an output path: `(<prefix>.)<stripped-stem><default_suffix>` (or
/// `<basename><basename_suffix>` when `-B` is set), under `output_dir`. Mirrors
/// the report/BAM naming (Perl 1559–1638). NB: the `--unmapped`/`--ambiguous`
/// names use a DIFFERENT, un-stripped scheme — see `aux_out::aux_filename`.
fn derive_output_path(
    read_file: &str,
    config: &RunConfig,
    default_suffix: &str,
    basename_suffix: &str,
) -> PathBuf {
    let name = if let Some(b) = &config.output.basename {
        format!("{b}{basename_suffix}")
    } else {
        let stem = strip_fastq_suffix(&basename(read_file));
        let pre = if let Some(p) = &config.output.prefix {
            format!("{p}.{stem}")
        } else {
            stem
        };
        format!("{pre}{default_suffix}")
    };
    config.output.output_dir.join(name)
}

/// Strip the first matching FastQ suffix (Perl `s/(\.fastq\.gz|\.fq\.gz|\.fastq|\.fq)$//`).
fn strip_fastq_suffix(name: &str) -> String {
    for suf in [".fastq.gz", ".fq.gz", ".fastq", ".fq"] {
        if let Some(s) = name.strip_suffix(suf) {
            return s.to_string();
        }
    }
    name.to_string()
}

/// Re-read the original FastQ and run the merge per read, in lockstep with the
/// instances. For each `UniqueBest`, extract the genomic window, apply the
/// `len == read_len + 2` guard (Perl 3127), make the `XM` call, and write the
/// BAM record. `skip`/`upto` MUST match Phase 2's conversion so the driver and
/// the streams see the same reads.
#[allow(clippy::too_many_arguments)]
fn drive_merge<S: SamStream>(
    read_file: &Path,
    streams: &mut [S],
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    pbat: bool,
    sinks: &mut Sinks,
    counters: &mut Counters,
) -> Result<()> {
    let file = File::open(read_file)?;
    let mut reader: Box<dyn BufRead> = if read_file.to_string_lossy().ends_with(".gz") {
        Box::new(BufReader::new(MultiGzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };
    let directional = matches!(config.library, LibraryType::Directional);
    let fasta = matches!(config.format, ReadFormat::FastA);
    let (skip, upto, icpc) = (
        config.read_processing.skip,
        config.read_processing.upto,
        config.read_processing.icpc,
    );

    let (mut id, mut seq, mut plus, mut qual) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut count: u64 = 0;
    loop {
        id.clear();
        seq.clear();
        plus.clear();
        qual.clear();
        // FastQ = 4-line record; FastA = 2-line (`>id` / seq, no `+`/qual — Perl
        // process_single_end_fastA_…_methylation_call 2317).
        let n1 = reader.read_until(b'\n', &mut id)?;
        let n2 = reader.read_until(b'\n', &mut seq)?;
        if fasta {
            if n1 == 0 || n2 == 0 {
                break;
            }
        } else {
            let n3 = reader.read_until(b'\n', &mut plus)?;
            let n4 = reader.read_until(b'\n', &mut qual)?;
            if n1 == 0 || n2 == 0 || n3 == 0 || n4 == 0 {
                break;
            }
        }
        count += 1;
        if let Some(s) = skip
            && s > 0
            && count <= s
        {
            continue;
        }
        if let Some(u) = upto
            && u > 0
            && count > u
        {
            break;
        }
        counters.sequences_count += 1;
        // identifier = fix_id(chomp(header)) with the leading '@' (FastQ, Perl 2442)
        // or '>' (FastA, Perl 2330) stripped.
        let fixed = convert::fix_id(convert::chomp_newline(&id), icpc);
        let prefix: &[u8] = if fasta { b">" } else { b"@" };
        let id_bytes = fixed.strip_prefix(prefix).unwrap_or(&fixed);
        let identifier = String::from_utf8_lossy(id_bytes).into_owned();
        let seq_uc: Vec<u8> = convert::chomp_newline(&seq).to_ascii_uppercase();
        // FastA reads carry no quality → Phred 40 (`'I'`) × read length (Perl
        // check_results_single_end 2707–2709). FastQ uses the chomped quality line.
        let qual_bytes: Vec<u8> = if fasta {
            vec![b'I'; seq_uc.len()]
        } else {
            convert::chomp_newline(&qual).to_vec()
        };
        let sequence = String::from_utf8_lossy(&seq_uc).into_owned();

        let decision = check_results_single_end(
            &identifier,
            &sequence,
            streams,
            directional,
            config.score_min_intercept,
            config.score_min_slope,
            config.score_min_local,
            config.ambig_bam,
            counters,
        )?;

        // Route each read to its sink (Perl 2451–2465 + the per-outcome return
        // codes). Shared with the combined-index drive (`drive_merge_combined`).
        route_se_decision(
            decision,
            &identifier,
            &seq_uc,
            &qual_bytes,
            &seq,
            &plus,
            fasta,
            genome,
            refid,
            pbat,
            config,
            sinks,
            counters,
        )?;
    }
    Ok(())
}

/// Route one SE [`Decision`] to its sink (Perl 2451–2465 + the per-outcome return
/// codes). Extracted verbatim from [`drive_merge`]'s per-read body so the
/// combined-index drive ([`drive_merge_combined`]) reuses the **byte-frozen**
/// output arm (genomic extraction → `XM` call → BAM, and the
/// `--ambig_bam`/`--ambiguous`/`--unmapped` routing) unchanged. `seq`/`plus` are
/// the raw (un-chomped) FastQ/FastA line buffers; `seq_uc` is the chomped
/// upper-cased read; `qual_bytes` the chomped quality. (The faithful default
/// path's behavior is unchanged — this is a pure relocation, covered by the
/// existing end-to-end tests + the oxy gate.)
#[allow(clippy::too_many_arguments)]
fn route_se_decision(
    decision: Decision,
    identifier: &str,
    seq_uc: &[u8],
    qual_bytes: &[u8],
    seq: &[u8],
    plus: &[u8],
    fasta: bool,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    pbat: bool,
    config: &RunConfig,
    sinks: &mut Sinks,
    counters: &mut Counters,
) -> Result<()> {
    match decision {
        // Unique best → genomic-seq + XM call + BAM record (Phase 5).
        Decision::UniqueBest(best) => {
            let ext =
                extract_corresponding_genomic_sequence_single_end(&best, genome, pbat, counters)?;
            // Length guard (Perl 3127): the window must be read_len + 2; a
            // shorter one means a chromosome-edge guard fired → skip (not written).
            if ext.unmodified_genomic_sequence.len() != seq_uc.len() + 2 {
                eprintln!(
                    "Chromosomal sequence could not be extracted for\t{identifier}\t{}\t{}",
                    best.chromosome, best.position
                );
                counters.genomic_sequence_could_not_be_extracted_count += 1;
                return Ok(());
            }
            let methcall = methylation_call(
                seq_uc,
                &ext.unmodified_genomic_sequence,
                ext.read_conversion,
                false, // bisulfite polarity (frozen path)
                counters,
            );
            let record = single_end_sam_output(
                identifier,
                seq_uc,
                qual_bytes,
                &best,
                &ext,
                &methcall,
                refid,
                config.phred64,
            )?;
            write_record(&mut sinks.bam, &record)?;
        }
        // Ambiguous → the within-thread path's first alignment to --ambig_bam
        // (Perl 2976), then the FastQ aux with precedence --ambiguous else
        // --unmapped (Perl 2979–2987).
        Decision::Ambiguous { first_ambig } => {
            if let Some(ab) = sinks.ambig_bam.as_mut()
                && let Some(line) = first_ambig.as_deref()
            {
                write_raw_sam_line_to_bam(ab, line, refid)?;
            }
            let route = if sinks.ambiguous.is_some() {
                sinks.ambiguous.as_mut()
            } else {
                sinks.unmapped.as_mut()
            };
            if let Some(w) = route {
                let seq_orig = convert::chomp_newline(seq).to_vec();
                write_se_aux_record(w, fasta, identifier.as_bytes(), &seq_orig, plus, qual_bytes)?;
            }
        }
        // No alignment → --unmapped FastQ (Perl 2995–2999).
        Decision::NoAlignment => {
            if let Some(w) = sinks.unmapped.as_mut() {
                let seq_orig = convert::chomp_newline(seq).to_vec();
                write_se_aux_record(w, fasta, identifier.as_bytes(), &seq_orig, plus, qual_bytes)?;
            }
        }
        // Directional wrong-strand rejection: counted only, written nowhere (Perl 3116).
        Decision::Rejected => {}
    }
    Ok(())
}

// ===========================================================================
// `--combined_index` (v2) single-end directional path. Opt-in, never-silent,
// concordance-gated (NOT byte-identical). One both-strands Bowtie 2 pass over the
// combined CT+GA index → `combined::classify`/`select` → the shared
// `route_se_decision` (the byte-frozen output arm). PLAN 06072026 phase 2.
// ===========================================================================

/// The aligner options for the combined-index instance: the faithful options
/// plus `-k 2` (so the cross-sub-genome runner-up is visible to the classifier,
/// PLAN §3.4). The SINGLE source of truth, used both for the spawn and for the
/// report's "Bismark was run with…" line — so the report never under-reports what
/// Bowtie 2 was actually run with (code-review L1).
fn combined_aligner_options(config: &RunConfig) -> String {
    format!("{} -k 2", config.aligner_options)
}

/// The single-pass per-read combined selector — `combined::select` (directional,
/// C→T pass → OT/OB) or `combined::select_pbat` (pbat, G→A pass → CTOT/CTOB); both
/// single-stream, both routed `pbat=false` (the synthetic index 0/1 or 2/3 comes
/// from the classifier). Threaded into the shared `process_se_chunk_combined` /
/// `drive_merge_combined` so the (identical) gather loop isn't triplicated across
/// the directional + pbat paths (the dual-driver back-port trap).
type SelectFn =
    fn(&[crate::align::SamRecord], &str, f64, f64, bool, &mut Counters) -> Result<Decision>;

/// The single-pass per-PAIR combined selector — `combined::select_pe` (directional,
/// C→T pass → OT/OB) or `combined::select_pe_pbat` (pbat, G→A pass → CTOT/CTOB); both
/// single-stream over ONE both-strands PE pass. Threaded into the shared
/// `process_pe_chunk_combined` / `drive_merge_combined_pe` so the (identical) PE gather
/// loop isn't duplicated across the directional + pbat paths (the dual-driver back-port
/// trap). NB the non-directional selector (`select_pe_nondir`) takes TWO pair slices
/// (one per pass) and so is NOT a `SelectFnPe` — it has its own two-stream driver.
type SelectFnPe = fn(
    &[crate::align::SamPair],
    &str,
    &str,
    f64,
    f64,
    bool,
    &mut Counters,
) -> Result<DecisionPaired>;

/// SE combined-index pipeline (single-core directional). Mirrors [`run_se`] but
/// drives ONE both-strands instance over the combined index
/// ([`process_se_chunk_combined`]) and announces the experimental,
/// concordance-gated mode (never-silent) on STDERR + in the report.
fn run_se_combined(config: &RunConfig, reads: &[String]) -> Result<()> {
    let started = Instant::now();
    let opts = convert::ConvertOptions::from_config(config);
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    let directional = matches!(config.library, LibraryType::Directional);
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    let tok = config.aligner.token();
    // The actual options Bowtie 2 is run with (faithful + `-k 2`) — shown in the
    // report header so it does not under-report the reporting mode (L1).
    let combined_opts = combined_aligner_options(config);

    // Never-silent banner (STDERR). The resolve guard guarantees the index exists.
    if let Some(combined_basename) = &config.genome.combined_index_basename {
        eprintln!(
            ">>> Combined-index mode (EXPERIMENTAL, concordance-gated — NOT byte-identical to the \
             faithful per-strand path): one both-strands {} pass over {} (-k 2) <<<",
            config.aligner.name(),
            combined_basename.display()
        );
    }
    if config.ambig_bam {
        eprintln!(
            "Note: combined-index mode does not populate --ambig_bam records in this phase \
             (ambiguous reads are still written to --ambiguous/--unmapped if requested)."
        );
    }

    for read_file in reads {
        let bam_path =
            derive_output_path(read_file, config, &format!("_bismark_{tok}.bam"), ".bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_sinks(read_file, config, &header, &bam_path)?;

        let report_path = derive_output_path(
            read_file,
            config,
            &format!("_bismark_{tok}_SE_report.txt"),
            "_SE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_file,
                sequence_file2: None,
                genome_folder: &genome_folder,
                aligner_options: &combined_opts,
                aligner: config.aligner,
                library: config.library,
            },
        )?;
        // Never-silent: mark the report as combined-index mode.
        writeln!(
            report,
            "Combined-index mode (experimental, concordance-gated; NOT byte-identical to the \
             faithful per-strand path)"
        )?;

        let mut counters = Counters::default();
        let converted = process_se_chunk_combined(
            config,
            &genome,
            &refid,
            Path::new(read_file),
            &opts,
            combined::select, // directional: C→T pass → OT/OB
            &mut sinks,
            &mut counters,
        )?;

        report::print_final_analysis_report_single_end(&mut report, &counters, directional)?;
        // Combined-mode extra: the spurious-discard tally (PLAN §3.8).
        writeln!(
            report,
            "Spurious alignments discarded (combined-index; wrong sub-genome for the orientation):\t{}",
            counters.combined_spurious_count
        )?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;

        sinks.finish()?;

        for cr in &converted {
            let _ = std::fs::remove_file(&cr.path);
        }
        eprintln!("{}", counters_summary(read_file, &counters));
    }
    Ok(())
}

/// SE combined-index pipeline, **PBAT** (single-core). Mirrors [`run_se_combined`]
/// but the single both-strands pass is fed the **G→A-converted** reads
/// (`convert_se_files(Pbat)`) → CTOT/CTOB, selected by [`combined::select_pbat`].
/// The report carries the `--pbat` library line (`library = Pbat`) + the 4-strand
/// final analysis (`directional=false`, only CTOT/CTOB populated). PBAT-combined is
/// the G→A-pass half of the non-directional path (Phase 7 of PLAN 06072026).
fn run_se_combined_pbat(config: &RunConfig, reads: &[String]) -> Result<()> {
    let started = Instant::now();
    let opts = convert::ConvertOptions::from_config(config);
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    // PBAT → the report prints the 4-strand split (only CTOT/CTOB populated).
    let directional = false;
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    let tok = config.aligner.token();
    let combined_opts = combined_aligner_options(config);

    // Never-silent banner (STDERR).
    if let Some(combined_basename) = &config.genome.combined_index_basename {
        eprintln!(
            ">>> Combined-index mode, PBAT (EXPERIMENTAL, concordance-gated — NOT byte-identical \
             to the faithful 2-instance path): one both-strands {} pass over {} (-k 2) on \
             the G->A-converted reads → CTOT/CTOB <<<",
            config.aligner.name(),
            combined_basename.display()
        );
    }
    if config.ambig_bam {
        eprintln!(
            "Note: combined-index mode does not populate --ambig_bam records in this phase \
             (ambiguous reads are still written to --ambiguous/--unmapped if requested)."
        );
    }

    for read_file in reads {
        let bam_path =
            derive_output_path(read_file, config, &format!("_bismark_{tok}.bam"), ".bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_sinks(read_file, config, &header, &bam_path)?;

        let report_path = derive_output_path(
            read_file,
            config,
            &format!("_bismark_{tok}_SE_report.txt"),
            "_SE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_file,
                sequence_file2: None,
                genome_folder: &genome_folder,
                aligner_options: &combined_opts,
                aligner: config.aligner,
                library: config.library,
            },
        )?;
        // Never-silent: mark the report as combined-index PBAT mode.
        writeln!(
            report,
            "Combined-index mode, PBAT (experimental, concordance-gated; NOT byte-identical to the \
             faithful 2-instance per-strand path)"
        )?;

        let mut counters = Counters::default();
        let converted = process_se_chunk_combined(
            config,
            &genome,
            &refid,
            Path::new(read_file),
            &opts,
            combined::select_pbat, // pbat: G→A pass → CTOT/CTOB
            &mut sinks,
            &mut counters,
        )?;

        report::print_final_analysis_report_single_end(&mut report, &counters, directional)?;
        writeln!(
            report,
            "Spurious alignments discarded (combined-index; wrong sub-genome for the orientation):\t{}",
            counters.combined_spurious_count
        )?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;

        sinks.finish()?;

        for cr in &converted {
            let _ = std::fs::remove_file(&cr.path);
        }
        eprintln!("{}", counters_summary(read_file, &counters));
    }
    Ok(())
}

/// Convert reads (directional C→T, or pbat G→A — per `config.library` via
/// `convert_se_files`), spawn ONE both-strands Bowtie 2 instance over the combined
/// index with `-k 2`, and drive the single-pass classify→select→route per read
/// with `select_fn` (`combined::select` directional / `combined::select_pbat` pbat).
/// Returns the converted temp file(s) for cleanup.
#[allow(clippy::too_many_arguments)]
fn process_se_chunk_combined(
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    input: &Path,
    opts: &convert::ConvertOptions,
    select_fn: SelectFn,
    sinks: &mut Sinks,
    counters: &mut Counters,
) -> Result<Vec<convert::ConvertedReads>> {
    let bt2 = &config.detected_aligner.path;
    let read_file = input.to_string_lossy();
    // One converted temp file: directional C→T or pbat G→A (per config.library).
    let converted = convert_se_files(config, &read_file, opts)?;
    for cr in &converted {
        eprintln!(
            "Created {} converted version of {read_file} -> {} ({} sequences)",
            conv_label(&cr.name),
            cr.path.display(),
            cr.count
        );
    }
    let combined_basename = config
        .genome
        .combined_index_basename
        .as_ref()
        .ok_or_else(|| {
            AlignerError::Validation(
                "internal error: --combined_index reached alignment without a combined index"
                    .into(),
            )
        })?;
    // `-k 2` so the cross-sub-genome runner-up is visible to the classifier
    // (PLAN §3.4); `Orientation::Both` emits no `--norc`/`--nofw`. One instance.
    // Same option string the report header advertises (`combined_aligner_options`).
    let combined_opts = combined_aligner_options(config);
    let mut stream = AlignerStream::spawn(
        config.aligner,
        bt2,
        &combined_opts,
        Orientation::Both,
        combined_basename,
        &converted[0].path,
    )?;
    drive_merge_combined(
        input,
        &mut stream,
        config,
        genome,
        refid,
        select_fn,
        sinks,
        counters,
    )?;
    stream.finish()?;
    Ok(converted)
}

/// Re-read the original reads and, per read, gather the combined stream's `-k`
/// line group (consecutive same-QNAME lines — Bowtie 2 emits a read's k lines
/// contiguously, PLAN §3.4), run the provisional combined selection
/// ([`combined::select`]), and route the resulting [`Decision`] through the shared
/// [`route_se_decision`]. Generic over [`SamStream`] so it is unit-testable with a
/// canned stream. `select_fn` is the single-pass per-read selector — `combined::select`
/// (directional, C→T pass → OT/OB) or `combined::select_pbat` (pbat, G→A pass →
/// CTOT/CTOB); both single-stream, both route `pbat=false` (the synthetic index 0/1
/// or 2/3 comes straight from the classifier, NOT the faithful `+2` modifier).
/// Parametrizing the (identical) gather loop avoids triplicating it across the
/// directional + pbat single-stream paths (the dual-driver back-port trap).
#[allow(clippy::too_many_arguments)]
fn drive_merge_combined<S: SamStream>(
    read_file: &Path,
    stream: &mut S,
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    select_fn: SelectFn,
    sinks: &mut Sinks,
    counters: &mut Counters,
) -> Result<()> {
    let file = File::open(read_file)?;
    let mut reader: Box<dyn BufRead> = if read_file.to_string_lossy().ends_with(".gz") {
        Box::new(BufReader::new(MultiGzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };
    let fasta = matches!(config.format, ReadFormat::FastA);
    let (skip, upto, icpc) = (
        config.read_processing.skip,
        config.read_processing.upto,
        config.read_processing.icpc,
    );

    let (mut id, mut seq, mut plus, mut qual) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut count: u64 = 0;
    loop {
        id.clear();
        seq.clear();
        plus.clear();
        qual.clear();
        let n1 = reader.read_until(b'\n', &mut id)?;
        let n2 = reader.read_until(b'\n', &mut seq)?;
        if fasta {
            if n1 == 0 || n2 == 0 {
                break;
            }
        } else {
            let n3 = reader.read_until(b'\n', &mut plus)?;
            let n4 = reader.read_until(b'\n', &mut qual)?;
            if n1 == 0 || n2 == 0 || n3 == 0 || n4 == 0 {
                break;
            }
        }
        count += 1;
        if let Some(s) = skip
            && s > 0
            && count <= s
        {
            continue;
        }
        if let Some(u) = upto
            && u > 0
            && count > u
        {
            break;
        }
        counters.sequences_count += 1;
        let fixed = convert::fix_id(convert::chomp_newline(&id), icpc);
        let prefix: &[u8] = if fasta { b">" } else { b"@" };
        let id_bytes = fixed.strip_prefix(prefix).unwrap_or(&fixed);
        let identifier = String::from_utf8_lossy(id_bytes).into_owned();
        let seq_uc: Vec<u8> = convert::chomp_newline(&seq).to_ascii_uppercase();
        let qual_bytes: Vec<u8> = if fasta {
            vec![b'I'; seq_uc.len()]
        } else {
            convert::chomp_newline(&qual).to_vec()
        };
        let sequence = String::from_utf8_lossy(&seq_uc).into_owned();

        // Gather this read's `-k` alignment line group from the single combined
        // stream — all CONSECUTIVE same-QNAME lines (Bowtie 2 emits a read's k
        // alignments contiguously, PLAN §3.4). A miss is one FLAG-4 line (filtered
        // by `combined::select`). A read with no line — which Bowtie 2 never
        // produces — yields an empty group → NoAlignment, leaving the stream head
        // for the next read (no mis-assignment, no infinite loop).
        //
        // No desync sentinel is needed (cf. the faithful merge's flag-4-then-same-
        // id `die`, merge.rs ~200): that guard catches a SINGLE instance reporting a
        // read as both unmapped and mapped, whereas here we drain the WHOLE
        // contiguous same-QNAME run in one pass, so a read's lines cannot reappear
        // at the head after we move on. A stream-exhaustion assert at EOF is
        // deliberately NOT added either — it would false-positive under `--upto`
        // (the FastQ loop breaks early while the stream still holds later reads).
        // Output order/lockstep rests on the same Bowtie 2 `--reorder`-under-`-p`
        // invariant the faithful path relies on.
        let mut records: Vec<crate::align::SamRecord> = Vec::new();
        while stream.current().is_some_and(|r| r.qname == identifier) {
            records.push(stream.current().unwrap().clone());
            stream.advance()?;
        }

        let decision = select_fn(
            &records,
            &sequence,
            config.score_min_intercept,
            config.score_min_slope,
            config.score_min_local,
            counters,
        )?;
        route_se_decision(
            decision,
            &identifier,
            &seq_uc,
            &qual_bytes,
            &seq,
            &plus,
            fasta,
            genome,
            refid,
            false, // directional/pbat → pbat is always false (index 0/1 or 2/3 from classify)
            config,
            sinks,
            counters,
        )?;
    }
    Ok(())
}

// ===========================================================================
// `--combined_index` (v2) single-end NON-DIRECTIONAL path. Opt-in, never-silent,
// concordance-gated (NOT byte-identical). TWO both-strands Bowtie 2 passes over
// the combined CT+GA index — C→T-converted reads (→ OT/OB) + G→A-converted reads
// (→ CTOT/CTOB) — unioned per read → `combined::select_nondir` → the shared
// `route_se_decision`. Model (a) (two parallel passes + per-read streaming union)
// of PLAN 06072026 phase 5. (Model (b), the single conversion-tagged invocation,
// is the exec-model spike's alternative — not implemented here.)
// ===========================================================================

/// SE combined-index pipeline, **non-directional** (single-core). Mirrors
/// [`run_se_combined`] but drives TWO both-strands Bowtie 2 passes over the
/// combined index — one on the C→T-converted reads (→ OT/OB) and one on the
/// G→A-converted reads (→ CTOT/CTOB) — unioned per read
/// ([`process_se_chunk_combined_nondir`]). Announces the experimental,
/// concordance-gated non-directional mode (never-silent); the report prints the
/// 4-strand (OT/OB/CTOT/CTOB) split (`directional=false`).
fn run_se_combined_nondir(config: &RunConfig, reads: &[String]) -> Result<()> {
    let started = Instant::now();
    let opts = convert::ConvertOptions::from_config(config);
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    // Non-directional → the report prints the 4-strand (OT/OB/CTOT/CTOB) split.
    let directional = false;
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    let tok = config.aligner.token();
    // The actual options each Bowtie 2 pass is run with (faithful + `-k 2`).
    let combined_opts = combined_aligner_options(config);

    // Never-silent banner (STDERR). The resolve guard guarantees the index exists.
    if let Some(combined_basename) = &config.genome.combined_index_basename {
        eprintln!(
            ">>> Combined-index mode, NON-DIRECTIONAL (EXPERIMENTAL, concordance-gated — NOT \
             byte-identical to the faithful 4-instance path): two both-strands {} passes \
             (C->T + G->A reads) over {} (-k 2), unioned per read <<<",
            config.aligner.name(),
            combined_basename.display()
        );
    }
    if config.ambig_bam {
        eprintln!(
            "Note: combined-index mode does not populate --ambig_bam records in this phase \
             (ambiguous reads are still written to --ambiguous/--unmapped if requested)."
        );
    }

    for read_file in reads {
        let bam_path =
            derive_output_path(read_file, config, &format!("_bismark_{tok}.bam"), ".bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_sinks(read_file, config, &header, &bam_path)?;

        let report_path = derive_output_path(
            read_file,
            config,
            &format!("_bismark_{tok}_SE_report.txt"),
            "_SE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_file,
                sequence_file2: None,
                genome_folder: &genome_folder,
                aligner_options: &combined_opts,
                aligner: config.aligner,
                library: config.library,
            },
        )?;
        // Never-silent: mark the report as combined-index non-directional mode.
        writeln!(
            report,
            "Combined-index mode, non-directional (experimental, concordance-gated; NOT \
             byte-identical to the faithful 4-instance per-strand path)"
        )?;

        let mut counters = Counters::default();
        let converted = process_se_chunk_combined_nondir(
            config,
            &genome,
            &refid,
            Path::new(read_file),
            &opts,
            &mut sinks,
            &mut counters,
        )?;

        report::print_final_analysis_report_single_end(&mut report, &counters, directional)?;
        // Combined-mode extra: the spurious-discard tally (PLAN §3.8).
        writeln!(
            report,
            "Spurious alignments discarded (combined-index; wrong sub-genome for the orientation):\t{}",
            counters.combined_spurious_count
        )?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;

        sinks.finish()?;

        for cr in &converted {
            let _ = std::fs::remove_file(&cr.path);
        }
        eprintln!("{}", counters_summary(read_file, &counters));
    }
    Ok(())
}

/// Convert reads to BOTH the C→T and G→A temp files (non-directional —
/// `convert_se_files` returns `[C→T (idx 0), G→A (idx 1)]`), spawn TWO both-strands
/// Bowtie 2 instances over the combined index with `-k 2` (one per converted file),
/// and drive the per-read UNION classify→select→route. Returns the converted temp
/// files for cleanup. The two passes run concurrently (two subprocesses, ~2× the
/// combined index resident — model (a)'s known memory cost vs the single-invocation
/// model (b)).
fn process_se_chunk_combined_nondir(
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    input: &Path,
    opts: &convert::ConvertOptions,
    sinks: &mut Sinks,
    counters: &mut Counters,
) -> Result<Vec<convert::ConvertedReads>> {
    let bt2 = &config.detected_aligner.path;
    let read_file = input.to_string_lossy();
    // Non-directional conversion → [C→T (idx 0), G→A (idx 1)] (convert_se_files).
    let converted = convert_se_files(config, &read_file, opts)?;
    for cr in &converted {
        eprintln!(
            "Created {} converted version of {read_file} -> {} ({} sequences)",
            conv_label(&cr.name),
            cr.path.display(),
            cr.count
        );
    }
    let combined_basename = config
        .genome
        .combined_index_basename
        .as_ref()
        .ok_or_else(|| {
            AlignerError::Validation(
                "internal error: --combined_index reached alignment without a combined index"
                    .into(),
            )
        })?;
    // `-k 2` per pass (so each pass's cross-sub-genome runner-up is visible);
    // `Orientation::Both` emits no `--norc`/`--nofw`. Two passes over the SAME
    // combined index, differing only by the converted-read input file:
    // converted[0] = C→T (→ OT/OB), converted[1] = G→A (→ CTOT/CTOB).
    let combined_opts = combined_aligner_options(config);
    let mut ct_stream = AlignerStream::spawn(
        config.aligner,
        bt2,
        &combined_opts,
        Orientation::Both,
        combined_basename,
        &converted[0].path,
    )?;
    let mut ga_stream = AlignerStream::spawn(
        config.aligner,
        bt2,
        &combined_opts,
        Orientation::Both,
        combined_basename,
        &converted[1].path,
    )?;
    drive_merge_combined_nondir(
        input,
        &mut ct_stream,
        &mut ga_stream,
        config,
        genome,
        refid,
        sinks,
        counters,
    )?;
    ct_stream.finish()?;
    ga_stream.finish()?;
    Ok(converted)
}

/// Re-read the original reads and, per read, gather the C→T pass's `-k` line group
/// AND the G→A pass's `-k` line group (each a contiguous same-QNAME run — Bowtie 2
/// emits a read's k lines contiguously), union-select ([`combined::select_nondir`]),
/// and route the resulting [`Decision`] through the shared [`route_se_decision`]
/// (`pbat=false` — the non-dir indices 2/3 come from `classify`, not the pbat +2
/// modifier). Generic over [`SamStream`] so it is unit-testable with canned streams.
// Two streams (one extra vs the directional `drive_merge_combined`) push this one
// arg over clippy's threshold; the args are all distinct read-only context.
// Two stream type params (NOT one `<S>`): the sequential exec model (v2 phase 9)
// passes `ct_stream = FileSamStream` (pass 1 replayed from disk) + `ga_stream =
// AlignerStream` (live pass 2) — DIFFERENT concrete types. Parallel model (a) infers
// `C = G = AlignerStream`. The body is unchanged from the single-`<S>` version.
#[allow(clippy::too_many_arguments)]
fn drive_merge_combined_nondir<C: SamStream, G: SamStream>(
    read_file: &Path,
    ct_stream: &mut C,
    ga_stream: &mut G,
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    sinks: &mut Sinks,
    counters: &mut Counters,
) -> Result<()> {
    let file = File::open(read_file)?;
    let mut reader: Box<dyn BufRead> = if read_file.to_string_lossy().ends_with(".gz") {
        Box::new(BufReader::new(MultiGzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };
    let fasta = matches!(config.format, ReadFormat::FastA);
    let (skip, upto, icpc) = (
        config.read_processing.skip,
        config.read_processing.upto,
        config.read_processing.icpc,
    );

    let (mut id, mut seq, mut plus, mut qual) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut count: u64 = 0;
    loop {
        id.clear();
        seq.clear();
        plus.clear();
        qual.clear();
        let n1 = reader.read_until(b'\n', &mut id)?;
        let n2 = reader.read_until(b'\n', &mut seq)?;
        if fasta {
            if n1 == 0 || n2 == 0 {
                break;
            }
        } else {
            let n3 = reader.read_until(b'\n', &mut plus)?;
            let n4 = reader.read_until(b'\n', &mut qual)?;
            if n1 == 0 || n2 == 0 || n3 == 0 || n4 == 0 {
                break;
            }
        }
        count += 1;
        if let Some(s) = skip
            && s > 0
            && count <= s
        {
            continue;
        }
        if let Some(u) = upto
            && u > 0
            && count > u
        {
            break;
        }
        counters.sequences_count += 1;
        let fixed = convert::fix_id(convert::chomp_newline(&id), icpc);
        let prefix: &[u8] = if fasta { b">" } else { b"@" };
        let id_bytes = fixed.strip_prefix(prefix).unwrap_or(&fixed);
        let identifier = String::from_utf8_lossy(id_bytes).into_owned();
        let seq_uc: Vec<u8> = convert::chomp_newline(&seq).to_ascii_uppercase();
        let qual_bytes: Vec<u8> = if fasta {
            vec![b'I'; seq_uc.len()]
        } else {
            convert::chomp_newline(&qual).to_vec()
        };
        let sequence = String::from_utf8_lossy(&seq_uc).into_owned();

        // Never-silent desync guard (PLAN §3.9): each pass emits exactly one line
        // per input read (mapped or a single FLAG-4 miss), and both converted files
        // preserve the original read order + the same skip/upto (applied at
        // conversion), so each stream's head must be THIS read. A mismatch means the
        // two streams desynced from the re-read input — die loudly rather than
        // silently mis-pair every downstream read (the two-stream gather has a larger
        // blast radius than the directional single stream).
        if let Some(r) = ct_stream.current()
            && r.qname != identifier
        {
            return Err(AlignerError::Validation(format!(
                "Combined non-directional desync: C->T pass stream head is '{}' but expected '{identifier}'",
                r.qname
            )));
        }
        if let Some(r) = ga_stream.current()
            && r.qname != identifier
        {
            return Err(AlignerError::Validation(format!(
                "Combined non-directional desync: G->A pass stream head is '{}' but expected '{identifier}'",
                r.qname
            )));
        }

        // Drain BOTH passes' contiguous same-QNAME runs for this read (incl. a lone
        // FLAG-4 miss — the common ~50%-per-pass case, filtered by `select_nondir`).
        // Draining both before advancing keeps the streams in lockstep with the
        // re-read input.
        let mut ct_records: Vec<crate::align::SamRecord> = Vec::new();
        while ct_stream.current().is_some_and(|r| r.qname == identifier) {
            ct_records.push(ct_stream.current().unwrap().clone());
            ct_stream.advance()?;
        }
        let mut ga_records: Vec<crate::align::SamRecord> = Vec::new();
        while ga_stream.current().is_some_and(|r| r.qname == identifier) {
            ga_records.push(ga_stream.current().unwrap().clone());
            ga_stream.advance()?;
        }

        select_and_route_se_nondir(
            &ct_records,
            &ga_records,
            &identifier,
            &sequence,
            &seq_uc,
            &qual_bytes,
            &seq,
            &plus,
            fasta,
            genome,
            refid,
            config,
            sinks,
            counters,
        )?;
    }
    Ok(())
}

/// Shared per-read TAIL of BOTH non-directional combined drivers — model (a)'s
/// two-stream [`drive_merge_combined_nondir`] AND model (b)'s one-stream tagged
/// [`drive_merge_combined_nondir_tagged`]. Union-selects the C→T-pass + G→A-pass
/// record groups via [`combined::select_nondir`] and routes the [`Decision`]
/// (`pbat=false` — the non-dir indices 2/3 come from `classify`, not the pbat `+2`
/// modifier). Extracted so a future fix to the select/route contract touches BOTH
/// exec models, not one (the dual-driver back-port trap — phase-8 plan-review
/// A-I5/B-I6). ONLY the GATHER of `ct_records`/`ga_records` legitimately differs
/// between the two drivers (two streams vs one tagged stream split by suffix); from
/// this point on they are identical.
#[allow(clippy::too_many_arguments)]
fn select_and_route_se_nondir(
    ct_records: &[crate::align::SamRecord],
    ga_records: &[crate::align::SamRecord],
    identifier: &str,
    sequence: &str,
    seq_uc: &[u8],
    qual_bytes: &[u8],
    seq: &[u8],
    plus: &[u8],
    fasta: bool,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    config: &RunConfig,
    sinks: &mut Sinks,
    counters: &mut Counters,
) -> Result<()> {
    let decision = combined::select_nondir(
        ct_records,
        ga_records,
        sequence,
        config.score_min_intercept,
        config.score_min_slope,
        config.score_min_local,
        counters,
    )?;
    route_se_decision(
        decision, identifier, seq_uc, qual_bytes, seq, plus, fasta, genome, refid,
        false, // non-directional → pbat is false (indices 2/3 come from classify)
        config, sinks, counters,
    )
}

// ===========================================================================
// Combined-index SEQUENTIAL non-directional driver (model (a) RSS variant)
// (PLAN 06072026 phase 9). Model (a)'s two both-strands passes run ONE AT A TIME:
// pass 1 (C->T) spills its records to a temp file and its Bowtie 2 EXITS (freeing
// the combined index) before pass 2 (G->A) spawns → one index resident at a time,
// ~half the peak RSS. BYTE-IDENTICAL to parallel model (a) (each pass sees the same
// untagged converted file + index regardless of when it runs — exec-model spike C2);
// reuses `drive_merge_combined_nondir` (signature-widened only) + the shared
// `select_and_route_se_nondir` tail UNCHANGED. The trade is wall time (no overlap).
// ===========================================================================

/// Drain a [`SamStream`] to `path`, writing each record's verbatim `raw_line` + `\n`
/// (bounded memory — one record held at a time). Returns the number of records
/// written. Records are written in stream order (Bowtie 2's output order = the
/// converted-input order), so a [`FileSamStream`] replays them in the same order a
/// live stream would, preserving the per-read lockstep `drive_merge_combined_nondir`
/// relies on. Generic over [`SamStream`] so it is unit-testable with a canned stream.
fn spill_stream_to_file<S: SamStream>(stream: &mut S, path: &Path) -> Result<u64> {
    let mut w = BufWriter::new(File::create(path)?);
    let mut n: u64 = 0;
    while let Some(r) = stream.current() {
        writeln!(w, "{}", r.raw_line)?;
        n += 1;
        stream.advance()?;
    }
    w.flush()?;
    Ok(n)
}

/// Drain a [`PairedSamStream`] to `path`, writing each pair's two verbatim `raw_line`s
/// (read1 then read2) one `\n`-terminated line each — the PE analog of
/// [`spill_stream_to_file`] (Phase 6, the sequential low-RSS PE variant). Returns the
/// number of PAIRS written. Bounded memory: one pair held at a time. The spill order is
/// always read1-then-read2 (the canonicalised order, NOT Bowtie 2's leftmost-first
/// emission order), but [`PairedFileSamStream`] replays via [`SamPair::from_lines`],
/// which re-canonicalises — so the replay yields identical pairs regardless of the
/// on-disk line order, preserving the per-pair lockstep `drive_merge_combined_pe_nondir`
/// relies on. Generic over [`PairedSamStream`] so it is unit-testable with a canned stream.
fn spill_pe_stream_to_file<S: PairedSamStream>(stream: &mut S, path: &Path) -> Result<u64> {
    let mut w = BufWriter::new(File::create(path)?);
    let mut n: u64 = 0;
    while let Some(p) = stream.current_pair() {
        writeln!(w, "{}", p.read1.raw_line)?;
        writeln!(w, "{}", p.read2.raw_line)?;
        n += 1;
        stream.advance_pair()?;
    }
    w.flush()?;
    Ok(n)
}

/// SEQUENTIAL non-directional combined driver (model (a) low-RSS variant): runs the
/// two both-strands passes one at a time (pass 1 exits before pass 2 spawns), unioned
/// per read via the shared `select_and_route_se_nondir`. Mirrors
/// [`run_se_combined_nondir`] (parallel model (a)) but with the sequential
/// banner/marker + [`process_se_chunk_combined_nondir_sequential`]. BYTE-IDENTICAL to
/// model (a) (see the section header).
fn run_se_combined_nondir_sequential(config: &RunConfig, reads: &[String]) -> Result<()> {
    let started = Instant::now();
    let opts = convert::ConvertOptions::from_config(config);
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    // Non-directional → the report prints the 4-strand (OT/OB/CTOT/CTOB) split.
    let directional = false;
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    let tok = config.aligner.token();
    let combined_opts = combined_aligner_options(config);

    // Never-silent banner (STDERR). The resolve guard guarantees the index exists.
    if let Some(combined_basename) = &config.genome.combined_index_basename {
        eprintln!(
            ">>> Combined-index mode, NON-DIRECTIONAL SEQUENTIAL (EXPERIMENTAL, concordance-gated — \
             byte-identical to the default PARALLEL combined non-dir path, NOT to the faithful \
             4-instance path): two both-strands {} passes (C->T then G->A) over {} (-k 2) run \
             ONE AT A TIME — pass 1 exits before pass 2 starts, so one combined index is resident at \
             a time (~half the peak RSS, ~2x the wall), unioned per read <<<",
            config.aligner.name(),
            combined_basename.display()
        );
    }
    if config.ambig_bam {
        eprintln!(
            "Note: combined-index mode does not populate --ambig_bam records in this phase \
             (ambiguous reads are still written to --ambiguous/--unmapped if requested)."
        );
    }

    for read_file in reads {
        let bam_path =
            derive_output_path(read_file, config, &format!("_bismark_{tok}.bam"), ".bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_sinks(read_file, config, &header, &bam_path)?;

        let report_path = derive_output_path(
            read_file,
            config,
            &format!("_bismark_{tok}_SE_report.txt"),
            "_SE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_file,
                sequence_file2: None,
                genome_folder: &genome_folder,
                aligner_options: &combined_opts,
                aligner: config.aligner,
                library: config.library,
            },
        )?;
        // Never-silent: mark the report as combined-index non-directional SEQUENTIAL.
        writeln!(
            report,
            "Combined-index mode, non-directional SEQUENTIAL (experimental, concordance-gated; \
             byte-identical to the default parallel combined non-dir path, one index resident at a \
             time; NOT byte-identical to the faithful 4-instance per-strand path)"
        )?;

        let mut counters = Counters::default();
        let converted = process_se_chunk_combined_nondir_sequential(
            config,
            &genome,
            &refid,
            Path::new(read_file),
            &opts,
            &mut sinks,
            &mut counters,
        )?;

        report::print_final_analysis_report_single_end(&mut report, &counters, directional)?;
        writeln!(
            report,
            "Spurious alignments discarded (combined-index; wrong sub-genome for the orientation):\t{}",
            counters.combined_spurious_count
        )?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;

        sinks.finish()?;

        for cr in &converted {
            let _ = std::fs::remove_file(&cr.path);
        }
        eprintln!("{}", counters_summary(read_file, &counters));
    }
    Ok(())
}

/// Convert reads to BOTH the C→T and G→A temp files (as parallel model (a)), then run
/// the two both-strands Bowtie 2 passes SEQUENTIALLY: spawn pass 1 (C→T), **spill its
/// records to a temp file and `finish()` it** (its Bowtie 2 exits → the combined index
/// is freed), THEN spawn pass 2 (G→A) and drive the per-read union with pass 1
/// replayed from disk (`FileSamStream`) against the live pass-2 stream. ONE combined
/// index resident at a time (the ~−50% RSS win vs model (a)'s two co-resident loads).
/// Returns the converted temp files for cleanup (the pass-1 spill is deleted here).
fn process_se_chunk_combined_nondir_sequential(
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    input: &Path,
    opts: &convert::ConvertOptions,
    sinks: &mut Sinks,
    counters: &mut Counters,
) -> Result<Vec<convert::ConvertedReads>> {
    let bt2 = &config.detected_aligner.path;
    let read_file = input.to_string_lossy();
    // Non-directional conversion → [C→T (idx 0), G→A (idx 1)] (convert_se_files) —
    // the SAME untagged files parallel model (a) produces.
    let converted = convert_se_files(config, &read_file, opts)?;
    for cr in &converted {
        eprintln!(
            "Created {} converted version of {read_file} -> {} ({} sequences)",
            conv_label(&cr.name),
            cr.path.display(),
            cr.count
        );
    }
    let combined_basename = config
        .genome
        .combined_index_basename
        .as_ref()
        .ok_or_else(|| {
            AlignerError::Validation(
                "internal error: --combined_index_sequential reached alignment without a combined index"
                    .into(),
            )
        })?;
    let combined_opts = combined_aligner_options(config);

    // Spill path: a sibling of the C→T converted file (already in the resolved
    // temp_dir — reusing its path sidesteps the empty default `temp_dir`).
    let mut spill_path = converted[0].path.clone();
    let mut spill_name = spill_path.file_name().unwrap_or_default().to_os_string();
    spill_name.push(".ct_pass.sam");
    spill_path.set_file_name(spill_name);

    // ---- PASS 1 (C→T → OT/OB) ------------------------------------------------
    // Spawn ONE both-strands pass over the C→T file, drain it to the spill file,
    // then FINISH it — `finish()` -> `child.wait()` blocks until pass-1 Bowtie 2
    // EXITS, freeing the combined index. THE RSS INVARIANT: pass 2 must NOT spawn
    // before this returns (the `?` also aborts here on a non-zero pass-1 exit,
    // before any pass-2 process exists). The gate's RSS ceiling is the primary
    // guard; the co-residency sampler corroborates.
    let mut ct_stream = AlignerStream::spawn(
        config.aligner,
        bt2,
        &combined_opts,
        Orientation::Both,
        combined_basename,
        &converted[0].path,
    )?;
    let spilled = spill_stream_to_file(&mut ct_stream, &spill_path)?;
    ct_stream.finish()?;
    eprintln!(
        "Sequential combined-index: spilled {spilled} pass-1 (C->T) alignment lines to {} and freed \
         the combined index before the G->A pass",
        spill_path.display()
    );

    // ---- PASS 2 (G→A → CTOT/CTOB) -------------------------------------------
    // Now (and only now) spawn the second pass; replay pass 1 from disk against it.
    let mut ga_stream = AlignerStream::spawn(
        config.aligner,
        bt2,
        &combined_opts,
        Orientation::Both,
        combined_basename,
        &converted[1].path,
    )?;
    let mut ct_file_stream = FileSamStream::open(&spill_path)?;
    // The byte-frozen (body-unchanged, signature-widened) model-(a) driver, with the
    // C→T stream sourced from disk and the G→A stream live.
    drive_merge_combined_nondir(
        input,
        &mut ct_file_stream,
        &mut ga_stream,
        config,
        genome,
        refid,
        sinks,
        counters,
    )?;
    ga_stream.finish()?;

    // The spill is internal scratch (not a `ConvertedReads`) — clean it up here;
    // `converted` is returned for the caller's cleanup loop.
    let _ = std::fs::remove_file(&spill_path);
    Ok(converted)
}

// ===========================================================================
// Combined-index model (b) — single-pass tagged non-directional driver
// (PLAN 06072026 phase 8). One Bowtie 2 pass over conversion-tagged interleaved
// reads (one index load) instead of model (a)'s two parallel passes (two loads).
// NON-FAITHFUL / NOT decision-equivalent (the qname tag perturbs Bowtie 2's
// read-name RNG); ground-truth-validated (SPIKE_modelb_accuracy.md). Reuses the
// shared `select_and_route_se_nondir` tail unchanged — only the GATHER differs.
// ===========================================================================

/// Which read-conversion pass a tagged Bowtie 2 output line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConvTag {
    Ct,
    Ga,
}

/// Strip the model-(b) conversion tag (`__CT`/`__GA`) that
/// `convert_se_tagged_interleaved` appended to a tagged read's qname, returning the
/// base id + which pass produced the line. **Fails loud** on a qname lacking the
/// tag — never a silent mis-partition (phase-8 plan-review A-Crit1/B-C2).
fn strip_conv_tag(qname: &str) -> Result<(&str, ConvTag)> {
    if let Some(base) = qname.strip_suffix("__CT") {
        Ok((base, ConvTag::Ct))
    } else if let Some(base) = qname.strip_suffix("__GA") {
        Ok((base, ConvTag::Ga))
    } else {
        Err(AlignerError::Validation(format!(
            "Combined-index tagged mode: Bowtie 2 output qname '{qname}' lacks the expected \
             __CT/__GA conversion tag — cannot assign it to a read-conversion pass."
        )))
    }
}

/// Model-(b) non-directional combined driver: ONE Bowtie 2 pass over the
/// conversion-tagged interleaved reads, split by tag → the shared
/// `select_and_route_se_nondir`. Mirrors [`run_se_combined_nondir`] (model (a))
/// but with the single-pass banner/marker + [`process_se_chunk_combined_nondir_tagged`].
fn run_se_combined_nondir_tagged(config: &RunConfig, reads: &[String]) -> Result<()> {
    let started = Instant::now();
    let opts = convert::ConvertOptions::from_config(config);
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    // Non-directional → the report prints the 4-strand (OT/OB/CTOT/CTOB) split.
    let directional = false;
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    let tok = config.aligner.token();
    let combined_opts = combined_aligner_options(config);

    // Never-silent banner (STDERR). The resolve guard guarantees the index exists.
    if let Some(combined_basename) = &config.genome.combined_index_basename {
        eprintln!(
            ">>> Combined-index mode, NON-DIRECTIONAL SINGLE-PASS (model b; EXPERIMENTAL — NOT \
             byte-identical AND NOT decision-equivalent to the model-(a) two-pass path: the \
             conversion tag perturbs Bowtie 2's read-name RNG; ground-truth-validated, never the \
             default): ONE both-strands Bowtie 2 pass over {} (-k 2) of conversion-tagged \
             interleaved reads (one index load instead of two) <<<",
            combined_basename.display()
        );
    }
    if config.ambig_bam {
        eprintln!(
            "Note: combined-index mode does not populate --ambig_bam records in this phase \
             (ambiguous reads are still written to --ambiguous/--unmapped if requested)."
        );
    }

    for read_file in reads {
        let bam_path =
            derive_output_path(read_file, config, &format!("_bismark_{tok}.bam"), ".bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_sinks(read_file, config, &header, &bam_path)?;

        let report_path = derive_output_path(
            read_file,
            config,
            &format!("_bismark_{tok}_SE_report.txt"),
            "_SE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_file,
                sequence_file2: None,
                genome_folder: &genome_folder,
                aligner_options: &combined_opts,
                aligner: config.aligner,
                library: config.library,
            },
        )?;
        // Never-silent: mark the report as combined-index model-(b) single-pass.
        writeln!(
            report,
            "Combined-index mode, non-directional SINGLE-PASS (model b; experimental, \
             concordance-gated; NOT byte-identical AND NOT decision-equivalent to the \
             model-(a) two-pass path)"
        )?;

        let mut counters = Counters::default();
        let converted = process_se_chunk_combined_nondir_tagged(
            config,
            &genome,
            &refid,
            Path::new(read_file),
            &opts,
            &mut sinks,
            &mut counters,
        )?;

        report::print_final_analysis_report_single_end(&mut report, &counters, directional)?;
        writeln!(
            report,
            "Spurious alignments discarded (combined-index; wrong sub-genome for the orientation):\t{}",
            counters.combined_spurious_count
        )?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;

        sinks.finish()?;

        for cr in &converted {
            let _ = std::fs::remove_file(&cr.path);
        }
        eprintln!("{}", counters_summary(read_file, &counters));
    }
    Ok(())
}

/// Convert reads to ONE conversion-tagged interleaved temp file
/// (`convert_se_tagged_interleaved`), spawn ONE both-strands Bowtie 2 instance over
/// the combined index with `-k 2`, and drive the per-read split→union
/// classify→select→route. Returns the converted temp file for cleanup. ONE
/// subprocess → ONE combined index resident (model (b)'s −47% RSS vs model (a)'s
/// two co-resident loads).
fn process_se_chunk_combined_nondir_tagged(
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    input: &Path,
    opts: &convert::ConvertOptions,
    sinks: &mut Sinks,
    counters: &mut Counters,
) -> Result<Vec<convert::ConvertedReads>> {
    let bt2 = &config.detected_aligner.path;
    let read_file = input.to_string_lossy();
    let fasta = matches!(config.format, ReadFormat::FastA);
    // ONE interleaved tagged converted file: per read, `<id>__CT` (C→T) then
    // `<id>__GA` (G→A). count = N base reads (2N emitted records).
    let converted =
        convert::convert_se_tagged_interleaved(input, &config.output.temp_dir, opts, fasta)?;
    eprintln!(
        "Created conversion-tagged interleaved version of {read_file} -> {} ({} base reads, 2x tagged records)",
        converted.path.display(),
        converted.count
    );
    let combined_basename = config
        .genome
        .combined_index_basename
        .as_ref()
        .ok_or_else(|| {
            AlignerError::Validation(
                "internal error: --combined_index_single_pass reached alignment without a combined index"
                    .into(),
            )
        })?;
    // `-k 2`; `Orientation::Both` emits no `--norc`/`--nofw`. ONE pass over the
    // combined index fed the tagged interleaved reads.
    let combined_opts = combined_aligner_options(config);
    let mut stream = AlignerStream::spawn(
        config.aligner,
        bt2,
        &combined_opts,
        Orientation::Both,
        combined_basename,
        &converted.path,
    )?;
    drive_merge_combined_nondir_tagged(input, &mut stream, config, genome, refid, sinks, counters)?;
    stream.finish()?;
    Ok(vec![converted])
}

/// Re-read the original reads and, per read, drain the SINGLE tagged stream's
/// contiguous same-base-id run (each base-id emits its `__CT` lines then its
/// `__GA` lines under the `--reorder`-under-`-p` invariant), partition by the
/// tag into `ct_records` / `ga_records` (tag stripped), then hand to the shared
/// [`select_and_route_se_nondir`]. Generic over [`SamStream`] for unit testing.
///
/// Never-silent contract (phase-8 plan-review A-Crit1/B-C2 — the single-stream
/// guard is weaker than model (a)'s two independent per-stream checks):
/// (i) the stream head's TAG-STRIPPED qname must == `identifier` (a raw compare
/// would always fire — the head carries a tag); (ii) after draining, BOTH
/// `ct_records` AND `ga_records` must be non-empty — every base-id is emitted
/// twice, so each tag MUST contribute ≥1 line (incl. a lone FLAG-4 miss); a
/// missing half → die loud; (iii) `strip_conv_tag` dies on an untagged record.
fn drive_merge_combined_nondir_tagged<S: SamStream>(
    read_file: &Path,
    stream: &mut S,
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    sinks: &mut Sinks,
    counters: &mut Counters,
) -> Result<()> {
    let file = File::open(read_file)?;
    let mut reader: Box<dyn BufRead> = if read_file.to_string_lossy().ends_with(".gz") {
        Box::new(BufReader::new(MultiGzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };
    let fasta = matches!(config.format, ReadFormat::FastA);
    let (skip, upto, icpc) = (
        config.read_processing.skip,
        config.read_processing.upto,
        config.read_processing.icpc,
    );

    let (mut id, mut seq, mut plus, mut qual) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut count: u64 = 0;
    loop {
        id.clear();
        seq.clear();
        plus.clear();
        qual.clear();
        let n1 = reader.read_until(b'\n', &mut id)?;
        let n2 = reader.read_until(b'\n', &mut seq)?;
        if fasta {
            if n1 == 0 || n2 == 0 {
                break;
            }
        } else {
            let n3 = reader.read_until(b'\n', &mut plus)?;
            let n4 = reader.read_until(b'\n', &mut qual)?;
            if n1 == 0 || n2 == 0 || n3 == 0 || n4 == 0 {
                break;
            }
        }
        count += 1;
        if let Some(s) = skip
            && s > 0
            && count <= s
        {
            continue;
        }
        if let Some(u) = upto
            && u > 0
            && count > u
        {
            break;
        }
        counters.sequences_count += 1;
        let fixed = convert::fix_id(convert::chomp_newline(&id), icpc);
        let prefix: &[u8] = if fasta { b">" } else { b"@" };
        let id_bytes = fixed.strip_prefix(prefix).unwrap_or(&fixed);
        let identifier = String::from_utf8_lossy(id_bytes).into_owned();
        let seq_uc: Vec<u8> = convert::chomp_newline(&seq).to_ascii_uppercase();
        let qual_bytes: Vec<u8> = if fasta {
            vec![b'I'; seq_uc.len()]
        } else {
            convert::chomp_newline(&qual).to_vec()
        };
        let sequence = String::from_utf8_lossy(&seq_uc).into_owned();

        // (i) desync guard on the TAG-STRIPPED head — every read emits __CT + __GA,
        // so the head's base id MUST be this read (cf. model (a)'s per-stream guard).
        if let Some(r) = stream.current() {
            let (base, _) = strip_conv_tag(&r.qname)?;
            if base != identifier {
                return Err(AlignerError::Validation(format!(
                    "Combined non-directional tagged desync: stream head base id is '{base}' (from \
                     qname '{}') but expected '{identifier}'",
                    r.qname
                )));
            }
        }

        // Drain the contiguous same-base-id run, partitioning by the tag. The tag
        // is stripped from each record's qname (downstream keys on `identifier`,
        // but keep the records clean). Borrow ends before each `advance`.
        let mut ct_records: Vec<crate::align::SamRecord> = Vec::new();
        let mut ga_records: Vec<crate::align::SamRecord> = Vec::new();
        loop {
            let parsed = match stream.current() {
                Some(r) => {
                    let (base, tag) = strip_conv_tag(&r.qname)?;
                    if base != identifier {
                        None
                    } else {
                        Some((base.to_string(), tag))
                    }
                }
                None => None,
            };
            let (base, tag) = match parsed {
                Some(p) => p,
                None => break,
            };
            let mut rec = stream.current().unwrap().clone();
            rec.qname = base;
            match tag {
                ConvTag::Ct => ct_records.push(rec),
                ConvTag::Ga => ga_records.push(rec),
            }
            stream.advance()?;
        }

        // (ii) both halves must be present — a missing __CT or __GA half means the
        // tagged stream desynced from the re-read input (never-silent).
        if ct_records.is_empty() || ga_records.is_empty() {
            return Err(AlignerError::Validation(format!(
                "Combined non-directional tagged desync: read '{identifier}' is missing its __CT \
                 or __GA half (ct lines={}, ga lines={}); the tagged stream did not emit both \
                 conversion records for this read",
                ct_records.len(),
                ga_records.len()
            )));
        }

        select_and_route_se_nondir(
            &ct_records,
            &ga_records,
            &identifier,
            &sequence,
            &seq_uc,
            &qual_bytes,
            &seq,
            &plus,
            fasta,
            genome,
            refid,
            config,
            sinks,
            counters,
        )?;
    }
    Ok(())
}

/// Write one SE `--unmapped`/`--ambiguous` record in the input format: FastA
/// 2-line `>id\nseq` (Perl 2454–2466) or FastQ 4-line. `seq` is the chomped,
/// **non-uppercased** original read.
fn write_se_aux_record<W: Write>(
    w: &mut W,
    fasta: bool,
    id: &[u8],
    seq: &[u8],
    plus: &[u8],
    qual: &[u8],
) -> Result<()> {
    if fasta {
        aux_out::write_fasta_record(w, id, seq)
    } else {
        aux_out::write_fastq_record(w, id, seq, plus, qual)
    }
}

// ===========================================================================
// Paired-end directional driver (Phase 7).
// ===========================================================================

/// The per-mode PE instance plan (Perl PE `@fhs` names 295–298, input assignment
/// 394–451, name rule 6466–6471). Each tuple is `(Bismark slot, orientation,
/// index, mate-1 conv kind, mate-2 conv kind)`; the stream is placed at `slot` in
/// the length-4 `Vec<Option<_>>`. Per-slot index is CT,GA,CT,GA and orientation
/// `--norc` for slots 0/1, `--nofw` for 2/3. PE extraction keys on the raw slot
/// index (NO `+2` modifier — that is SE-pbat-only). The directional reject (index
/// 1/2) is inert for non-dir/pbat (`directional=false`).
fn pe_instance_plan(
    library: LibraryType,
) -> Vec<(
    usize,
    Orientation,
    IndexChoice,
    convert::ConvKind,
    convert::ConvKind,
)> {
    use IndexChoice::{Ct as ICt, Ga as IGa};
    use Orientation::{Nofw, Norc};
    use convert::ConvKind::{Ct, Ga};
    match library {
        // directional: s0 OT (CT idx, --norc), s3 OB (GA idx, --nofw); both
        // read `-1 C→T_R1 -2 G→A_R2`.
        LibraryType::Directional => vec![(0, Norc, ICt, Ct, Ga), (3, Nofw, IGa, Ct, Ga)],
        // pbat: s1 CTOB (GA idx, --norc), s2 CTOT (CT idx, --nofw); both read
        // `-1 G→A_R1 -2 C→T_R2`. (Slots 0/3 unpopulated.)
        LibraryType::Pbat => vec![(1, Norc, IGa, Ga, Ct), (2, Nofw, ICt, Ga, Ct)],
        // non-dir: all 4 slots — s0,s3 read C→T_R1/G→A_R2; s1,s2 read G→A_R1/C→T_R2.
        LibraryType::NonDirectional => vec![
            (0, Norc, ICt, Ct, Ga),
            (1, Norc, IGa, Ga, Ct),
            (2, Nofw, ICt, Ga, Ct),
            (3, Nofw, IGa, Ct, Ga),
        ],
    }
}

/// Look up the converted temp file for a planned `(mate, kind)` (every planned
/// pair is converted exactly once into `converted`).
fn pe_lookup(
    converted: &[((u8, convert::ConvKind), convert::ConvertedReads)],
    mate: u8,
    kind: convert::ConvKind,
) -> &Path {
    &converted
        .iter()
        .find(|((m, k), _)| *m == mate && *k == kind)
        .expect("a converted file exists for every planned (mate, kind)")
        .1
        .path
}

/// Process one PE input — a whole mate-pair (single-core / `--parallel 1`) or one
/// contiguous chunk subset pair (`--parallel N`, Phase 9b): convert each distinct
/// `(mate, kind)` exactly once (2 files for directional/pbat, 4 for non-dir), spawn
/// the 2/4 paired Bowtie 2 instances per the [`pe_instance_plan`], and drive the PE
/// lockstep merge into the (already-open) `sinks`, accumulating `counters`. Returns
/// the converted temp files for the caller to clean up. The report is **not** written
/// here — the caller owns it (`run_pe` for N==1, [`parallel`] for N>1).
#[allow(clippy::too_many_arguments)]
fn process_pe_chunk(
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    read_1: &Path,
    read_2: &Path,
    opts: &convert::ConvertOptions,
    sinks: &mut PeSinks,
    counters: &mut Counters,
) -> Result<Vec<((u8, convert::ConvKind), convert::ConvertedReads)>> {
    let bt2 = &config.detected_aligner.path;
    let fasta = matches!(config.format, ReadFormat::FastA);
    let plan = pe_instance_plan(config.library);

    // Convert each distinct (mate, kind) the plan needs EXACTLY ONCE — Perl makes 2
    // files for directional/pbat (shared by both instances) and 4 for non-dir (each
    // pair shared by two slots). Preserve first-seen order.
    let mut needed: Vec<(u8, convert::ConvKind)> = Vec::new();
    for &(_slot, _orient, _idx, k1, k2) in &plan {
        for mk in [(1u8, k1), (2u8, k2)] {
            if !needed.contains(&mk) {
                needed.push(mk);
            }
        }
    }
    let mut converted: Vec<((u8, convert::ConvKind), convert::ConvertedReads)> = Vec::new();
    for &(mate, kind) in &needed {
        let input = if mate == 1 { read_1 } else { read_2 };
        let cr = convert_pe_kind(fasta, input, &config.output.temp_dir, opts, mate, kind)?;
        eprintln!(
            "Created {} converted version of {} -> {} ({} sequences)",
            conv_label(&cr.name),
            input.display(),
            cr.path.display(),
            cr.count
        );
        converted.push(((mate, kind), cr));
    }

    // Slot-indexed (0..4): populate the per-mode slots, leaving the rest `None`
    // (the merge scans 0,3,1,2). Each instance reads its `-1`/`-2` converted files.
    let mut streams: Vec<Option<PairedAlignerStream>> = vec![None, None, None, None];
    for (slot, orientation, index_choice, k1, k2) in plan {
        let index_basename = match index_choice {
            IndexChoice::Ct => &config.genome.ct_index_basename,
            IndexChoice::Ga => &config.genome.ga_index_basename,
        };
        let m1 = pe_lookup(&converted, 1, k1);
        let m2 = pe_lookup(&converted, 2, k2);
        streams[slot] = Some(PairedAlignerStream::spawn(
            config.aligner,
            bt2,
            &config.aligner_options,
            orientation,
            index_basename,
            m1,
            m2,
        )?);
    }

    // Perl's `$dovetail` (8047): `!--no_dovetail`, set for EVERY aligner — the
    // `if($bowtie2)` at 8051 only gates pushing `--dovetail` to the aligner
    // options, NOT this variable. HISAT2 suppresses the flag from `aligner_options`
    // (2a) but still uses `$dovetail=1` for the PE TLEN sign (Perl 8898/8946), so
    // this MUST come from `config.dovetail`, not a scan of `aligner_options`
    // (which would wrongly yield `false` for HISAT2 → flipped TLEN on same-POS
    // fully-overlapping pairs). For Bowtie 2 the two are equal, so this is a no-op.
    let dovetail = config.dovetail;
    drive_merge_pe(
        read_1,
        read_2,
        &mut streams,
        config,
        genome,
        refid,
        dovetail,
        sinks,
        counters,
    )?;
    for s in streams.into_iter().flatten() {
        s.finish()?;
    }
    Ok(converted)
}

/// PE pipeline (single-core / `--parallel 1`, all library types) (Perl
/// `start_methylation_call_procedure_paired_ends`, 1746–1962): load the genome once,
/// then per mate-pair open the sinks + report header, run [`process_pe_chunk`] against
/// the whole pair, write the final analysis + wall-clock line, finalise the sinks, and
/// clean up the converted temp files. (The `--parallel N` path lives in
/// [`parallel::run_pe_multicore`].)
fn run_pe(config: &RunConfig, mates1: &[String], mates2: &[String]) -> Result<()> {
    let started = Instant::now();
    let opts = convert::ConvertOptions::from_config(config);
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    let directional = matches!(config.library, LibraryType::Directional);
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    let tok = config.aligner.token();

    for (read_1, read_2) in mates1.iter().zip(mates2) {
        let bam_path =
            derive_output_path(read_1, config, &format!("_bismark_{tok}_pe.bam"), "_pe.bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_pe_sinks(read_1, read_2, config, &header, &bam_path)?;

        let report_path = derive_output_path(
            read_1,
            config,
            &format!("_bismark_{tok}_PE_report.txt"),
            "_PE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_1,
                sequence_file2: Some(read_2),
                genome_folder: &genome_folder,
                aligner_options: &config.aligner_options,
                aligner: config.aligner,
                library: config.library,
            },
        )?;

        let mut counters = Counters::default();
        let converted = process_pe_chunk(
            config,
            &genome,
            &refid,
            Path::new(read_1),
            Path::new(read_2),
            &opts,
            &mut sinks,
            &mut counters,
        )?;

        report::print_final_analysis_report_paired_ends(&mut report, &counters, directional)?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;
        sinks.finish()?;

        // Per-mode temp cleanup (rev1 A; Perl 2155): delete EVERY converted temp
        // file — 2 for directional/pbat, 4 for non-directional. Best-effort.
        for ((_mate, _kind), cr) in &converted {
            let _ = std::fs::remove_file(&cr.path);
        }

        eprintln!("{}", counters_summary_pe(read_1, read_2, &counters));
    }
    Ok(())
}

/// PE output sinks: one BAM (both mates), one optional `--ambig_bam`, and the
/// `_1`/`_2` gzipped `--unmapped`/`--ambiguous` FastQ writers.
struct PeSinks {
    bam: BamWriter<BufWriter<File>>,
    ambig_bam: Option<BamWriter<BufWriter<File>>>,
    unmapped_1: Option<AuxWriter>,
    unmapped_2: Option<AuxWriter>,
    ambiguous_1: Option<AuxWriter>,
    ambiguous_2: Option<AuxWriter>,
}

impl PeSinks {
    fn finish(self) -> Result<()> {
        self.bam
            .finish()
            .map_err(|e| AlignerError::Validation(format!("failed to finalise BAM: {e}")))?;
        if let Some(ab) = self.ambig_bam {
            ab.finish().map_err(|e| {
                AlignerError::Validation(format!("failed to finalise ambig BAM: {e}"))
            })?;
        }
        for g in [
            self.unmapped_1,
            self.unmapped_2,
            self.ambiguous_1,
            self.ambiguous_2,
        ]
        .into_iter()
        .flatten()
        {
            g.finish()?;
        }
        Ok(())
    }
}

/// Open the PE BAM + the optional `--ambig_bam` (`_pe.ambig.bam`) and the `_1`/`_2`
/// gzipped `--unmapped`/`--ambiguous` files (named off each mate's un-stripped basename).
fn open_pe_sinks(
    read_1: &str,
    read_2: &str,
    config: &RunConfig,
    header: &noodles_sam::Header,
    bam_path: &Path,
) -> Result<PeSinks> {
    let bam = BamWriter::from_path(bam_path, header.clone())
        .map_err(|e| AlignerError::Validation(format!("failed to open BAM {bam_path:?}: {e}")))?;

    let ambig_bam = if config.ambig_bam {
        let p = derive_output_path(
            read_1,
            config,
            &format!("_bismark_{}_pe.ambig.bam", config.aligner.token()),
            "_pe.ambig.bam",
        );
        eprintln!("Ambiguous BAM output: {}", p.display());
        Some(BamWriter::from_path(&p, header.clone()).map_err(|e| {
            AlignerError::Validation(format!("failed to open ambig BAM {p:?}: {e}"))
        })?)
    } else {
        None
    };

    let fasta = matches!(config.format, ReadFormat::FastA);
    let prefix = config.output.prefix.as_deref();
    let base = config.output.basename.as_deref();
    let (b1, b2) = (basename(read_1), basename(read_2));
    let open_gz = |name: String| -> Result<GzEncoder<BufWriter<File>>> {
        let p = config.output.output_dir.join(name);
        Ok(GzEncoder::new(
            BufWriter::new(File::create(&p)?),
            Compression::default(),
        ))
    };
    let (unmapped_1, unmapped_2) = if config.unmapped {
        (
            Some(AuxWriter::Gz(open_gz(aux_out::aux_filename(
                &b1,
                prefix,
                base,
                AuxKind::Unmapped,
                fasta,
                Some(1),
            ))?)),
            Some(AuxWriter::Gz(open_gz(aux_out::aux_filename(
                &b2,
                prefix,
                base,
                AuxKind::Unmapped,
                fasta,
                Some(2),
            ))?)),
        )
    } else {
        (None, None)
    };
    let (ambiguous_1, ambiguous_2) = if config.ambiguous {
        (
            Some(AuxWriter::Gz(open_gz(aux_out::aux_filename(
                &b1,
                prefix,
                base,
                AuxKind::Ambiguous,
                fasta,
                Some(1),
            ))?)),
            Some(AuxWriter::Gz(open_gz(aux_out::aux_filename(
                &b2,
                prefix,
                base,
                AuxKind::Ambiguous,
                fasta,
                Some(2),
            ))?)),
        )
    } else {
        (None, None)
    };

    Ok(PeSinks {
        bam,
        ambig_bam,
        unmapped_1,
        unmapped_2,
        ambiguous_1,
        ambiguous_2,
    })
}

/// Open a FastQ reader (gz or plain) for the PE lockstep.
fn open_reader(path: &Path) -> Result<Box<dyn BufRead>> {
    let file = File::open(path)?;
    Ok(if path.to_string_lossy().ends_with(".gz") {
        Box::new(BufReader::new(MultiGzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    })
}

/// Re-read BOTH original FastQ files in lockstep (Perl 2600–2674) and run the PE
/// merge per pair, routing each `DecisionPaired` to its sink. The two genomic-seq
/// length guards run in order (R1 short-circuits before R2 — Perl 3864→3867).
#[allow(clippy::too_many_arguments)]
fn drive_merge_pe(
    read_1: &Path,
    read_2: &Path,
    streams: &mut [Option<PairedAlignerStream>],
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    dovetail: bool,
    sinks: &mut PeSinks,
    counters: &mut Counters,
) -> Result<()> {
    let mut r1 = open_reader(read_1)?;
    let mut r2 = open_reader(read_2)?;
    let directional = matches!(config.library, LibraryType::Directional);
    let fasta = matches!(config.format, ReadFormat::FastA);
    let (skip, upto, icpc) = (
        config.read_processing.skip,
        config.read_processing.upto,
        config.read_processing.icpc,
    );

    let (mut id1, mut seq1, mut plus1, mut qual1) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut id2, mut seq2, mut plus2, mut qual2) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut count: u64 = 0;
    loop {
        for v in [
            &mut id1, &mut seq1, &mut plus1, &mut qual1, &mut id2, &mut seq2, &mut plus2,
            &mut qual2,
        ] {
            v.clear();
        }
        // FastQ = 4 lines/mate; FastA = 2 lines/mate (no `+`/qual — Perl
        // process_fastA_files_for_paired_end_methylation_calls 2484). Read r1 fully
        // then r2 fully (the per-mate-file order).
        let n_id1 = r1.read_until(b'\n', &mut id1)?;
        let n_seq1 = r1.read_until(b'\n', &mut seq1)?;
        if !fasta {
            let _ = r1.read_until(b'\n', &mut plus1)?;
            let _ = r1.read_until(b'\n', &mut qual1)?;
        }
        let n_id2 = r2.read_until(b'\n', &mut id2)?;
        let n_seq2 = r2.read_until(b'\n', &mut seq2)?;
        if !fasta {
            let _ = r2.read_until(b'\n', &mut plus2)?;
            let _ = r2.read_until(b'\n', &mut qual2)?;
        }
        // Break on a missing required line. FastQ guards the 6 data lines (the two
        // `+` lines are NOT guarded — Perl 2611); FastA guards id+seq per mate
        // (Perl 2484 `last unless ($id1 and $seq1 and $id2 and $seq2)`). For FastQ,
        // `qual{1,2}.is_empty()` ≡ the original `n_qual == 0` (buffers were cleared).
        let incomplete = if fasta {
            n_id1 == 0 || n_seq1 == 0 || n_id2 == 0 || n_seq2 == 0
        } else {
            n_id1 == 0
                || n_seq1 == 0
                || qual1.is_empty()
                || n_id2 == 0
                || n_seq2 == 0
                || qual2.is_empty()
        };
        if incomplete {
            break;
        }
        count += 1;
        if let Some(s) = skip
            && s > 0
            && count <= s
        {
            continue;
        }
        if let Some(u) = upto
            && u > 0
            && count > u
        {
            break;
        }
        counters.sequences_count += 1;

        // R1 id: fix_id + @-strip → the merge identifier (Perl 2640). R2 id: fix_id
        // + @-strip → the aux R2 id (R2 is never the merge key; Perl never strips R2's
        // @, but write_fastq_record re-adds the @, so we pass the @-stripped form).
        let id1_fixed = convert::fix_id(convert::chomp_newline(&id1), icpc);
        let id2_fixed = convert::fix_id(convert::chomp_newline(&id2), icpc);
        let id_prefix: &[u8] = if fasta { b">" } else { b"@" };
        let identifier =
            String::from_utf8_lossy(id1_fixed.strip_prefix(id_prefix).unwrap_or(&id1_fixed))
                .into_owned();
        let id2_stripped =
            String::from_utf8_lossy(id2_fixed.strip_prefix(id_prefix).unwrap_or(&id2_fixed))
                .into_owned();
        let seq1_uc: Vec<u8> = convert::chomp_newline(&seq1).to_ascii_uppercase();
        let seq2_uc: Vec<u8> = convert::chomp_newline(&seq2).to_ascii_uppercase();
        // FastA: per-mate Phred 40 (`'I'`) × that mate's read length (Perl
        // check_results_paired_end 3271–3280). FastQ: the chomped quality lines.
        let qual1_bytes: Vec<u8> = if fasta {
            vec![b'I'; seq1_uc.len()]
        } else {
            convert::chomp_newline(&qual1).to_vec()
        };
        let qual2_bytes: Vec<u8> = if fasta {
            vec![b'I'; seq2_uc.len()]
        } else {
            convert::chomp_newline(&qual2).to_vec()
        };
        let s1 = String::from_utf8_lossy(&seq1_uc).into_owned();
        let s2 = String::from_utf8_lossy(&seq2_uc).into_owned();

        let decision = check_results_paired_end(
            &identifier,
            &s1,
            &s2,
            streams,
            directional,
            config.score_min_intercept,
            config.score_min_slope,
            config.score_min_local,
            config.ambig_bam,
            config.aligner,
            counters,
        )?;

        // Route each pair to its sink (Perl 2649–2674 + the per-outcome return
        // codes). Shared with the combined-index PE drive (`drive_merge_combined_pe`).
        route_pe_decision(
            decision,
            &identifier,
            &id2_stripped,
            &seq1_uc,
            &seq2_uc,
            &qual1_bytes,
            &qual2_bytes,
            &seq1,
            &plus1,
            &seq2,
            &plus2,
            fasta,
            genome,
            refid,
            dovetail,
            config,
            sinks,
            counters,
        )?;
    }
    Ok(())
}

/// Route one PE [`DecisionPaired`] to its sink (Perl 2649–2674 + the per-outcome
/// return codes). Extracted verbatim from [`drive_merge_pe`]'s per-pair body so the
/// combined-index PE drive ([`drive_merge_combined_pe`]) reuses the **byte-frozen**
/// output arm (PE genomic extraction → two `XM` calls → two BAM records, and the
/// `--ambig_bam`/`--ambiguous`/`--unmapped` routing) unchanged. `seq1`/`plus1`/`seq2`/
/// `plus2` are the raw (un-chomped) FastQ/FastA line buffers; `seq{1,2}_uc` the chomped
/// upper-cased reads; `qual{1,2}_bytes` the chomped qualities; `id2_stripped` the
/// `@`/`>`-stripped read-2 id for the aux files. (The faithful default path is
/// unchanged — a pure relocation, covered by the existing PE end-to-end tests + the
/// oxy gate, exactly as `route_se_decision` was extracted.)
#[allow(clippy::too_many_arguments)]
fn route_pe_decision(
    decision: DecisionPaired,
    identifier: &str,
    id2_stripped: &str,
    seq1_uc: &[u8],
    seq2_uc: &[u8],
    qual1_bytes: &[u8],
    qual2_bytes: &[u8],
    seq1: &[u8],
    plus1: &[u8],
    seq2: &[u8],
    plus2: &[u8],
    fasta: bool,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    dovetail: bool,
    config: &RunConfig,
    sinks: &mut PeSinks,
    counters: &mut Counters,
) -> Result<()> {
    match decision {
        DecisionPaired::UniqueBest(best) => {
            let ext = extract_corresponding_genomic_sequence_paired_end(&best, genome, counters)?;
            // R1 length guard first; on failure return BEFORE checking R2 (Perl
            // 3864→3867), each bumping the count by exactly 1.
            if ext.unmodified_genomic_sequence_1.len() != seq1_uc.len() + 2 {
                eprintln!(
                    "Chromosomal sequence could not be extracted for\t{identifier}\t{}\t{}",
                    best.chromosome, best.position_1
                );
                counters.genomic_sequence_could_not_be_extracted_count += 1;
                return Ok(());
            }
            if ext.unmodified_genomic_sequence_2.len() != seq2_uc.len() + 2 {
                eprintln!(
                    "Chromosomal sequence could not be extracted for\t{identifier}\t{}\t{}",
                    best.chromosome, best.position_2
                );
                counters.genomic_sequence_could_not_be_extracted_count += 1;
                return Ok(());
            }
            let mc1 = methylation_call(
                seq1_uc,
                &ext.unmodified_genomic_sequence_1,
                ext.read_conversion_1,
                false, // bisulfite polarity (frozen path)
                counters,
            );
            let mc2 = methylation_call(
                seq2_uc,
                &ext.unmodified_genomic_sequence_2,
                ext.read_conversion_2,
                false, // bisulfite polarity (frozen path)
                counters,
            );
            let (rec1, rec2) = paired_end_sam_output(
                identifier,
                seq1_uc,
                seq2_uc,
                qual1_bytes,
                qual2_bytes,
                &best,
                &ext,
                &mc1,
                &mc2,
                refid,
                config.phred64,
                dovetail,
            )?;
            write_record(&mut sinks.bam, &rec1)?;
            write_record(&mut sinks.bam, &rec2)?;
        }
        DecisionPaired::Ambiguous { first_ambig } => {
            if let Some(ab) = sinks.ambig_bam.as_mut()
                && let Some((l1, l2)) = first_ambig.as_ref()
            {
                write_raw_pe_ambig_lines(ab, l1, l2, refid)?;
            }
            // precedence: --ambiguous else --unmapped (Perl 2649/2663).
            let (route1, route2) = if sinks.ambiguous_1.is_some() {
                (sinks.ambiguous_1.as_mut(), sinks.ambiguous_2.as_mut())
            } else {
                (sinks.unmapped_1.as_mut(), sinks.unmapped_2.as_mut())
            };
            write_pe_aux(
                route1,
                route2,
                fasta,
                identifier,
                id2_stripped,
                seq1,
                plus1,
                qual1_bytes,
                seq2,
                plus2,
                qual2_bytes,
            )?;
        }
        DecisionPaired::NoAlignment => {
            write_pe_aux(
                sinks.unmapped_1.as_mut(),
                sinks.unmapped_2.as_mut(),
                fasta,
                identifier,
                id2_stripped,
                seq1,
                plus1,
                qual1_bytes,
                seq2,
                plus2,
                qual2_bytes,
            )?;
        }
        DecisionPaired::Rejected => {}
    }
    Ok(())
}

// ===========================================================================
// `--combined_index` (v2.x) paired-end directional path. Opt-in, never-silent,
// concordance-gated (NOT byte-identical). ONE both-strands Bowtie 2 `-1/-2` pass
// over the combined CT+GA index (`-k 2`, `Orientation::Both`) → `combined::select_pe`
// → the shared `route_pe_decision` (the byte-frozen PE output arm). PLAN
// 06102026_combined-index-v2x phase 2. Mirrors the SE combined block (run_se_combined
// / process_se_chunk_combined / drive_merge_combined) doubled for two mates.
// ===========================================================================

/// PE combined-index pipeline (single-core, directional Bowtie 2). Mirrors [`run_pe`]
/// but drives ONE both-strands instance over the combined index
/// ([`process_pe_chunk_combined`]) and announces the experimental, concordance-gated
/// mode (never-silent) on STDERR + in the report. The scope guard
/// (`reject_combined_index_unsupported`) guarantees directional Bowtie 2 here.
fn run_pe_combined(config: &RunConfig, mates1: &[String], mates2: &[String]) -> Result<()> {
    let started = Instant::now();
    let opts = convert::ConvertOptions::from_config(config);
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    let directional = matches!(config.library, LibraryType::Directional);
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    let tok = config.aligner.token();
    // The actual options Bowtie 2 is run with (faithful + `-k 2`) — shown in the
    // report header so it does not under-report the reporting mode.
    let combined_opts = combined_aligner_options(config);

    // Never-silent banner (STDERR). The resolve guard guarantees the index exists.
    if let Some(combined_basename) = &config.genome.combined_index_basename {
        eprintln!(
            ">>> Combined-index mode, paired-end (EXPERIMENTAL, concordance-gated — NOT \
             byte-identical to the faithful per-strand path): one both-strands {} pass over \
             {} (-k 2) <<<",
            config.aligner.name(),
            combined_basename.display()
        );
    }
    if config.ambig_bam {
        eprintln!(
            "Note: combined-index mode does not populate --ambig_bam records in this phase \
             (ambiguous reads are still written to --ambiguous/--unmapped if requested)."
        );
    }

    for (read_1, read_2) in mates1.iter().zip(mates2) {
        let bam_path =
            derive_output_path(read_1, config, &format!("_bismark_{tok}_pe.bam"), "_pe.bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_pe_sinks(read_1, read_2, config, &header, &bam_path)?;

        let report_path = derive_output_path(
            read_1,
            config,
            &format!("_bismark_{tok}_PE_report.txt"),
            "_PE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_1,
                sequence_file2: Some(read_2),
                genome_folder: &genome_folder,
                aligner_options: &combined_opts,
                aligner: config.aligner,
                library: config.library,
            },
        )?;
        // Never-silent: mark the report as combined-index mode.
        writeln!(
            report,
            "Combined-index mode (experimental, concordance-gated; NOT byte-identical to the \
             faithful per-strand path)"
        )?;

        let mut counters = Counters::default();
        // Directional PE combined: `-1 C→T_R1 -2 G→A_R2` (Ct, Ga) → OT/OB via select_pe.
        let converted = process_pe_chunk_combined(
            config,
            &genome,
            &refid,
            Path::new(read_1),
            Path::new(read_2),
            &opts,
            convert::ConvKind::Ct,
            convert::ConvKind::Ga,
            combined::select_pe,
            &mut sinks,
            &mut counters,
        )?;

        report::print_final_analysis_report_paired_ends(&mut report, &counters, directional)?;
        // Combined-mode extra: the spurious-discard tally.
        writeln!(
            report,
            "Spurious alignments discarded (combined-index; wrong sub-genome for the orientation):\t{}",
            counters.combined_spurious_count
        )?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;
        sinks.finish()?;

        // Per-mode temp cleanup: delete both converted temp files (C→T R1, G→A R2).
        for cr in &converted {
            let _ = std::fs::remove_file(&cr.path);
        }
        eprintln!("{}", counters_summary_pe(read_1, read_2, &counters));
    }
    Ok(())
}

/// PE combined-index pipeline, **PBAT** (single-core, Bowtie 2). Mirrors
/// [`run_pe_combined`] but the single both-strands PE pass is fed the **G→A-converted**
/// reads (`-1 G→A_R1 -2 C→T_R2`, `(Ga, Ct)`) → CTOT/CTOB, selected by
/// [`combined::select_pe_pbat`]. The report carries the `--pbat` library line
/// (`library = Pbat`) + the 4-strand final analysis (`directional=false`, only CTOT/CTOB
/// populated). PBAT-combined is the G→A-pass half of the non-directional PE path
/// (Phase 3) standalone — ONE pass, no union, no cross-pass desync guard. The scope
/// guard (`reject_combined_index_unsupported`) guarantees Bowtie 2 pbat here.
fn run_pe_combined_pbat(config: &RunConfig, mates1: &[String], mates2: &[String]) -> Result<()> {
    let started = Instant::now();
    let opts = convert::ConvertOptions::from_config(config);
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    // PBAT → the report prints the 4-strand split (only CTOT/CTOB populated).
    let directional = false;
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    let tok = config.aligner.token();
    let combined_opts = combined_aligner_options(config);

    // Never-silent banner (STDERR). The resolve guard guarantees the index exists.
    if let Some(combined_basename) = &config.genome.combined_index_basename {
        eprintln!(
            ">>> Combined-index mode, paired-end PBAT (EXPERIMENTAL, concordance-gated — NOT \
             byte-identical to the faithful 2-instance path): one both-strands {} PE pass over \
             {} (-k 2) on the G->A-converted reads (-1 G->A_R1 -2 C->T_R2) → CTOT/CTOB <<<",
            config.aligner.name(),
            combined_basename.display()
        );
    }
    if config.ambig_bam {
        eprintln!(
            "Note: combined-index mode does not populate --ambig_bam records in this phase \
             (ambiguous reads are still written to --ambiguous/--unmapped if requested)."
        );
    }

    for (read_1, read_2) in mates1.iter().zip(mates2) {
        let bam_path =
            derive_output_path(read_1, config, &format!("_bismark_{tok}_pe.bam"), "_pe.bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_pe_sinks(read_1, read_2, config, &header, &bam_path)?;

        let report_path = derive_output_path(
            read_1,
            config,
            &format!("_bismark_{tok}_PE_report.txt"),
            "_PE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_1,
                sequence_file2: Some(read_2),
                genome_folder: &genome_folder,
                aligner_options: &combined_opts,
                aligner: config.aligner,
                library: config.library,
            },
        )?;
        // Never-silent: mark the report as combined-index PBAT mode.
        writeln!(
            report,
            "Combined-index mode, PBAT (experimental, concordance-gated; NOT byte-identical to the \
             faithful 2-instance per-strand path)"
        )?;

        let mut counters = Counters::default();
        // PBAT PE combined: `-1 G→A_R1 -2 C→T_R2` (Ga, Ct) → CTOT/CTOB via select_pe_pbat.
        let converted = process_pe_chunk_combined(
            config,
            &genome,
            &refid,
            Path::new(read_1),
            Path::new(read_2),
            &opts,
            convert::ConvKind::Ga,
            convert::ConvKind::Ct,
            combined::select_pe_pbat,
            &mut sinks,
            &mut counters,
        )?;

        report::print_final_analysis_report_paired_ends(&mut report, &counters, directional)?;
        // Combined-mode extra: the spurious-discard tally.
        writeln!(
            report,
            "Spurious alignments discarded (combined-index; wrong sub-genome for the orientation):\t{}",
            counters.combined_spurious_count
        )?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;
        sinks.finish()?;

        // Per-mode temp cleanup: delete both converted temp files (G→A R1, C→T R2).
        for cr in &converted {
            let _ = std::fs::remove_file(&cr.path);
        }
        eprintln!("{}", counters_summary_pe(read_1, read_2, &counters));
    }
    Ok(())
}

/// Convert R1 and R2 per the given `(k1, k2)` conversion kinds, spawn ONE both-strands
/// Bowtie 2 instance over the combined index with `-k 2`, and drive the per-pair
/// classify→select→route with the given `select_fn`. Returns the converted temp files
/// for cleanup. Mirrors [`process_se_chunk_combined`] (also `SelectFn`-parametrized)
/// doubled for two mates. The two single-stream PE library types pass:
/// - **directional**: `(Ct, Ga)` + [`combined::select_pe`] — `-1 C→T_R1 -2 G→A_R2` → OT/OB;
/// - **pbat**: `(Ga, Ct)` + [`combined::select_pe_pbat`] — `-1 G→A_R1 -2 C→T_R2` → CTOT/CTOB.
#[allow(clippy::too_many_arguments)]
fn process_pe_chunk_combined(
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    read_1: &Path,
    read_2: &Path,
    opts: &convert::ConvertOptions,
    k1: convert::ConvKind,
    k2: convert::ConvKind,
    select_fn: SelectFnPe,
    sinks: &mut PeSinks,
    counters: &mut Counters,
) -> Result<Vec<convert::ConvertedReads>> {
    let bt2 = &config.detected_aligner.path;
    let fasta = matches!(config.format, ReadFormat::FastA);
    let td = &config.output.temp_dir;

    let cr1 = convert_pe_kind(fasta, read_1, td, opts, 1, k1)?;
    eprintln!(
        "Created {} converted version of {} -> {} ({} sequences)",
        conv_label(&cr1.name),
        read_1.display(),
        cr1.path.display(),
        cr1.count
    );
    let cr2 = convert_pe_kind(fasta, read_2, td, opts, 2, k2)?;
    eprintln!(
        "Created {} converted version of {} -> {} ({} sequences)",
        conv_label(&cr2.name),
        read_2.display(),
        cr2.path.display(),
        cr2.count
    );

    let combined_basename = config
        .genome
        .combined_index_basename
        .as_ref()
        .ok_or_else(|| {
            AlignerError::Validation(
                "internal error: --combined_index reached alignment without a combined index"
                    .into(),
            )
        })?;
    // Same option string the report header advertises (`combined_aligner_options`).
    let combined_opts = combined_aligner_options(config);

    // ONE both-strands PE instance: `Orientation::Both` emits no `--norc`/`--nofw`, so
    // the single pass searches both sub-genomes; `-k 2` surfaces the cross-sub-genome
    // runner-up for MAPQ + the spurious/ambiguity gate (PLAN §select_core_pe).
    let mut stream = PairedAlignerStream::spawn(
        config.aligner,
        bt2,
        &combined_opts,
        Orientation::Both,
        combined_basename,
        &cr1.path,
        &cr2.path,
    )?;
    // `$dovetail` from config (NOT a scan of aligner_options — Bowtie 2 here, so they
    // agree, but be consistent with the faithful `process_pe_chunk`).
    let dovetail = config.dovetail;
    drive_merge_combined_pe(
        read_1,
        read_2,
        &mut stream,
        config,
        genome,
        refid,
        dovetail,
        select_fn,
        sinks,
        counters,
    )?;
    stream.finish()?;
    Ok(vec![cr1, cr2])
}

/// Re-read both original read files in lockstep and, per pair, gather the combined PE
/// stream's `-k` group (consecutive same-`seq_id` [`SamPair`]s — Bowtie 2 emits a
/// pair's k alignments contiguously), run the provided `select_fn` (`combined::select_pe`
/// for directional, `combined::select_pe_pbat` for pbat), and route the
/// [`DecisionPaired`] through the shared [`route_pe_decision`]. Generic over
/// [`PairedSamStream`] so it is unit-testable with a canned pair stream. Mirrors
/// [`drive_merge_combined`] (SE, also `SelectFn`-parametrized) doubled for two mates;
/// the FastQ/FastA read loop + `skip`/`upto`/`icpc`/per-mate-quality handling is copied
/// from [`drive_merge_pe`]. Parametrizing `select_fn` lets the directional + pbat
/// single-stream PE paths share this gather loop (the dual-driver back-port trap).
///
/// No desync sentinel (cf. [`drive_merge_combined`]): the whole contiguous same-id
/// run is drained in one pass, so a pair's lines cannot reappear at the head after we
/// move on; output order rests on the same Bowtie 2 `--reorder`-under-`-p` invariant
/// the faithful path relies on. A miss is one `(77,141)` pair, filtered by the selector.
#[allow(clippy::too_many_arguments)]
fn drive_merge_combined_pe<S: PairedSamStream>(
    read_1: &Path,
    read_2: &Path,
    stream: &mut S,
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    dovetail: bool,
    select_fn: SelectFnPe,
    sinks: &mut PeSinks,
    counters: &mut Counters,
) -> Result<()> {
    let mut r1 = open_reader(read_1)?;
    let mut r2 = open_reader(read_2)?;
    let fasta = matches!(config.format, ReadFormat::FastA);
    let (skip, upto, icpc) = (
        config.read_processing.skip,
        config.read_processing.upto,
        config.read_processing.icpc,
    );

    let (mut id1, mut seq1, mut plus1, mut qual1) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut id2, mut seq2, mut plus2, mut qual2) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut count: u64 = 0;
    loop {
        for v in [
            &mut id1, &mut seq1, &mut plus1, &mut qual1, &mut id2, &mut seq2, &mut plus2,
            &mut qual2,
        ] {
            v.clear();
        }
        let n_id1 = r1.read_until(b'\n', &mut id1)?;
        let n_seq1 = r1.read_until(b'\n', &mut seq1)?;
        if !fasta {
            let _ = r1.read_until(b'\n', &mut plus1)?;
            let _ = r1.read_until(b'\n', &mut qual1)?;
        }
        let n_id2 = r2.read_until(b'\n', &mut id2)?;
        let n_seq2 = r2.read_until(b'\n', &mut seq2)?;
        if !fasta {
            let _ = r2.read_until(b'\n', &mut plus2)?;
            let _ = r2.read_until(b'\n', &mut qual2)?;
        }
        let incomplete = if fasta {
            n_id1 == 0 || n_seq1 == 0 || n_id2 == 0 || n_seq2 == 0
        } else {
            n_id1 == 0
                || n_seq1 == 0
                || qual1.is_empty()
                || n_id2 == 0
                || n_seq2 == 0
                || qual2.is_empty()
        };
        if incomplete {
            break;
        }
        count += 1;
        if let Some(s) = skip
            && s > 0
            && count <= s
        {
            continue;
        }
        if let Some(u) = upto
            && u > 0
            && count > u
        {
            break;
        }
        counters.sequences_count += 1;

        let id1_fixed = convert::fix_id(convert::chomp_newline(&id1), icpc);
        let id2_fixed = convert::fix_id(convert::chomp_newline(&id2), icpc);
        let id_prefix: &[u8] = if fasta { b">" } else { b"@" };
        let identifier =
            String::from_utf8_lossy(id1_fixed.strip_prefix(id_prefix).unwrap_or(&id1_fixed))
                .into_owned();
        let id2_stripped =
            String::from_utf8_lossy(id2_fixed.strip_prefix(id_prefix).unwrap_or(&id2_fixed))
                .into_owned();
        let seq1_uc: Vec<u8> = convert::chomp_newline(&seq1).to_ascii_uppercase();
        let seq2_uc: Vec<u8> = convert::chomp_newline(&seq2).to_ascii_uppercase();
        let qual1_bytes: Vec<u8> = if fasta {
            vec![b'I'; seq1_uc.len()]
        } else {
            convert::chomp_newline(&qual1).to_vec()
        };
        let qual2_bytes: Vec<u8> = if fasta {
            vec![b'I'; seq2_uc.len()]
        } else {
            convert::chomp_newline(&qual2).to_vec()
        };
        let s1 = String::from_utf8_lossy(&seq1_uc).into_owned();
        let s2 = String::from_utf8_lossy(&seq2_uc).into_owned();

        // Gather this pair's `-k` group: all consecutive same-`seq_id` pairs.
        let mut pairs: Vec<crate::align::SamPair> = Vec::new();
        while stream
            .current_pair()
            .is_some_and(|p| p.seq_id == identifier)
        {
            pairs.push(stream.current_pair().unwrap().clone());
            stream.advance_pair()?;
        }

        let decision = select_fn(
            &pairs,
            &s1,
            &s2,
            config.score_min_intercept,
            config.score_min_slope,
            config.score_min_local,
            counters,
        )?;
        route_pe_decision(
            decision,
            &identifier,
            &id2_stripped,
            &seq1_uc,
            &seq2_uc,
            &qual1_bytes,
            &qual2_bytes,
            &seq1,
            &plus1,
            &seq2,
            &plus2,
            fasta,
            genome,
            refid,
            dovetail,
            config,
            sinks,
            counters,
        )?;
    }
    Ok(())
}

// ===========================================================================
// `--combined_index` (v2.x) paired-end NON-DIRECTIONAL path (parallel model (a)).
// Opt-in, never-silent, concordance-gated (NOT byte-identical). TWO both-strands
// Bowtie 2 PE passes over the combined CT+GA index — C→T-converted reads
// (`-1 C→T_R1 -2 G→A_R2` → OT/OB) + G→A-converted reads (`-1 G→A_R1 -2 C→T_R2` →
// CTOT/CTOB) — unioned per read pair → `combined::select_pe_nondir` → the shared
// `route_pe_decision`. PLAN 06102026 phase 3. Mirrors the SE non-dir block
// (run_se_combined_nondir / process_se_chunk_combined_nondir /
// drive_merge_combined_nondir / select_and_route_se_nondir) doubled for two mates,
// reusing the Phase-2 `select_core_pe` (4-slot-ready) + `route_pe_decision` unchanged.
// ===========================================================================

/// PE combined-index pipeline, **non-directional** (single-core, parallel model (a)).
/// Mirrors [`run_pe_combined`] but drives TWO both-strands PE passes
/// ([`process_pe_chunk_combined_nondir`]); the report prints the 4-strand split
/// (`directional=false`).
fn run_pe_combined_nondir(config: &RunConfig, mates1: &[String], mates2: &[String]) -> Result<()> {
    let started = Instant::now();
    let opts = convert::ConvertOptions::from_config(config);
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    // Non-directional → the report prints the 4-strand (OT/OB/CTOT/CTOB) split.
    let directional = false;
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    let tok = config.aligner.token();
    let combined_opts = combined_aligner_options(config);

    if let Some(combined_basename) = &config.genome.combined_index_basename {
        eprintln!(
            ">>> Combined-index mode, paired-end NON-DIRECTIONAL (EXPERIMENTAL, concordance-gated \
             — NOT byte-identical to the faithful 4-instance path): two both-strands {} passes \
             (C->T + G->A reads) over {} (-k 2), unioned per pair <<<",
            config.aligner.name(),
            combined_basename.display()
        );
    }
    if config.ambig_bam {
        eprintln!(
            "Note: combined-index mode does not populate --ambig_bam records in this phase \
             (ambiguous reads are still written to --ambiguous/--unmapped if requested)."
        );
    }

    for (read_1, read_2) in mates1.iter().zip(mates2) {
        let bam_path =
            derive_output_path(read_1, config, &format!("_bismark_{tok}_pe.bam"), "_pe.bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_pe_sinks(read_1, read_2, config, &header, &bam_path)?;

        let report_path = derive_output_path(
            read_1,
            config,
            &format!("_bismark_{tok}_PE_report.txt"),
            "_PE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_1,
                sequence_file2: Some(read_2),
                genome_folder: &genome_folder,
                aligner_options: &combined_opts,
                aligner: config.aligner,
                library: config.library,
            },
        )?;
        writeln!(
            report,
            "Combined-index mode, non-directional (experimental, concordance-gated; NOT \
             byte-identical to the faithful 4-instance per-strand path)"
        )?;

        let mut counters = Counters::default();
        let converted = process_pe_chunk_combined_nondir(
            config,
            &genome,
            &refid,
            Path::new(read_1),
            Path::new(read_2),
            &opts,
            &mut sinks,
            &mut counters,
        )?;

        report::print_final_analysis_report_paired_ends(&mut report, &counters, directional)?;
        writeln!(
            report,
            "Spurious alignments discarded (combined-index; wrong sub-genome for the orientation):\t{}",
            counters.combined_spurious_count
        )?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;
        sinks.finish()?;

        // Per-mode temp cleanup: the 4 converted files (R1/R2 × C→T/G→A).
        for cr in &converted {
            let _ = std::fs::remove_file(&cr.path);
        }
        eprintln!("{}", counters_summary_pe(read_1, read_2, &counters));
    }
    Ok(())
}

/// Convert the 4 `(mate,kind)` files the non-dir two-pass model needs and spawn TWO
/// both-strands PE instances over the combined index with `-k 2`: **pass 1 (C→T reads)**
/// `-1 (R1,Ct) -2 (R2,Ga)` → OT/OB, **pass 2 (G→A reads)** `-1 (R1,Ga) -2 (R2,Ct)` →
/// CTOT/CTOB (the `pe_instance_plan` NonDirectional layout, collapsed to two both-strands
/// passes). Drive the per-pair UNION classify→select→route. Returns the 4 converted temp
/// files for cleanup. Mirrors [`process_se_chunk_combined_nondir`] doubled. The two
/// passes run concurrently → ~2× the combined index resident (model (a)'s RAM cost — the
/// number Phase 3 measures to gate Phase 6).
#[allow(clippy::too_many_arguments)]
fn process_pe_chunk_combined_nondir(
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    read_1: &Path,
    read_2: &Path,
    opts: &convert::ConvertOptions,
    sinks: &mut PeSinks,
    counters: &mut Counters,
) -> Result<Vec<convert::ConvertedReads>> {
    let bt2 = &config.detected_aligner.path;
    let fasta = matches!(config.format, ReadFormat::FastA);
    let td = &config.output.temp_dir;

    // 4 converted files: pass 1 = (R1 C→T, R2 G→A); pass 2 = (R1 G→A, R2 C→T).
    let r1_ct = convert_pe_kind(fasta, read_1, td, opts, 1, convert::ConvKind::Ct)?;
    let r2_ga = convert_pe_kind(fasta, read_2, td, opts, 2, convert::ConvKind::Ga)?;
    let r1_ga = convert_pe_kind(fasta, read_1, td, opts, 1, convert::ConvKind::Ga)?;
    let r2_ct = convert_pe_kind(fasta, read_2, td, opts, 2, convert::ConvKind::Ct)?;
    for (cr, src) in [
        (&r1_ct, read_1),
        (&r2_ga, read_2),
        (&r1_ga, read_1),
        (&r2_ct, read_2),
    ] {
        eprintln!(
            "Created {} converted version of {} -> {} ({} sequences)",
            conv_label(&cr.name),
            src.display(),
            cr.path.display(),
            cr.count
        );
    }

    let combined_basename = config
        .genome
        .combined_index_basename
        .as_ref()
        .ok_or_else(|| {
            AlignerError::Validation(
                "internal error: --combined_index reached alignment without a combined index"
                    .into(),
            )
        })?;
    let combined_opts = combined_aligner_options(config);

    // Two both-strands PE passes (`Orientation::Both` → no --norc/--nofw), differing only
    // by the converted-read inputs: ct_stream = C→T reads (→ OT/OB), ga_stream = G→A reads
    // (→ CTOT/CTOB). Two concurrent subprocesses → ~2× the combined index resident.
    let mut ct_stream = PairedAlignerStream::spawn(
        config.aligner,
        bt2,
        &combined_opts,
        Orientation::Both,
        combined_basename,
        &r1_ct.path,
        &r2_ga.path,
    )?;
    let mut ga_stream = PairedAlignerStream::spawn(
        config.aligner,
        bt2,
        &combined_opts,
        Orientation::Both,
        combined_basename,
        &r1_ga.path,
        &r2_ct.path,
    )?;
    let dovetail = config.dovetail;
    drive_merge_combined_pe_nondir(
        read_1,
        read_2,
        &mut ct_stream,
        &mut ga_stream,
        config,
        genome,
        refid,
        dovetail,
        sinks,
        counters,
    )?;
    ct_stream.finish()?;
    ga_stream.finish()?;
    Ok(vec![r1_ct, r2_ga, r1_ga, r2_ct])
}

/// Never-silent desync check for one pass of the non-directional PE two-stream gather:
/// the pass's current head pair (if any) MUST belong to `identifier` (keyed on
/// `SamPair::seq_id`, the `/1`-stripped id — NOT `qname`). A `None` head (the pass is at
/// EOF / already past this read) is in-sync. Fails loud on a mismatch — the two-stream
/// gather has a larger blast radius than the directional single stream, so a desync from
/// the re-read input dies rather than silently mis-pairs every downstream read. Extracted
/// from [`drive_merge_combined_pe_nondir`] so it is unit-testable without a `PeSinks` mock.
fn assert_pe_pass_in_sync(
    head: Option<&crate::align::SamPair>,
    identifier: &str,
    pass: &str,
) -> Result<()> {
    if let Some(p) = head
        && p.seq_id != identifier
    {
        return Err(AlignerError::Validation(format!(
            "Combined non-directional PE desync: {pass} pass stream head is '{}' but expected '{identifier}'",
            p.seq_id
        )));
    }
    Ok(())
}

/// Re-read both original read files in lockstep and, per pair, gather the C→T pass's
/// `-k 2` `SamPair` run AND the G→A pass's run (each a contiguous same-`seq_id` group),
/// union-select ([`combined::select_pe_nondir`]), and route through the shared
/// [`route_pe_decision`]. Generic over TWO [`PairedSamStream`] params (mirrors the SE
/// non-dir `<C, G>` — so a future Phase-6 sequential PE variant can pass a file-backed
/// stream for one pass). Mirrors [`drive_merge_combined_nondir`] doubled; the read loop is
/// copied from [`drive_merge_combined_pe`].
#[allow(clippy::too_many_arguments)]
fn drive_merge_combined_pe_nondir<C: PairedSamStream, G: PairedSamStream>(
    read_1: &Path,
    read_2: &Path,
    ct_stream: &mut C,
    ga_stream: &mut G,
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    dovetail: bool,
    sinks: &mut PeSinks,
    counters: &mut Counters,
) -> Result<()> {
    let mut r1 = open_reader(read_1)?;
    let mut r2 = open_reader(read_2)?;
    let fasta = matches!(config.format, ReadFormat::FastA);
    let (skip, upto, icpc) = (
        config.read_processing.skip,
        config.read_processing.upto,
        config.read_processing.icpc,
    );

    let (mut id1, mut seq1, mut plus1, mut qual1) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut id2, mut seq2, mut plus2, mut qual2) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut count: u64 = 0;
    loop {
        for v in [
            &mut id1, &mut seq1, &mut plus1, &mut qual1, &mut id2, &mut seq2, &mut plus2,
            &mut qual2,
        ] {
            v.clear();
        }
        let n_id1 = r1.read_until(b'\n', &mut id1)?;
        let n_seq1 = r1.read_until(b'\n', &mut seq1)?;
        if !fasta {
            let _ = r1.read_until(b'\n', &mut plus1)?;
            let _ = r1.read_until(b'\n', &mut qual1)?;
        }
        let n_id2 = r2.read_until(b'\n', &mut id2)?;
        let n_seq2 = r2.read_until(b'\n', &mut seq2)?;
        if !fasta {
            let _ = r2.read_until(b'\n', &mut plus2)?;
            let _ = r2.read_until(b'\n', &mut qual2)?;
        }
        let incomplete = if fasta {
            n_id1 == 0 || n_seq1 == 0 || n_id2 == 0 || n_seq2 == 0
        } else {
            n_id1 == 0
                || n_seq1 == 0
                || qual1.is_empty()
                || n_id2 == 0
                || n_seq2 == 0
                || qual2.is_empty()
        };
        if incomplete {
            break;
        }
        count += 1;
        if let Some(s) = skip
            && s > 0
            && count <= s
        {
            continue;
        }
        if let Some(u) = upto
            && u > 0
            && count > u
        {
            break;
        }
        counters.sequences_count += 1;

        let id1_fixed = convert::fix_id(convert::chomp_newline(&id1), icpc);
        let id2_fixed = convert::fix_id(convert::chomp_newline(&id2), icpc);
        let id_prefix: &[u8] = if fasta { b">" } else { b"@" };
        let identifier =
            String::from_utf8_lossy(id1_fixed.strip_prefix(id_prefix).unwrap_or(&id1_fixed))
                .into_owned();
        let id2_stripped =
            String::from_utf8_lossy(id2_fixed.strip_prefix(id_prefix).unwrap_or(&id2_fixed))
                .into_owned();
        let seq1_uc: Vec<u8> = convert::chomp_newline(&seq1).to_ascii_uppercase();
        let seq2_uc: Vec<u8> = convert::chomp_newline(&seq2).to_ascii_uppercase();
        let qual1_bytes: Vec<u8> = if fasta {
            vec![b'I'; seq1_uc.len()]
        } else {
            convert::chomp_newline(&qual1).to_vec()
        };
        let qual2_bytes: Vec<u8> = if fasta {
            vec![b'I'; seq2_uc.len()]
        } else {
            convert::chomp_newline(&qual2).to_vec()
        };
        let s1 = String::from_utf8_lossy(&seq1_uc).into_owned();
        let s2 = String::from_utf8_lossy(&seq2_uc).into_owned();

        // Never-silent desync guard (mirror `drive_merge_combined_nondir`): each pass
        // emits ≥1 pair per input pair (a `-k` run OR one `(77,141)` miss), both converted
        // from the same reads in the same order with the same skip/upto → each stream's
        // head pair MUST be this read pair (keyed on `SamPair::seq_id`). Die loud on
        // desync — the two-stream PE gather has a larger blast radius than Phase-2's
        // single stream. (Extracted to `assert_pe_pass_in_sync` so it's unit-testable.)
        assert_pe_pass_in_sync(ct_stream.current_pair(), &identifier, "C->T")?;
        assert_pe_pass_in_sync(ga_stream.current_pair(), &identifier, "G->A")?;

        // Drain BOTH passes' contiguous same-`seq_id` pair runs (incl. a lone `(77,141)`
        // miss — filtered by `select_pe_nondir`). Draining both before advancing keeps the
        // streams in lockstep with the re-read input.
        let mut ct_pairs: Vec<crate::align::SamPair> = Vec::new();
        while ct_stream
            .current_pair()
            .is_some_and(|p| p.seq_id == identifier)
        {
            ct_pairs.push(ct_stream.current_pair().unwrap().clone());
            ct_stream.advance_pair()?;
        }
        let mut ga_pairs: Vec<crate::align::SamPair> = Vec::new();
        while ga_stream
            .current_pair()
            .is_some_and(|p| p.seq_id == identifier)
        {
            ga_pairs.push(ga_stream.current_pair().unwrap().clone());
            ga_stream.advance_pair()?;
        }

        select_and_route_pe_nondir(
            &ct_pairs,
            &ga_pairs,
            &identifier,
            &id2_stripped,
            &s1,
            &s2,
            &seq1_uc,
            &seq2_uc,
            &qual1_bytes,
            &qual2_bytes,
            &seq1,
            &plus1,
            &seq2,
            &plus2,
            fasta,
            genome,
            refid,
            dovetail,
            config,
            sinks,
            counters,
        )?;
    }
    Ok(())
}

/// Shared per-pair tail of the non-directional PE combined driver: union-select the
/// C→T-pass + G→A-pass pair groups ([`combined::select_pe_nondir`]) and route the
/// [`DecisionPaired`] through the byte-frozen [`route_pe_decision`]. Extracted (one
/// caller today) so a future Phase-6 PE sequential/single-pass driver reuses the SAME
/// select+route contract (the dual-driver back-port trap). Mirrors
/// [`select_and_route_se_nondir`].
#[allow(clippy::too_many_arguments)]
fn select_and_route_pe_nondir(
    ct_pairs: &[crate::align::SamPair],
    ga_pairs: &[crate::align::SamPair],
    identifier: &str,
    id2_stripped: &str,
    s1: &str,
    s2: &str,
    seq1_uc: &[u8],
    seq2_uc: &[u8],
    qual1_bytes: &[u8],
    qual2_bytes: &[u8],
    seq1: &[u8],
    plus1: &[u8],
    seq2: &[u8],
    plus2: &[u8],
    fasta: bool,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    dovetail: bool,
    config: &RunConfig,
    sinks: &mut PeSinks,
    counters: &mut Counters,
) -> Result<()> {
    let decision = combined::select_pe_nondir(
        ct_pairs,
        ga_pairs,
        s1,
        s2,
        config.score_min_intercept,
        config.score_min_slope,
        config.score_min_local,
        counters,
    )?;
    route_pe_decision(
        decision,
        identifier,
        id2_stripped,
        seq1_uc,
        seq2_uc,
        qual1_bytes,
        qual2_bytes,
        seq1,
        plus1,
        seq2,
        plus2,
        fasta,
        genome,
        refid,
        dovetail,
        config,
        sinks,
        counters,
    )
}

/// Write a pair's two records to the routed `_1`/`_2` aux files (Perl 2649–2674).
/// FastQ = `@<id>\n<orig non-uc seq>\n<verbatim + line><qual>\n`; FastA = 2-line
/// `>id\nseq` (the `+`/qual args are ignored — Phase 9a).
#[allow(clippy::too_many_arguments)]
fn write_pe_aux(
    route1: Option<&mut AuxWriter>,
    route2: Option<&mut AuxWriter>,
    fasta: bool,
    id1: &str,
    id2: &str,
    seq1: &[u8],
    plus1: &[u8],
    qual1: &[u8],
    seq2: &[u8],
    plus2: &[u8],
    qual2: &[u8],
) -> Result<()> {
    if let Some(w) = route1 {
        let s = convert::chomp_newline(seq1).to_vec();
        write_se_aux_record(w, fasta, id1.as_bytes(), &s, plus1, qual1)?;
    }
    if let Some(w) = route2 {
        let s = convert::chomp_newline(seq2).to_vec();
        write_se_aux_record(w, fasta, id2.as_bytes(), &s, plus2, qual2)?;
    }
    Ok(())
}

fn counters_summary_pe(read_1: &str, read_2: &str, c: &Counters) -> String {
    format!(
        "Mapping summary for {read_1} / {read_2}:\n\
           sequence pairs analysed:  {}\n\
           unique best alignments:   {}\n\
           no alignment found:       {}\n\
           ambiguous (unsuitable):   {}\n\
           directional-rejected:     {}\n\
           could-not-extract genomic:{}\n\
           strand OT/CTOB/CTOT/OB:   {}/{}/{}/{}",
        c.sequences_count,
        c.unique_best_alignment_count,
        c.no_single_alignment_found,
        c.unsuitable_sequence_count,
        c.alignments_rejected_count,
        c.genomic_sequence_could_not_be_extracted_count,
        c.ct_ga_ct_count,
        c.ga_ct_ga_count,
        c.ga_ct_ct_count,
        c.ct_ga_ga_count,
    )
}

fn counters_summary(read_file: &str, c: &Counters) -> String {
    format!(
        "Mapping summary for {read_file}:\n\
           sequences analysed:       {}\n\
           unique best alignments:   {}\n\
           no alignment found:       {}\n\
           ambiguous (unsuitable):   {}\n\
           directional-rejected:     {}\n\
           could-not-extract genomic:{}\n\
           strand OT/OB/CTOT/CTOB:   {}/{}/{}/{}\n\
           (full report layout is Phase 6)",
        c.sequences_count,
        c.unique_best_alignment_count,
        c.no_single_alignment_found,
        c.unsuitable_sequence_count,
        c.alignments_rejected_count,
        c.genomic_sequence_could_not_be_extracted_count,
        c.ct_ct_count,
        c.ct_ga_count,
        c.ga_ct_count,
        c.ga_ga_count,
    )
}

// ===========================================================================
// `--combined_index` (v2.x) paired-end NON-DIRECTIONAL **low-RAM variants** (PLAN
// 06102026 phase 6). Both cut model (a)'s two-co-resident-index peak RSS (~15.71 GB)
// to ~half (one index resident at a time). NON-DIRECTIONAL + Bowtie 2-only (the config
// guard enforces both). PE analogs of the shipped SE variants:
//   1. SEQUENTIAL (`run_pe_combined_nondir_sequential`) — the FAITHFUL model: run model
//      (a)'s two both-strands PE passes one at a time (pass 1 spills + EXITS, freeing the
//      index, before pass 2 spawns), replaying pass 1 from disk. BYTE-IDENTICAL to model
//      (a) (the aligner's output for a pass is independent of *when* it runs; both feed
//      the SAME untagged converted files). PE analog of SE #959.
//   2. SINGLE-PASS model (b) (below) — ONE PE pass over conversion-TAGGED interleaved
//      reads. NON-FAITHFUL (the qname tag perturbs Bowtie 2's read-name RNG). PE analog
//      of SE #958.
// Both reuse the byte-frozen `drive_merge_combined_pe_nondir` (already generic over two
// `PairedSamStream`s — Phase-3 foresight) / `select_pe_nondir` / `select_and_route_pe_
// nondir` / `route_pe_decision` unchanged. Only the GATHER differs.
// ===========================================================================

/// PE combined-index pipeline, **non-directional SEQUENTIAL** (faithful low-RSS variant).
/// Mirrors [`run_pe_combined_nondir`] (parallel model (a)) but with the sequential
/// banner/marker + [`process_pe_chunk_combined_nondir_sequential`]. BYTE-IDENTICAL to
/// model (a) — its BAM gate is "same md5 as model (a)" + the RSS measurement, NOT a fresh
/// accuracy gate (see the section header / the PLAN assumptions).
fn run_pe_combined_nondir_sequential(
    config: &RunConfig,
    mates1: &[String],
    mates2: &[String],
) -> Result<()> {
    let started = Instant::now();
    let opts = convert::ConvertOptions::from_config(config);
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    // Non-directional → the report prints the 4-strand (OT/OB/CTOT/CTOB) split.
    let directional = false;
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    let tok = config.aligner.token();
    let combined_opts = combined_aligner_options(config);

    if let Some(combined_basename) = &config.genome.combined_index_basename {
        eprintln!(
            ">>> Combined-index mode, paired-end NON-DIRECTIONAL SEQUENTIAL (EXPERIMENTAL, \
             concordance-gated — byte-identical to the default PARALLEL combined non-dir path, \
             NOT to the faithful 4-instance path): two both-strands {} PE passes (C->T then G->A \
             reads) over {} (-k 2) run ONE AT A TIME — pass 1 exits before pass 2 starts, so one \
             combined index is resident at a time (~half the peak RSS, ~2x the wall), unioned per \
             pair <<<",
            config.aligner.name(),
            combined_basename.display()
        );
    }
    if config.ambig_bam {
        eprintln!(
            "Note: combined-index mode does not populate --ambig_bam records in this phase \
             (ambiguous reads are still written to --ambiguous/--unmapped if requested)."
        );
    }

    for (read_1, read_2) in mates1.iter().zip(mates2) {
        let bam_path =
            derive_output_path(read_1, config, &format!("_bismark_{tok}_pe.bam"), "_pe.bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_pe_sinks(read_1, read_2, config, &header, &bam_path)?;

        let report_path = derive_output_path(
            read_1,
            config,
            &format!("_bismark_{tok}_PE_report.txt"),
            "_PE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_1,
                sequence_file2: Some(read_2),
                genome_folder: &genome_folder,
                aligner_options: &combined_opts,
                aligner: config.aligner,
                library: config.library,
            },
        )?;
        // Never-silent: mark the report as combined-index non-directional SEQUENTIAL.
        writeln!(
            report,
            "Combined-index mode, non-directional SEQUENTIAL (experimental, concordance-gated; \
             byte-identical to the default parallel combined non-dir path, one index resident at a \
             time; NOT byte-identical to the faithful 4-instance per-strand path)"
        )?;

        let mut counters = Counters::default();
        let converted = process_pe_chunk_combined_nondir_sequential(
            config,
            &genome,
            &refid,
            Path::new(read_1),
            Path::new(read_2),
            &opts,
            &mut sinks,
            &mut counters,
        )?;

        report::print_final_analysis_report_paired_ends(&mut report, &counters, directional)?;
        writeln!(
            report,
            "Spurious alignments discarded (combined-index; wrong sub-genome for the orientation):\t{}",
            counters.combined_spurious_count
        )?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;
        sinks.finish()?;

        for cr in &converted {
            let _ = std::fs::remove_file(&cr.path);
        }
        eprintln!("{}", counters_summary_pe(read_1, read_2, &counters));
    }
    Ok(())
}

/// Convert the 4 `(mate,kind)` files model (a) needs (the SAME untagged files, so
/// byte-identity holds), then run the two both-strands PE passes SEQUENTIALLY: spawn
/// **pass 1 (C→T reads)** `-1 (R1,Ct) -2 (R2,Ga)`, **spill its pairs to a temp file and
/// `finish()` it** (its Bowtie 2 exits → the combined index is freed), THEN spawn
/// **pass 2 (G→A reads)** `-1 (R1,Ga) -2 (R2,Ct)` and drive the per-pair union with pass
/// 1 replayed from disk ([`PairedFileSamStream`]) against the live pass-2 stream — via the
/// byte-frozen (Phase-3-widened) [`drive_merge_combined_pe_nondir`]. ONE combined index
/// resident at a time (the ~−50% RSS win vs model (a)'s two co-resident loads). Returns
/// the 4 converted temp files for cleanup (the pass-1 spill is deleted here). The PE
/// analog of [`process_se_chunk_combined_nondir_sequential`] doubled.
#[allow(clippy::too_many_arguments)]
fn process_pe_chunk_combined_nondir_sequential(
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    read_1: &Path,
    read_2: &Path,
    opts: &convert::ConvertOptions,
    sinks: &mut PeSinks,
    counters: &mut Counters,
) -> Result<Vec<convert::ConvertedReads>> {
    let bt2 = &config.detected_aligner.path;
    let fasta = matches!(config.format, ReadFormat::FastA);
    let td = &config.output.temp_dir;

    // 4 converted files: pass 1 = (R1 C→T, R2 G→A); pass 2 = (R1 G→A, R2 C→T) — IDENTICAL
    // to parallel model (a) (`process_pe_chunk_combined_nondir`), so the alignments — and
    // thus the BAM — are byte-identical regardless of the exec model.
    let r1_ct = convert_pe_kind(fasta, read_1, td, opts, 1, convert::ConvKind::Ct)?;
    let r2_ga = convert_pe_kind(fasta, read_2, td, opts, 2, convert::ConvKind::Ga)?;
    let r1_ga = convert_pe_kind(fasta, read_1, td, opts, 1, convert::ConvKind::Ga)?;
    let r2_ct = convert_pe_kind(fasta, read_2, td, opts, 2, convert::ConvKind::Ct)?;
    for (cr, src) in [
        (&r1_ct, read_1),
        (&r2_ga, read_2),
        (&r1_ga, read_1),
        (&r2_ct, read_2),
    ] {
        eprintln!(
            "Created {} converted version of {} -> {} ({} sequences)",
            conv_label(&cr.name),
            src.display(),
            cr.path.display(),
            cr.count
        );
    }

    let combined_basename = config
        .genome
        .combined_index_basename
        .as_ref()
        .ok_or_else(|| {
            AlignerError::Validation(
                "internal error: --combined_index_sequential reached alignment without a combined index"
                    .into(),
            )
        })?;
    let combined_opts = combined_aligner_options(config);

    // Spill path: a sibling of the C→T `-1` converted file (already in the resolved
    // temp_dir — reusing its path sidesteps the empty default `temp_dir`).
    let mut spill_path = r1_ct.path.clone();
    let mut spill_name = spill_path.file_name().unwrap_or_default().to_os_string();
    spill_name.push(".ct_pass.sam");
    spill_path.set_file_name(spill_name);

    // ---- PASS 1 (C→T reads → OT/OB) -----------------------------------------
    // Spawn ONE both-strands PE pass over the C→T mate files, drain it to the spill file,
    // then FINISH it — `finish()` -> `child.wait()` blocks until pass-1 Bowtie 2 EXITS,
    // freeing the combined index. THE RSS INVARIANT: pass 2 must NOT spawn before this
    // returns (the `?` also aborts here on a non-zero pass-1 exit, before any pass-2
    // process exists). The gate's RSS ceiling is the primary guard; the co-residency
    // sampler corroborates.
    let mut ct_stream = PairedAlignerStream::spawn(
        config.aligner,
        bt2,
        &combined_opts,
        Orientation::Both,
        combined_basename,
        &r1_ct.path,
        &r2_ga.path,
    )?;
    let spilled = spill_pe_stream_to_file(&mut ct_stream, &spill_path)?;
    ct_stream.finish()?;
    eprintln!(
        "Sequential combined-index: spilled {spilled} pass-1 (C->T) alignment pairs to {} and freed \
         the combined index before the G->A pass",
        spill_path.display()
    );

    // ---- PASS 2 (G→A reads → CTOT/CTOB) -------------------------------------
    // Now (and only now) spawn the second pass; replay pass 1 from disk against it.
    let mut ga_stream = PairedAlignerStream::spawn(
        config.aligner,
        bt2,
        &combined_opts,
        Orientation::Both,
        combined_basename,
        &r1_ga.path,
        &r2_ct.path,
    )?;
    let mut ct_file_stream = PairedFileSamStream::open(&spill_path)?;
    let dovetail = config.dovetail;
    // The byte-frozen (body-unchanged, Phase-3-widened-generic) model-(a) PE driver, with
    // the C→T stream sourced from disk and the G→A stream live.
    drive_merge_combined_pe_nondir(
        read_1,
        read_2,
        &mut ct_file_stream,
        &mut ga_stream,
        config,
        genome,
        refid,
        dovetail,
        sinks,
        counters,
    )?;
    ga_stream.finish()?;

    // The spill is internal scratch (not a `ConvertedReads`) — clean it up here;
    // `converted` is returned for the caller's cleanup loop.
    let _ = std::fs::remove_file(&spill_path);
    Ok(vec![r1_ct, r2_ga, r1_ga, r2_ct])
}

/// PE combined-index pipeline, **non-directional SINGLE-PASS** (model (b), the
/// non-faithful low-RSS variant). Mirrors [`run_pe_combined_nondir`] but drives ONE
/// both-strands PE pass over the conversion-TAGGED interleaved reads
/// ([`process_pe_chunk_combined_nondir_tagged`]); the report prints the 4-strand split
/// (`directional=false`). NOT byte-identical AND NOT decision-equivalent (the qname tag
/// perturbs Bowtie 2's read-name RNG) → opt-in, never the default, own ground-truth gate.
fn run_pe_combined_nondir_tagged(
    config: &RunConfig,
    mates1: &[String],
    mates2: &[String],
) -> Result<()> {
    let started = Instant::now();
    let opts = convert::ConvertOptions::from_config(config);
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    // Non-directional → the report prints the 4-strand (OT/OB/CTOT/CTOB) split.
    let directional = false;
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    let tok = config.aligner.token();
    let combined_opts = combined_aligner_options(config);

    if let Some(combined_basename) = &config.genome.combined_index_basename {
        eprintln!(
            ">>> Combined-index mode, paired-end NON-DIRECTIONAL SINGLE-PASS (model b; EXPERIMENTAL \
             — NOT byte-identical AND NOT decision-equivalent to the model-(a) two-pass path: the \
             conversion tag perturbs Bowtie 2's read-name RNG; ground-truth-validated, never the \
             default): ONE both-strands {} PE pass over {} (-k 2) of conversion-tagged interleaved \
             read pairs (one index load instead of two) <<<",
            config.aligner.name(),
            combined_basename.display()
        );
    }
    if config.ambig_bam {
        eprintln!(
            "Note: combined-index mode does not populate --ambig_bam records in this phase \
             (ambiguous reads are still written to --ambiguous/--unmapped if requested)."
        );
    }

    for (read_1, read_2) in mates1.iter().zip(mates2) {
        let bam_path =
            derive_output_path(read_1, config, &format!("_bismark_{tok}_pe.bam"), "_pe.bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_pe_sinks(read_1, read_2, config, &header, &bam_path)?;

        let report_path = derive_output_path(
            read_1,
            config,
            &format!("_bismark_{tok}_PE_report.txt"),
            "_PE_report.txt",
        );
        let mut report = BufWriter::new(File::create(&report_path)?);
        report::write_report_header(
            &mut report,
            &ReportHeader {
                sequence_file: read_1,
                sequence_file2: Some(read_2),
                genome_folder: &genome_folder,
                aligner_options: &combined_opts,
                aligner: config.aligner,
                library: config.library,
            },
        )?;
        // Never-silent: mark the report as combined-index model-(b) single-pass.
        writeln!(
            report,
            "Combined-index mode, non-directional SINGLE-PASS (model b; experimental, \
             concordance-gated; NOT byte-identical AND NOT decision-equivalent to the \
             model-(a) two-pass path)"
        )?;

        let mut counters = Counters::default();
        let converted = process_pe_chunk_combined_nondir_tagged(
            config,
            &genome,
            &refid,
            Path::new(read_1),
            Path::new(read_2),
            &opts,
            &mut sinks,
            &mut counters,
        )?;

        report::print_final_analysis_report_paired_ends(&mut report, &counters, directional)?;
        writeln!(
            report,
            "Spurious alignments discarded (combined-index; wrong sub-genome for the orientation):\t{}",
            counters.combined_spurious_count
        )?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;
        sinks.finish()?;

        for cr in &converted {
            let _ = std::fs::remove_file(&cr.path);
        }
        eprintln!("{}", counters_summary_pe(read_1, read_2, &counters));
    }
    Ok(())
}

/// Convert the read pairs to TWO conversion-tagged interleaved temp files (`-1`/`-2`,
/// [`convert::convert_pe_tagged_interleaved`]), spawn ONE both-strands PE instance over
/// the combined index with `-k 2`, and drive the per-pair split→union classify→select→
/// route. Returns the two converted temp files for cleanup. ONE subprocess → ONE combined
/// index resident (model (b)'s −50% RSS vs model (a)'s two co-resident loads). The PE
/// analog of [`process_se_chunk_combined_nondir_tagged`].
#[allow(clippy::too_many_arguments)]
fn process_pe_chunk_combined_nondir_tagged(
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    read_1: &Path,
    read_2: &Path,
    opts: &convert::ConvertOptions,
    sinks: &mut PeSinks,
    counters: &mut Counters,
) -> Result<Vec<convert::ConvertedReads>> {
    let bt2 = &config.detected_aligner.path;
    let fasta = matches!(config.format, ReadFormat::FastA);
    // TWO interleaved tagged converted files (-1/-2): per pair, `<id>__CT` (C→T-reads
    // pass) then `<id>__GA` (G→A-reads pass). count = N base pairs (2N emitted pairs).
    let (tagged1, tagged2) = convert::convert_pe_tagged_interleaved(
        read_1,
        read_2,
        &config.output.temp_dir,
        opts,
        fasta,
    )?;
    eprintln!(
        "Created conversion-tagged interleaved versions of {} / {} -> {} , {} ({} base pairs, 2x tagged pairs)",
        read_1.display(),
        read_2.display(),
        tagged1.path.display(),
        tagged2.path.display(),
        tagged1.count
    );
    let combined_basename = config
        .genome
        .combined_index_basename
        .as_ref()
        .ok_or_else(|| {
            AlignerError::Validation(
                "internal error: --combined_index_single_pass reached alignment without a combined index"
                    .into(),
            )
        })?;
    // `-k 2`; `Orientation::Both` emits no `--norc`/`--nofw`. ONE PE pass over the
    // combined index fed the tagged interleaved mate files.
    let combined_opts = combined_aligner_options(config);
    let mut stream = PairedAlignerStream::spawn(
        config.aligner,
        bt2,
        &combined_opts,
        Orientation::Both,
        combined_basename,
        &tagged1.path,
        &tagged2.path,
    )?;
    let dovetail = config.dovetail;
    drive_merge_combined_pe_nondir_tagged(
        read_1,
        read_2,
        &mut stream,
        config,
        genome,
        refid,
        dovetail,
        sinks,
        counters,
    )?;
    stream.finish()?;
    Ok(vec![tagged1, tagged2])
}

/// Never-silent desync check for the model-(b) single tagged PE stream: the stream's
/// current head pair (if any) MUST belong to `identifier` AFTER its tag is stripped —
/// keyed on the TAG-STRIPPED [`SamPair::seq_id`] (NOT the raw qname; the Phase-3 B-1
/// lesson, and the tag must be stripped BEFORE the compare since the head always carries
/// one). A `None` head is in-sync. `strip_conv_tag` itself dies on an untagged record
/// (guard iii). Extracted from [`drive_merge_combined_pe_nondir_tagged`] so it is
/// unit-testable without a `PeSinks` mock (the Phase-3 `assert_pe_pass_in_sync` approach).
fn assert_pe_tag_in_sync(head: Option<&crate::align::SamPair>, identifier: &str) -> Result<()> {
    if let Some(p) = head {
        let (base, _) = strip_conv_tag(&p.seq_id)?;
        if base != identifier {
            return Err(AlignerError::Validation(format!(
                "Combined non-directional tagged PE desync: stream head base id is '{base}' (from \
                 seq_id '{}') but expected '{identifier}'",
                p.seq_id
            )));
        }
    }
    Ok(())
}

/// Re-read both original read files in lockstep and, per pair, drain the SINGLE tagged
/// stream's contiguous same-base-id run (each base id emits its `__CT` pairs then its
/// `__GA` pairs under the `--reorder`-under-`-p` invariant), partition by the
/// tag-stripped `seq_id` into `ct_pairs` / `ga_pairs`, then hand to the shared
/// [`select_and_route_pe_nondir`]. Generic over [`PairedSamStream`] for unit testing. The
/// PE analog of [`drive_merge_combined_nondir_tagged`] doubled; the read loop is copied
/// from [`drive_merge_combined_pe_nondir`].
///
/// **Never-silent contract** (the single-stream guard is weaker than model (a)'s two
/// per-stream checks — phase-8 A-Crit1/B-C2 analog): (i) the head pair's TAG-STRIPPED
/// `seq_id` must == `identifier` ([`assert_pe_tag_in_sync`]; a raw compare always fires —
/// the head carries a tag); (ii) after draining, BOTH `ct_pairs` AND `ga_pairs` must be
/// non-empty (every base id emits both tags, ≥1 pair each incl. a `(77,141)` miss) — a
/// missing half → die loud; (iii) `strip_conv_tag` dies on an untagged record.
#[allow(clippy::too_many_arguments)]
fn drive_merge_combined_pe_nondir_tagged<S: PairedSamStream>(
    read_1: &Path,
    read_2: &Path,
    stream: &mut S,
    config: &RunConfig,
    genome: &Genome,
    refid: &HashMap<String, usize>,
    dovetail: bool,
    sinks: &mut PeSinks,
    counters: &mut Counters,
) -> Result<()> {
    let mut r1 = open_reader(read_1)?;
    let mut r2 = open_reader(read_2)?;
    let fasta = matches!(config.format, ReadFormat::FastA);
    let (skip, upto, icpc) = (
        config.read_processing.skip,
        config.read_processing.upto,
        config.read_processing.icpc,
    );

    let (mut id1, mut seq1, mut plus1, mut qual1) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut id2, mut seq2, mut plus2, mut qual2) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut count: u64 = 0;
    loop {
        for v in [
            &mut id1, &mut seq1, &mut plus1, &mut qual1, &mut id2, &mut seq2, &mut plus2,
            &mut qual2,
        ] {
            v.clear();
        }
        let n_id1 = r1.read_until(b'\n', &mut id1)?;
        let n_seq1 = r1.read_until(b'\n', &mut seq1)?;
        if !fasta {
            let _ = r1.read_until(b'\n', &mut plus1)?;
            let _ = r1.read_until(b'\n', &mut qual1)?;
        }
        let n_id2 = r2.read_until(b'\n', &mut id2)?;
        let n_seq2 = r2.read_until(b'\n', &mut seq2)?;
        if !fasta {
            let _ = r2.read_until(b'\n', &mut plus2)?;
            let _ = r2.read_until(b'\n', &mut qual2)?;
        }
        let incomplete = if fasta {
            n_id1 == 0 || n_seq1 == 0 || n_id2 == 0 || n_seq2 == 0
        } else {
            n_id1 == 0
                || n_seq1 == 0
                || qual1.is_empty()
                || n_id2 == 0
                || n_seq2 == 0
                || qual2.is_empty()
        };
        if incomplete {
            break;
        }
        count += 1;
        if let Some(s) = skip
            && s > 0
            && count <= s
        {
            continue;
        }
        if let Some(u) = upto
            && u > 0
            && count > u
        {
            break;
        }
        counters.sequences_count += 1;

        let id1_fixed = convert::fix_id(convert::chomp_newline(&id1), icpc);
        let id2_fixed = convert::fix_id(convert::chomp_newline(&id2), icpc);
        let id_prefix: &[u8] = if fasta { b">" } else { b"@" };
        let identifier =
            String::from_utf8_lossy(id1_fixed.strip_prefix(id_prefix).unwrap_or(&id1_fixed))
                .into_owned();
        let id2_stripped =
            String::from_utf8_lossy(id2_fixed.strip_prefix(id_prefix).unwrap_or(&id2_fixed))
                .into_owned();
        let seq1_uc: Vec<u8> = convert::chomp_newline(&seq1).to_ascii_uppercase();
        let seq2_uc: Vec<u8> = convert::chomp_newline(&seq2).to_ascii_uppercase();
        let qual1_bytes: Vec<u8> = if fasta {
            vec![b'I'; seq1_uc.len()]
        } else {
            convert::chomp_newline(&qual1).to_vec()
        };
        let qual2_bytes: Vec<u8> = if fasta {
            vec![b'I'; seq2_uc.len()]
        } else {
            convert::chomp_newline(&qual2).to_vec()
        };
        let s1 = String::from_utf8_lossy(&seq1_uc).into_owned();
        let s2 = String::from_utf8_lossy(&seq2_uc).into_owned();

        // (i) desync guard on the TAG-STRIPPED head seq_id — every pair emits __CT + __GA,
        // so the head's base id MUST be this pair (cf. model (a)'s per-stream guard).
        assert_pe_tag_in_sync(stream.current_pair(), &identifier)?;

        // Drain the contiguous same-base-id run, partitioning by the tag. The tag is
        // stripped off the cloned pair's seq_id (cosmetic — downstream keys on
        // `identifier`, and `select_core_pe` classifies on flag/rname, never seq_id; the
        // read1/read2 qnames are likewise never read in combined mode, so the BAM is
        // tag-free regardless). Borrow ends before each `advance_pair`.
        let mut ct_pairs: Vec<crate::align::SamPair> = Vec::new();
        let mut ga_pairs: Vec<crate::align::SamPair> = Vec::new();
        loop {
            let parsed: Option<(String, ConvTag)> = match stream.current_pair() {
                Some(p) => {
                    let (base, tag) = strip_conv_tag(&p.seq_id)?;
                    if base != identifier {
                        None
                    } else {
                        Some((base.to_string(), tag))
                    }
                }
                None => None,
            };
            let (base, tag) = match parsed {
                Some(p) => p,
                None => break,
            };
            let mut pair = stream.current_pair().unwrap().clone();
            pair.seq_id = base;
            match tag {
                ConvTag::Ct => ct_pairs.push(pair),
                ConvTag::Ga => ga_pairs.push(pair),
            }
            stream.advance_pair()?;
        }

        // (ii) both halves must be present — a missing __CT or __GA half means the
        // tagged stream desynced from the re-read input (never-silent).
        if ct_pairs.is_empty() || ga_pairs.is_empty() {
            return Err(AlignerError::Validation(format!(
                "Combined non-directional tagged PE desync: read pair '{identifier}' is missing its \
                 __CT or __GA half (ct pairs={}, ga pairs={}); the tagged stream did not emit both \
                 conversion records for this read pair",
                ct_pairs.len(),
                ga_pairs.len()
            )));
        }

        select_and_route_pe_nondir(
            &ct_pairs,
            &ga_pairs,
            &identifier,
            &id2_stripped,
            &s1,
            &s2,
            &seq1_uc,
            &seq2_uc,
            &qual1_bytes,
            &qual2_bytes,
            &seq1,
            &plus1,
            &seq2,
            &plus2,
            fasta,
            genome,
            refid,
            dovetail,
            config,
            sinks,
            counters,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- #787 Illumina 5-Base per-record emission (hermetic, no minimap2) ----

    fn five_base_genome(chr: &str, seq: &[u8]) -> Genome {
        let mut chromosomes = std::collections::HashMap::new();
        chromosomes.insert(chr.to_string(), seq.to_vec());
        Genome {
            chromosomes,
            sq_order: vec![chr.to_string()],
        }
    }

    /// A forward (FLAG 0) read with a read `T` at a genomic C (downstream G) is a
    /// METHYLATED CpG under 5-Base polarity → XM `Z` (the inverse of bisulfite, where
    /// a read T at C is unmethylated `z`). Genome "AACGAATTTT": read "TGAA" at POS 3
    /// (window "CGAATT") → i0 T vs C, downstream G → meCpG; i1..3 match/non-C → '.'.
    #[test]
    fn five_base_emit_forward_methylated_cpg() {
        let genome = five_base_genome("chr1", b"AACGAATTTT");
        let refid = build_refid(&genome);
        let rec = SamRecord::parse("r1\t0\tchr1\t3\t60\t4M\t*\t0\t0\tTGAA\tIIII\tAS:i:0").unwrap();
        let mut c = Counters::default();
        let out = five_base_emit_record(
            &rec, "r1", b"TGAA", b"IIII", &genome, &refid, false, 0, &mut c,
        )
        .unwrap()
        .expect("forward mapped read emits a record");
        let inner = out.inner();
        assert_eq!(u16::from(inner.flags()), 0); // forward
        let xm = bismark_io::tags::xm(inner.data()).unwrap();
        assert_eq!(
            xm, b"Z...",
            "5-Base: read T at genomic C (CpG) = methylated Z"
        );
        assert_eq!(c.total_me_cpg, 1);
        assert_eq!(c.total_unme_cpg, 0);
        assert_eq!(c.unique_best_alignment_count, 1);
    }

    /// The same locus but the read keeps the C (read "CGAA") is UNMETHYLATED under
    /// 5-Base → lower-case `z` (the exact inverse of bisulfite's `Z`).
    #[test]
    fn five_base_emit_forward_unmethylated_cpg() {
        let genome = five_base_genome("chr1", b"AACGAATTTT");
        let refid = build_refid(&genome);
        let rec = SamRecord::parse("r1\t0\tchr1\t3\t60\t4M\t*\t0\t0\tCGAA\tIIII\tAS:i:0").unwrap();
        let mut c = Counters::default();
        let out = five_base_emit_record(
            &rec, "r1", b"CGAA", b"IIII", &genome, &refid, false, 0, &mut c,
        )
        .unwrap()
        .unwrap();
        let xm = bismark_io::tags::xm(out.inner().data()).unwrap();
        assert_eq!(xm, b"z...");
        assert_eq!(c.total_unme_cpg, 1);
        assert_eq!(c.total_me_cpg, 0);
    }

    /// `mask_low_quality`: bases below the Phred threshold become `N`; `baseq=0` is a
    /// passthrough.
    #[test]
    fn mask_low_quality_masks_below_threshold() {
        // qual "!" = Phred 0, "I" = Phred 40 (Phred+33). threshold 20 → only the '!' masks.
        assert_eq!(mask_low_quality(b"ACGT", b"!III", 20, 33), b"NCGT");
        assert_eq!(mask_low_quality(b"ACGT", b"!III", 0, 33), b"ACGT"); // off
        assert_eq!(mask_low_quality(b"ACGT", b"IIII", 20, 33), b"ACGT"); // all high-Q
    }

    /// With `--five_base_baseq`, a low-quality read base at a methylated CpG is masked to
    /// a no-call (`.`) instead of `Z` — cutting the sequencing-error noise floor.
    #[test]
    fn five_base_emit_baseq_masks_low_quality_call() {
        let genome = five_base_genome("chr1", b"AACGAATTTT");
        let refid = build_refid(&genome);
        let rec = SamRecord::parse("r1\t0\tchr1\t3\t60\t4M\t*\t0\t0\tTGAA\t!III\tAS:i:0").unwrap();
        let mut c = Counters::default();
        let out = five_base_emit_record(
            &rec, "r1", b"TGAA", b"!III", &genome, &refid, false, 20, &mut c,
        )
        .unwrap()
        .unwrap();
        let xm = bismark_io::tags::xm(out.inner().data()).unwrap();
        assert_eq!(xm, b"....", "low-Q CpG base is a no-call, not Z");
        assert_eq!(c.total_me_cpg, 0);
    }

    /// An unmapped primary (FLAG 4) yields no record and counts as no-alignment.
    #[test]
    fn five_base_emit_unmapped_is_none() {
        let genome = five_base_genome("chr1", b"AACGAATTTT");
        let refid = build_refid(&genome);
        let rec = SamRecord::parse("r1\t4\t*\t0\t0\t*\t*\t0\t0\tTGAA\tIIII").unwrap();
        let mut c = Counters::default();
        let out = five_base_emit_record(
            &rec, "r1", b"TGAA", b"IIII", &genome, &refid, false, 0, &mut c,
        )
        .unwrap();
        assert!(out.is_none());
        assert_eq!(c.no_single_alignment_found, 1);
        assert_eq!(c.unique_best_alignment_count, 0);
    }

    /// A proper FR pair (R1 forward FLAG 99, R2 reverse FLAG 147) → PE index 0 (OT);
    /// R1's window is identical to the SE index-0 case, so a read `T` at a genomic CpG
    /// C is methylated (`Z`) under 5-Base. Output FLAGs are the Bismark PE 99/147, and
    /// the pair counts as one unique-best alignment.
    #[test]
    fn five_base_emit_pe_forward_pair_methylated_and_flags() {
        let genome = five_base_genome("chr1", b"AACGAATTTTAACGAA"); // 16 bp
        let refid = build_refid(&genome);
        let r1 = SamRecord::parse("p\t99\tchr1\t3\t60\t4M\t=\t5\t6\tTGAA\tIIII\tAS:i:0").unwrap();
        let r2 = SamRecord::parse("p\t147\tchr1\t5\t60\t4M\t=\t3\t-6\tGAAT\tIIII\tAS:i:0").unwrap();
        let mut c = Counters::default();
        let (o1, o2) = five_base_emit_pe_record(
            &r1, &r2, "p", b"TGAA", b"IIII", b"GAAT", b"IIII", &genome, &refid, false, true, 0,
            &mut c,
        )
        .unwrap()
        .expect("proper pair emits two records");
        assert_eq!(u16::from(o1.inner().flags()), 99); // R1 OT output FLAG
        assert_eq!(u16::from(o2.inner().flags()), 147); // R2 OT output FLAG
        let xm1 = bismark_io::tags::xm(o1.inner().data()).unwrap();
        assert_eq!(
            xm1, b"Z...",
            "R1 5-Base: read T at genomic CpG C = methylated Z"
        );
        assert_eq!(c.unique_best_alignment_count, 1);
        assert_eq!(c.total_me_cpg, 1);
    }

    /// A non-proper pair (R1 not flagged 0x2, e.g. one mate unmapped) emits nothing and
    /// counts as no-alignment.
    #[test]
    fn five_base_emit_pe_improper_pair_is_none() {
        let genome = five_base_genome("chr1", b"AACGAATTTTAACGAA");
        let refid = build_refid(&genome);
        // R1 mapped but NOT a proper pair (no 0x2); R2 unmapped (0x4).
        let r1 = SamRecord::parse("p\t73\tchr1\t3\t60\t4M\t*\t0\t0\tTGAA\tIIII\tAS:i:0").unwrap();
        let r2 = SamRecord::parse("p\t133\t*\t0\t0\t*\t*\t0\t0\tGAAT\tIIII").unwrap();
        let mut c = Counters::default();
        let out = five_base_emit_pe_record(
            &r1, &r2, "p", b"TGAA", b"IIII", b"GAAT", b"IIII", &genome, &refid, false, true, 0,
            &mut c,
        )
        .unwrap();
        assert!(out.is_none());
        assert_eq!(c.no_single_alignment_found, 1);
        assert_eq!(c.unique_best_alignment_count, 0);
    }

    /// `five_base_next_primary` skips `@` headers + secondary (0x100) / supplementary
    /// (0x800) lines and returns primaries in order.
    #[test]
    fn five_base_next_primary_skips_headers_and_secondary() {
        let sam = "@HD\tVN:1.0\n\
                   @SQ\tSN:chr1\tLN:10\n\
                   r1\t0\tchr1\t3\t60\t4M\t*\t0\t0\tTGAA\tIIII\n\
                   r1\t256\tchr1\t7\t0\t4M\t*\t0\t0\tTGAA\tIIII\n\
                   r2\t16\tchr1\t3\t60\t4M\t*\t0\t0\tTGAA\tIIII\n";
        let mut reader = std::io::BufReader::new(sam.as_bytes());
        let mut line = String::new();
        let a = five_base_next_primary(&mut reader, &mut line)
            .unwrap()
            .unwrap();
        assert_eq!(a.qname, "r1");
        assert_eq!(a.flag, 0);
        let b = five_base_next_primary(&mut reader, &mut line)
            .unwrap()
            .unwrap();
        assert_eq!(b.qname, "r2"); // the 0x100 secondary line was skipped
        assert_eq!(b.flag, 16);
        assert!(
            five_base_next_primary(&mut reader, &mut line)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn strip_conv_tag_ct_ga_and_fail_loud() {
        // happy paths: the base id + which pass.
        assert_eq!(strip_conv_tag("read1__CT").unwrap(), ("read1", ConvTag::Ct));
        assert_eq!(strip_conv_tag("read1__GA").unwrap(), ("read1", ConvTag::Ga));
        // a base id that itself contains a double underscore is fine (strip ONE tag).
        assert_eq!(
            strip_conv_tag("r_a__b__GA").unwrap(),
            ("r_a__b", ConvTag::Ga)
        );
        // fail-loud: an untagged qname is NEVER silently mis-partitioned.
        let err = strip_conv_tag("read1").unwrap_err();
        assert!(format!("{err}").contains("__CT/__GA conversion tag"));
    }

    /// Epic 06152026: the SE in-process rammap selection predicate (V4 + V5). #995 dropped
    /// the `multicore` term (the in-process path is now N-threaded for ANY N — fork-worker
    /// safety is via the `pipeline()` short-circuit + the `aligner == Rammap` conjunct). Runs
    /// on BOTH builds — "selected" is true iff the feature is compiled in (`cfg!` first conjunct).
    #[test]
    fn inprocess_rammap_predicate_truth_table() {
        use crate::config::{Aligner, ReadFormat};
        // rammap + opt-in + FastQ selects in-process IFF the feature is on.
        let selected = cfg!(feature = "rammap-inprocess");
        assert_eq!(
            inprocess_rammap_selected(Aligner::Rammap, true, ReadFormat::FastQ),
            selected
        );
        // NOT opted in → subprocess (the Phase-4 default), on both builds.
        assert!(!inprocess_rammap_selected(
            Aligner::Rammap,
            false,
            ReadFormat::FastQ
        ));
        // Wrong aligner → subprocess even with the opt-in (on both builds).
        assert!(!inprocess_rammap_selected(
            Aligner::Bowtie2,
            true,
            ReadFormat::FastQ
        ));
        assert!(!inprocess_rammap_selected(
            Aligner::Hisat2,
            true,
            ReadFormat::FastQ
        ));
        assert!(!inprocess_rammap_selected(
            Aligner::Minimap2,
            true,
            ReadFormat::FastQ
        ));
        // FastA → subprocess even with the opt-in (the in-process stream is 4-line FastQ-only).
        assert!(!inprocess_rammap_selected(
            Aligner::Rammap,
            true,
            ReadFormat::FastA
        ));
    }

    /// A canned [`SamStream`] over a fixed record list — for `spill_stream_to_file`.
    struct VecStream {
        recs: Vec<crate::align::SamRecord>,
        idx: usize,
    }
    impl crate::align::SamStream for VecStream {
        fn current(&self) -> Option<&crate::align::SamRecord> {
            self.recs.get(self.idx)
        }
        fn advance(&mut self) -> Result<()> {
            self.idx += 1;
            Ok(())
        }
    }

    /// `spill_stream_to_file` writes every record's `raw_line` in stream order, and a
    /// `FileSamStream` replays them byte-identically — INCLUDING a FLAG-4 miss line
    /// (the common ~50%-per-pass case). This is the round-trip the sequential exec
    /// model relies on (pass 1 spilled to disk == pass 1 live).
    #[test]
    fn spill_stream_to_file_round_trips_incl_unmapped() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("ct_pass.sam");
        let lines = [
            "r1\t0\tchr1_CT_converted\t1\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
            "r2\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF", // FLAG-4 miss
            "r3\t16\tchr1_GA_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
        ];
        let recs: Vec<_> = lines
            .iter()
            .map(|l| crate::align::SamRecord::parse(l).unwrap())
            .collect();
        let mut stream = VecStream {
            recs: recs.clone(),
            idx: 0,
        };
        let n = spill_stream_to_file(&mut stream, &p).unwrap();
        assert_eq!(n, 3);

        let mut replay = crate::align::FileSamStream::open(&p).unwrap();
        for orig in &recs {
            assert_eq!(replay.current().unwrap(), orig);
            replay.advance().unwrap();
        }
        assert!(replay.current().is_none());
    }

    /// An empty stream spills a zero-record (empty) file → the replay is immediately
    /// at EOF (the empty-input edge of the sequential path).
    #[test]
    fn spill_stream_to_file_empty_stream() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("ct_pass.sam");
        let mut stream = VecStream {
            recs: Vec::new(),
            idx: 0,
        };
        assert_eq!(spill_stream_to_file(&mut stream, &p).unwrap(), 0);
        assert!(
            crate::align::FileSamStream::open(&p)
                .unwrap()
                .current()
                .is_none()
        );
    }

    /// A canned [`PairedSamStream`] over a fixed pair list — for `spill_pe_stream_to_file`.
    struct VecPairStream {
        pairs: Vec<crate::align::SamPair>,
        idx: usize,
    }
    impl PairedSamStream for VecPairStream {
        fn current_pair(&self) -> Option<&crate::align::SamPair> {
            self.pairs.get(self.idx)
        }
        fn advance_pair(&mut self) -> Result<()> {
            self.idx += 1;
            Ok(())
        }
    }

    /// `spill_pe_stream_to_file` writes both `raw_line`s of every pair (read1 then
    /// read2), and a `PairedFileSamStream` replays the identical pairs — INCLUDING a
    /// (77,141) miss pair. The PE analog of `spill_stream_to_file_round_trips_incl_
    /// unmapped`; the round-trip the sequential PE exec model relies on.
    #[test]
    fn spill_pe_stream_to_file_round_trips_incl_miss() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("ct_pass.sam");
        let mk = |l1: &str, l2: &str| crate::align::SamPair::from_lines(l1, l2).unwrap();
        let pairs = vec![
            mk(
                "a/1\t99\tchr1_CT_converted\t1\t40\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
                "a/2\t147\tchr1_CT_converted\t1\t40\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
            ),
            // a (77,141) miss pair — the common ~50%-per-pass case.
            mk(
                "b/1\t77\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF",
                "b/2\t141\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF",
            ),
        ];
        let mut stream = VecPairStream {
            pairs: pairs.clone(),
            idx: 0,
        };
        let n = spill_pe_stream_to_file(&mut stream, &p).unwrap();
        assert_eq!(n, 2);

        let mut replay = PairedFileSamStream::open(&p).unwrap();
        for orig in &pairs {
            assert_eq!(replay.current_pair().unwrap(), orig);
            replay.advance_pair().unwrap();
        }
        assert!(replay.current_pair().is_none());
    }

    /// The non-directional PE desync guard: in-sync (head `seq_id` == identifier) and a
    /// `None` head both pass; a mismatched head fails loud with the pass label. (Covers
    /// `drive_merge_combined_pe_nondir`'s guard without a `PeSinks` mock — review B-2.)
    #[test]
    fn assert_pe_pass_in_sync_fires_on_mismatch() {
        let pair = crate::align::SamPair::from_lines(
            "rX/1\t99\tchr1_CT_converted\t1\t40\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
            "rX/2\t147\tchr1_CT_converted\t1\t40\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
        )
        .unwrap();
        // in sync (head seq_id == identifier) → Ok; None head (pass past this read) → Ok.
        assert!(assert_pe_pass_in_sync(Some(&pair), "rX", "C->T").is_ok());
        assert!(assert_pe_pass_in_sync(None, "rX", "C->T").is_ok());
        // desync → fail loud, carrying the pass label.
        let err = assert_pe_pass_in_sync(Some(&pair), "rY", "G->A").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("desync"), "got: {msg}");
        assert!(msg.contains("G->A"), "got: {msg}");
    }

    /// The model-(b) PE desync guard: a head whose TAG-STRIPPED `seq_id` matches the
    /// identifier passes; a `None` head passes; a tag-stripped mismatch fails loud; and
    /// an UNTAGGED head fails loud via `strip_conv_tag` (guard iii). The tag must be
    /// stripped BEFORE the compare — the head always carries one.
    #[test]
    fn assert_pe_tag_in_sync_fires_on_mismatch_and_untagged() {
        // tagged head, base id == identifier → Ok.
        let tagged = crate::align::SamPair::from_lines(
            "rX__CT/1\t99\tchr1_CT_converted\t1\t40\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
            "rX__CT/2\t147\tchr1_CT_converted\t1\t40\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
        )
        .unwrap();
        assert_eq!(tagged.seq_id, "rX__CT"); // seq_id carries the tag (from_lines strips only /1)
        assert!(assert_pe_tag_in_sync(Some(&tagged), "rX").is_ok());
        assert!(assert_pe_tag_in_sync(None, "rX").is_ok());
        // tag-stripped mismatch → fail loud.
        let err = assert_pe_tag_in_sync(Some(&tagged), "rY").unwrap_err();
        assert!(format!("{err}").contains("desync"), "got: {err}");
        // untagged head → `strip_conv_tag` fails loud (guard iii).
        let untagged = crate::align::SamPair::from_lines(
            "rX/1\t99\tchr1_CT_converted\t1\t40\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
            "rX/2\t147\tchr1_CT_converted\t1\t40\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
        )
        .unwrap();
        let err = assert_pe_tag_in_sync(Some(&untagged), "rX").unwrap_err();
        assert!(
            format!("{err}").contains("__CT/__GA conversion tag"),
            "got: {err}"
        );
    }

    /// THE load-bearing tag-placement round-trip (rev-1 / reviews A-I1 + B-I1): the
    /// model-(b) PE qname `<base>__CT/1/1` survives the full path. Bowtie 2 strips the
    /// OUTER `/1`,`/2` (here we feed `<base>__CT/1` + `<base>__CT/2`, the post-strip
    /// form); `SamPair::from_lines` pairs by `.strip_suffix("/1")` → `seq_id =
    /// <base>__CT`; `strip_conv_tag(seq_id)` recovers `(<base>, Ct)`. A tag-AFTER-suffix
    /// qname (`<base>/1/1__CT` → post-strip `<base>/1__CT`) would have NO `/1` tail →
    /// `from_lines` would fail to identify read 1.
    #[test]
    fn modelb_pe_tag_before_suffix_round_trips() {
        // The post-Bowtie-strip form of `<base>__CT/1/1` / `<base>__CT/2/2`.
        let pair = crate::align::SamPair::from_lines(
            "base__CT/1\t99\tchr1_CT_converted\t1\t40\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
            "base__CT/2\t147\tchr1_CT_converted\t1\t40\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
        )
        .unwrap();
        assert_eq!(pair.seq_id, "base__CT");
        assert_eq!(strip_conv_tag(&pair.seq_id).unwrap(), ("base", ConvTag::Ct));

        // The GA half likewise round-trips, regardless of which mate Bowtie 2 emits first.
        let ga = crate::align::SamPair::from_lines(
            "base__GA/2\t147\tchr1_CT_converted\t5\t40\t6M\t=\t5\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
            "base__GA/1\t83\tchr1_CT_converted\t5\t40\t6M\t=\t5\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
        )
        .unwrap();
        assert_eq!(ga.seq_id, "base__GA");
        assert_eq!(strip_conv_tag(&ga.seq_id).unwrap(), ("base", ConvTag::Ga));

        // The BROKEN tag-after-suffix shape: `<base>/1/1__CT` post-Bowtie-strip is
        // `<base>/1__CT` — its tail is NOT `/1`, so neither line is identified as read 1
        // → from_lines dies (the failure mode the placement avoids).
        let broken = crate::align::SamPair::from_lines(
            "base/1__CT\t99\tchr1_CT_converted\t1\t40\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
            "base/2__CT\t147\tchr1_CT_converted\t1\t40\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6",
        );
        assert!(broken.is_err(), "tag-after-suffix must fail to pair");
    }
}
