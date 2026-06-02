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
//! FastQ + `--ambig_bam`, Phase 7 paired-end (directional), and Phase 8 the
//! non-directional + pbat library types. **FastQ single-end + paired-end, all
//! library types (directional/non-directional/pbat), run end to end.** FastA input
//! and multicore/threading land in later phases.

pub mod align;
pub mod aligner;
pub mod aux_out;
pub mod cli;
pub mod config;
pub mod convert;
pub mod discovery;
pub mod error;
pub mod genome;
pub mod mapq;
pub mod merge;
pub mod methylation;
pub mod options;
pub mod output;
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

use crate::align::{AlignerStream, Orientation, PairedAlignerStream};
use crate::aux_out::AuxKind;
use crate::config::{LibraryType, ReadFormat, ReadLayout};
use crate::genome::{Genome, read_genome_into_memory};
use crate::merge::{
    Counters, Decision, DecisionPaired, check_results_paired_end, check_results_single_end,
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

/// `--version` banner (uses the crate's own `CARGO_PKG_VERSION`; not byte-gated).
pub fn version_string() -> String {
    format!(
        "\n          Bismark - Bisulfite Mapper and Methylation Caller.\n\n          \
         Bismark Aligner (Rust port) Version: {}\n        \
         Copyright 2010-25, Felix Krueger, Altos Bioinformatics\n\n               \
         https://github.com/FelixKrueger/Bismark\n",
        env!("CARGO_PKG_VERSION")
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
    eprintln!("{}", config.summary());
    pipeline(&config)?;
    Ok(())
}

/// Dispatch the v1 spine (single-end + directional + FastQ) to the full
/// convert→align→merge pipeline; other modes are wired in later phases.
fn pipeline(config: &RunConfig) -> Result<()> {
    match (&config.layout, config.format) {
        // SE FastQ — all library types (directional/non-directional/pbat) fold
        // into the generalized `run_se` (the per-mode instance plan).
        (ReadLayout::SingleEnd { reads }, ReadFormat::FastQ) => run_se(config, reads),
        (ReadLayout::PairedEnd { mates1, mates2 }, ReadFormat::FastQ) => {
            run_pe(config, mates1, mates2)
        }
        _ => {
            eprintln!(
                "(FastA input and multicore/threading are wired in a later phase; \
                 this build handles FastQ single-end/paired-end, all library types)"
            );
            Ok(())
        }
    }
}

/// Which bisulfite index a spawned instance reads (`BS_CT` vs `BS_GA`).
#[derive(Clone, Copy)]
enum IndexChoice {
    Ct,
    Ga,
}

/// Convert the per-mode SE temp file(s) (Perl `biTransformFastQFiles` 5489–5651):
/// directional = `[C→T]`, pbat = `[G→A]`, non-directional = `[C→T, G→A]` (in that
/// order — the [`se_instance_plan`] file indices key off it).
fn convert_se_files(
    config: &RunConfig,
    read_file: &str,
    opts: &convert::ConvertOptions,
) -> Result<Vec<convert::ConvertedReads>> {
    let path = Path::new(read_file);
    let td = &config.output.temp_dir;
    Ok(match config.library {
        LibraryType::Directional => vec![convert::bisulfite_convert_fastq_se(path, td, opts)?],
        LibraryType::Pbat => vec![convert::bisulfite_convert_fastq_se_ga(path, td, opts)?],
        LibraryType::NonDirectional => vec![
            convert::bisulfite_convert_fastq_se(path, td, opts)?, // file 0 = C→T
            convert::bisulfite_convert_fastq_se_ga(path, td, opts)?, // file 1 = G→A
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

/// SE pipeline (all library types): load the genome once, then per read file
/// convert the per-mode temp file(s), spawn the 2 (directional/pbat) or 4
/// (non-directional) Bowtie 2 instances per the [`se_instance_plan`], drive the
/// lockstep merge, write the Bismark BAM + the alignment report, and (when
/// requested) the `--unmapped` / `--ambiguous` FastQ files and the `--ambig_bam`.
fn run_se(config: &RunConfig, reads: &[String]) -> Result<()> {
    let started = Instant::now();
    let opts = convert::ConvertOptions::from_config(config);
    let bt2 = &config.detected_aligner.path;

    // Load the raw genome once (Perl 273–277), consuming Phase 1's ordered
    // FASTA list — the single source of truth for the `@SQ` order.
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    // The header is identical for every read file (Bismark `@PG` reconstructed
    // from argv; samtools `@PG` normalised out per gate policy P1).
    let header = generate_sam_header(&genome, &config.command_line);
    let pbat = matches!(config.library, LibraryType::Pbat);
    let directional = matches!(config.library, LibraryType::Directional);
    // The report's genome path is the absolute path WITH a trailing `/` (Perl
    // forces it, 7619–7629); `genome_dir` is absolute (canonicalize) but slashless.
    let genome_folder = format!("{}/", config.genome.genome_dir.display());

    for read_file in reads {
        // Phase 2/8: convert the per-mode temp file(s) (1 for directional/pbat, 2
        // for non-directional). Both/all instances read from this set.
        let converted = convert_se_files(config, read_file, &opts)?;
        for cr in &converted {
            eprintln!(
                "Created {} converted version of {read_file} -> {} ({} sequences)",
                conv_label(&cr.name),
                cr.path.display(),
                cr.count
            );
        }
        // Phase 3/8: spawn the instances per the per-mode plan, in Bismark slot
        // order so the merge's `enumerate` index == the Perl `@fhs` index.
        let mut streams = Vec::with_capacity(2);
        for (orientation, index_choice, file_idx) in se_instance_plan(config.library) {
            let index_basename = match index_choice {
                IndexChoice::Ct => &config.genome.ct_index_basename,
                IndexChoice::Ga => &config.genome.ga_index_basename,
            };
            streams.push(AlignerStream::spawn(
                bt2,
                &config.aligner_options,
                orientation,
                index_basename,
                &converted[file_idx].path,
            )?);
        }

        // Open the BAM + optional --ambig_bam / --unmapped / --ambiguous sinks.
        let bam_path = derive_output_path(read_file, config, "_bismark_bt2.bam", ".bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_sinks(read_file, config, &header, &bam_path)?;

        // Open + write the alignment report header (Perl 1641–1729).
        let report_path = derive_output_path(
            read_file,
            config,
            "_bismark_bt2_SE_report.txt",
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
                library: config.library,
            },
        )?;

        // Phase 4–6: drive the merge, routing each read to its sink.
        let mut counters = Counters::default();
        drive_merge(
            Path::new(read_file),
            &mut streams,
            config,
            &genome,
            &refid,
            pbat,
            &mut sinks,
            &mut counters,
        )?;
        for s in streams {
            s.finish()?;
        }

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

/// The per-read output sinks for one read file: the Bismark BAM plus the
/// optional `--ambig_bam` and the gzipped `--unmapped`/`--ambiguous` FastQ files.
struct Sinks {
    bam: BamWriter<BufWriter<File>>,
    ambig_bam: Option<BamWriter<BufWriter<File>>>,
    unmapped: Option<GzEncoder<BufWriter<File>>>,
    ambiguous: Option<GzEncoder<BufWriter<File>>>,
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
        let p = derive_output_path(read_file, config, "_bismark_bt2.ambig.bam", ".ambig.bam");
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
        Some(open_gz(AuxKind::Unmapped)?)
    } else {
        None
    };
    let ambiguous = if config.ambiguous {
        Some(open_gz(AuxKind::Ambiguous)?)
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
fn drive_merge(
    read_file: &Path,
    streams: &mut [AlignerStream],
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
            continue;
        }
        if let Some(u) = upto
            && u > 0
            && count > u
        {
            break;
        }
        counters.sequences_count += 1;
        // identifier = fix_id(chomp(header)) with the leading '@' stripped (Perl 2442).
        let fixed = convert::fix_id(convert::chomp_newline(&id), icpc);
        let id_bytes = fixed.strip_prefix(b"@").unwrap_or(&fixed);
        let identifier = String::from_utf8_lossy(id_bytes).into_owned();
        let seq_uc: Vec<u8> = convert::chomp_newline(&seq).to_ascii_uppercase();
        let qual_bytes: Vec<u8> = convert::chomp_newline(&qual).to_vec();
        let sequence = String::from_utf8_lossy(&seq_uc).into_owned();

        let decision = check_results_single_end(
            &identifier,
            &sequence,
            streams,
            directional,
            config.score_min_intercept,
            config.score_min_slope,
            config.ambig_bam,
            counters,
        )?;

        // Route each read to its sink (Perl 2451–2465 + the per-outcome return codes).
        match decision {
            // Unique best → genomic-seq + XM call + BAM record (Phase 5).
            Decision::UniqueBest(best) => {
                let ext = extract_corresponding_genomic_sequence_single_end(
                    &best, genome, pbat, counters,
                )?;
                // Length guard (Perl 3127): the window must be read_len + 2; a
                // shorter one means a chromosome-edge guard fired → skip (not written).
                if ext.unmodified_genomic_sequence.len() != seq_uc.len() + 2 {
                    eprintln!(
                        "Chromosomal sequence could not be extracted for\t{identifier}\t{}\t{}",
                        best.chromosome, best.position
                    );
                    counters.genomic_sequence_could_not_be_extracted_count += 1;
                    continue;
                }
                let methcall = methylation_call(
                    &seq_uc,
                    &ext.unmodified_genomic_sequence,
                    ext.read_conversion,
                    counters,
                );
                let record = single_end_sam_output(
                    &identifier,
                    &seq_uc,
                    &qual_bytes,
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
                    let seq_orig = convert::chomp_newline(&seq).to_vec();
                    aux_out::write_fastq_record(
                        w,
                        identifier.as_bytes(),
                        &seq_orig,
                        &plus,
                        &qual_bytes,
                    )?;
                }
            }
            // No alignment → --unmapped FastQ (Perl 2995–2999).
            Decision::NoAlignment => {
                if let Some(w) = sinks.unmapped.as_mut() {
                    let seq_orig = convert::chomp_newline(&seq).to_vec();
                    aux_out::write_fastq_record(
                        w,
                        identifier.as_bytes(),
                        &seq_orig,
                        &plus,
                        &qual_bytes,
                    )?;
                }
            }
            // Directional wrong-strand rejection: counted only, written nowhere (Perl 3116).
            Decision::Rejected => {}
        }
    }
    Ok(())
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

/// PE pipeline (all library types) (Perl `start_methylation_call_procedure_paired_ends`,
/// 1746–1962): load the genome once, then per mate-pair convert the per-mode temp
/// files (each distinct `(mate, kind)` once — directional/pbat = 2 files, non-dir
/// = 4), spawn the **2** (directional/pbat) or **4** (non-dir) paired Bowtie 2
/// instances per the [`pe_instance_plan`], drive the PE lockstep merge, write the
/// `_pe.bam` + `_PE_report.txt` + the `_1`/`_2` aux files.
fn run_pe(config: &RunConfig, mates1: &[String], mates2: &[String]) -> Result<()> {
    let started = Instant::now();
    let opts = convert::ConvertOptions::from_config(config);
    let bt2 = &config.detected_aligner.path;
    let genome = read_genome_into_memory(&config.genome.fastas)?;
    let refid = build_refid(&genome);
    let header = generate_sam_header(&genome, &config.command_line);
    let directional = matches!(config.library, LibraryType::Directional);
    let genome_folder = format!("{}/", config.genome.genome_dir.display());
    // Perl's `$dovetail` (8047–8048): set unless `--no_dovetail`. `options.rs` adds
    // the `--dovetail` token to aligner_options for paired && !no_dovetail, so its
    // presence is exactly `$dovetail` — used by the PE TLEN dovetail sub-cases.
    let dovetail = config
        .aligner_options
        .split_whitespace()
        .any(|t| t == "--dovetail");

    for (read_1, read_2) in mates1.iter().zip(mates2) {
        let plan = pe_instance_plan(config.library);

        // Convert each distinct (mate, kind) the plan needs EXACTLY ONCE — Perl
        // makes 2 files for directional/pbat (shared by both instances) and 4 for
        // non-dir (each pair shared by two slots). Preserve first-seen order.
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
            let cr = convert::bisulfite_convert_fastq_pe_kind(
                Path::new(input),
                &config.output.temp_dir,
                &opts,
                mate,
                kind,
            )?;
            eprintln!(
                "Created {} converted version of {input} -> {} ({} sequences)",
                conv_label(&cr.name),
                cr.path.display(),
                cr.count
            );
            converted.push(((mate, kind), cr));
        }

        // Slot-indexed (0..4): populate the per-mode slots, leaving the rest `None`
        // (the merge scans 0,3,1,2). Each instance reads its `-1`/`-2` converted
        // files (Perl 405–451, 6474).
        let mut streams: Vec<Option<PairedAlignerStream>> = vec![None, None, None, None];
        for (slot, orientation, index_choice, k1, k2) in plan {
            let index_basename = match index_choice {
                IndexChoice::Ct => &config.genome.ct_index_basename,
                IndexChoice::Ga => &config.genome.ga_index_basename,
            };
            let m1 = pe_lookup(&converted, 1, k1);
            let m2 = pe_lookup(&converted, 2, k2);
            streams[slot] = Some(PairedAlignerStream::spawn(
                bt2,
                &config.aligner_options,
                orientation,
                index_basename,
                m1,
                m2,
            )?);
        }

        let bam_path = derive_output_path(read_1, config, "_bismark_bt2_pe.bam", "_pe.bam");
        eprintln!(
            ">>> Writing bisulfite mapping results to {} <<<",
            bam_path.display()
        );
        let mut sinks = open_pe_sinks(read_1, read_2, config, &header, &bam_path)?;

        let report_path = derive_output_path(
            read_1,
            config,
            "_bismark_bt2_PE_report.txt",
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
                library: config.library,
            },
        )?;

        let mut counters = Counters::default();
        drive_merge_pe(
            Path::new(read_1),
            Path::new(read_2),
            &mut streams,
            config,
            &genome,
            &refid,
            dovetail,
            &mut sinks,
            &mut counters,
        )?;
        for s in streams.into_iter().flatten() {
            s.finish()?;
        }

        report::print_final_analysis_report_paired_ends(&mut report, &counters, directional)?;
        report::write_completion_line(&mut report, started.elapsed().as_secs())?;
        report.flush()?;
        sinks.finish()?;

        // Per-mode temp cleanup (rev1 A; Perl 2155): delete EVERY converted temp
        // file — 2 for directional (C→T_1, G→A_2) / pbat (G→A_1, C→T_2), 4 for
        // non-directional. Byte-invisible, so no gate catches an omission.
        // Best-effort.
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
    unmapped_1: Option<GzEncoder<BufWriter<File>>>,
    unmapped_2: Option<GzEncoder<BufWriter<File>>>,
    ambiguous_1: Option<GzEncoder<BufWriter<File>>>,
    ambiguous_2: Option<GzEncoder<BufWriter<File>>>,
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
        let p = derive_output_path(read_1, config, "_bismark_bt2_pe.ambig.bam", "_pe.ambig.bam");
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
            Some(open_gz(aux_out::aux_filename(
                &b1,
                prefix,
                base,
                AuxKind::Unmapped,
                fasta,
                Some(1),
            ))?),
            Some(open_gz(aux_out::aux_filename(
                &b2,
                prefix,
                base,
                AuxKind::Unmapped,
                fasta,
                Some(2),
            ))?),
        )
    } else {
        (None, None)
    };
    let (ambiguous_1, ambiguous_2) = if config.ambiguous {
        (
            Some(open_gz(aux_out::aux_filename(
                &b1,
                prefix,
                base,
                AuxKind::Ambiguous,
                fasta,
                Some(1),
            ))?),
            Some(open_gz(aux_out::aux_filename(
                &b2,
                prefix,
                base,
                AuxKind::Ambiguous,
                fasta,
                Some(2),
            ))?),
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
        let _ = r1.read_until(b'\n', &mut plus1)?;
        let n_qual1 = r1.read_until(b'\n', &mut qual1)?;
        let n_id2 = r2.read_until(b'\n', &mut id2)?;
        let n_seq2 = r2.read_until(b'\n', &mut seq2)?;
        let _ = r2.read_until(b'\n', &mut plus2)?;
        let n_qual2 = r2.read_until(b'\n', &mut qual2)?;
        // Perl 2611 guard: the 6 needed lines (the two `+` lines are NOT guarded).
        if n_id1 == 0 || n_seq1 == 0 || n_qual1 == 0 || n_id2 == 0 || n_seq2 == 0 || n_qual2 == 0 {
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
        let identifier =
            String::from_utf8_lossy(id1_fixed.strip_prefix(b"@").unwrap_or(&id1_fixed))
                .into_owned();
        let id2_stripped =
            String::from_utf8_lossy(id2_fixed.strip_prefix(b"@").unwrap_or(&id2_fixed))
                .into_owned();
        let seq1_uc: Vec<u8> = convert::chomp_newline(&seq1).to_ascii_uppercase();
        let seq2_uc: Vec<u8> = convert::chomp_newline(&seq2).to_ascii_uppercase();
        let qual1_bytes: Vec<u8> = convert::chomp_newline(&qual1).to_vec();
        let qual2_bytes: Vec<u8> = convert::chomp_newline(&qual2).to_vec();
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
            config.ambig_bam,
            counters,
        )?;

        match decision {
            DecisionPaired::UniqueBest(best) => {
                let ext =
                    extract_corresponding_genomic_sequence_paired_end(&best, genome, counters)?;
                // R1 length guard first; on failure return (continue) BEFORE checking R2
                // (Perl 3864→3867), each bumping the count by exactly 1.
                if ext.unmodified_genomic_sequence_1.len() != seq1_uc.len() + 2 {
                    eprintln!(
                        "Chromosomal sequence could not be extracted for\t{identifier}\t{}\t{}",
                        best.chromosome, best.position_1
                    );
                    counters.genomic_sequence_could_not_be_extracted_count += 1;
                    continue;
                }
                if ext.unmodified_genomic_sequence_2.len() != seq2_uc.len() + 2 {
                    eprintln!(
                        "Chromosomal sequence could not be extracted for\t{identifier}\t{}\t{}",
                        best.chromosome, best.position_2
                    );
                    counters.genomic_sequence_could_not_be_extracted_count += 1;
                    continue;
                }
                let mc1 = methylation_call(
                    &seq1_uc,
                    &ext.unmodified_genomic_sequence_1,
                    ext.read_conversion_1,
                    counters,
                );
                let mc2 = methylation_call(
                    &seq2_uc,
                    &ext.unmodified_genomic_sequence_2,
                    ext.read_conversion_2,
                    counters,
                );
                let (rec1, rec2) = paired_end_sam_output(
                    &identifier,
                    &seq1_uc,
                    &seq2_uc,
                    &qual1_bytes,
                    &qual2_bytes,
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
                    &identifier,
                    &id2_stripped,
                    &seq1,
                    &plus1,
                    &qual1_bytes,
                    &seq2,
                    &plus2,
                    &qual2_bytes,
                )?;
            }
            DecisionPaired::NoAlignment => {
                write_pe_aux(
                    sinks.unmapped_1.as_mut(),
                    sinks.unmapped_2.as_mut(),
                    &identifier,
                    &id2_stripped,
                    &seq1,
                    &plus1,
                    &qual1_bytes,
                    &seq2,
                    &plus2,
                    &qual2_bytes,
                )?;
            }
            DecisionPaired::Rejected => {}
        }
    }
    Ok(())
}

/// Write a pair's two FastQ records to the routed `_1`/`_2` aux files (Perl
/// 2649–2674). Each record = `@<id>\n<orig non-uc seq>\n<verbatim + line><qual>\n`.
#[allow(clippy::too_many_arguments)]
fn write_pe_aux(
    route1: Option<&mut GzEncoder<BufWriter<File>>>,
    route2: Option<&mut GzEncoder<BufWriter<File>>>,
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
        aux_out::write_fastq_record(w, id1.as_bytes(), &s, plus1, qual1)?;
    }
    if let Some(w) = route2 {
        let s = convert::chomp_newline(seq2).to_vec();
        aux_out::write_fastq_record(w, id2.as_bytes(), &s, plus2, qual2)?;
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
