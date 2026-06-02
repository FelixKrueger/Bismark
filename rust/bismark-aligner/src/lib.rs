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
//! Implemented so far (single-end directional spine): Phase 1 CLI/discovery/
//! detection, Phase 2 read conversion, Phase 3 single-instance stream, Phase 4
//! N-way lockstep merge + scoring + MAPQ, Phase 5 genomic-seq + `XM`/`XR`/`XG`
//! call + BAM output (the first byte-identity gate, passed on oxy), and Phase 6
//! the alignment report + `--unmapped`/`--ambiguous` FastQ + `--ambig_bam`. The
//! SE-directional pipeline runs end to end. PE / non-directional / pbat / FastA /
//! threading land in later phases.

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

use crate::align::{AlignerStream, Orientation};
use crate::aux_out::AuxKind;
use crate::config::{LibraryType, ReadFormat, ReadLayout};
use crate::genome::{Genome, read_genome_into_memory};
use crate::merge::{Counters, Decision, check_results_single_end};
use crate::methylation::{extract_corresponding_genomic_sequence_single_end, methylation_call};
use crate::output::{
    build_refid, generate_sam_header, single_end_sam_output, write_raw_sam_line_to_bam,
    write_record,
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
    match (&config.layout, config.library, config.format) {
        (ReadLayout::SingleEnd { reads }, LibraryType::Directional, ReadFormat::FastQ) => {
            run_se_directional(config, reads)
        }
        _ => {
            eprintln!(
                "(alignment for this mode is wired in a later phase; the v1 spine is \
                 FastQ single-end directional)"
            );
            Ok(())
        }
    }
}

/// SE-directional pipeline: load the genome once, then per read file convert
/// (C→T), spawn the 2 Bowtie 2 instances, drive the lockstep merge, write the
/// Bismark BAM + the alignment report, and (when requested) the `--unmapped` /
/// `--ambiguous` FastQ files and the `--ambig_bam`.
fn run_se_directional(config: &RunConfig, reads: &[String]) -> Result<()> {
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
        // Phase 2: C→T temp file (both instances read it).
        let converted = convert::bisulfite_convert_fastq_se(
            Path::new(read_file),
            &config.output.temp_dir,
            &opts,
        )?;
        eprintln!(
            "Created C->T converted version of {read_file} -> {} ({} sequences)",
            converted.path.display(),
            converted.count
        );
        // Phase 3: 2 instances — CTreadCTgenome (--norc, CT index) + CTreadGAgenome (--nofw, GA index).
        let mut streams = vec![
            AlignerStream::spawn(
                bt2,
                &config.aligner_options,
                Orientation::Norc,
                &config.genome.ct_index_basename,
                &converted.path,
            )?,
            AlignerStream::spawn(
                bt2,
                &config.aligner_options,
                Orientation::Nofw,
                &config.genome.ga_index_basename,
                &converted.path,
            )?,
        ];

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

        // Delete the C→T temp file (best-effort; Perl warns, never dies, 1974–1981).
        let _ = std::fs::remove_file(&converted.path);

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
        let name = aux_out::aux_filename(&filename, prefix, base, kind, fasta);
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
