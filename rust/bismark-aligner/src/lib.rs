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
//! N-way lockstep merge + scoring + MAPQ. The pipeline runs end to end and emits
//! a per-read-file **merge counters summary**; the genomic-sequence + `XM` call +
//! BAM output land in Phase 5.

pub mod align;
pub mod aligner;
pub mod cli;
pub mod config;
pub mod convert;
pub mod discovery;
pub mod error;
pub mod mapq;
pub mod merge;
pub mod options;

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use flate2::read::MultiGzDecoder;

use crate::align::{AlignerStream, Orientation};
use crate::config::{LibraryType, ReadFormat, ReadLayout};
use crate::merge::{Counters, check_results_single_end};

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

/// SE-directional pipeline: per read file, convert (C→T), spawn the 2 Bowtie 2
/// instances on the converted file, then drive the lockstep merge. Emits a
/// counters summary (no BAM yet — Phase 5).
fn run_se_directional(config: &RunConfig, reads: &[String]) -> Result<()> {
    let opts = convert::ConvertOptions::from_config(config);
    let bt2 = &config.detected_aligner.path;
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
        // Phase 4: drive the merge over the original reads (re-read in lockstep).
        let mut counters = Counters::default();
        drive_merge(Path::new(read_file), &mut streams, config, &mut counters)?;
        for s in streams {
            s.finish()?;
        }
        eprintln!("{}", counters_summary(read_file, &counters));
    }
    Ok(())
}

/// Re-read the original FastQ and run the merge per read, in lockstep with the
/// instances. `skip`/`upto` MUST match Phase 2's conversion so the driver and
/// the streams see the same reads.
fn drive_merge(
    read_file: &Path,
    streams: &mut [AlignerStream],
    config: &RunConfig,
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
        let sequence = String::from_utf8_lossy(convert::chomp_newline(&seq)).to_ascii_uppercase();

        // Phase 5 turns Decision::UniqueBest into the BAM; Phase 4 only tallies.
        let _decision = check_results_single_end(
            &identifier,
            &sequence,
            streams,
            directional,
            config.score_min_intercept,
            config.score_min_slope,
            counters,
        )?;
    }
    Ok(())
}

fn counters_summary(read_file: &str, c: &Counters) -> String {
    format!(
        "Phase 4 merge summary for {read_file} (no BAM yet — Phase 5):\n\
           sequences analysed:       {}\n\
           unique best alignments:   {}\n\
           no alignment found:       {}\n\
           ambiguous (unsuitable):   {}\n\
           directional-rejected:     {}",
        c.sequences_count,
        c.unique_best_alignment_count,
        c.no_single_alignment_found,
        c.unsuitable_sequence_count,
        c.alignments_rejected_count,
    )
}
