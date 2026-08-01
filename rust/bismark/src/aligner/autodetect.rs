//! `--auto_aligner` (v2): pick the backend from the reads themselves.
//!
//! The v2 backend set covers two disjoint lanes and the choice is a property of
//! the DATA, not of the user's memory:
//!
//! - **long reads** (ONT / PacBio) → [`Aligner::Rammap`], the pure-Rust minimap2;
//! - **short reads** (Illumina WGBS / EM-seq) → [`Aligner::BwaMem4`], the pure-Rust
//!   bwa-mem2.
//!
//! So `--auto_aligner` samples the first reads of the input, takes the **median**
//! read length, and compares it against [`DEFAULT_AUTO_LENGTH_THRESHOLD`]
//! (overridable with `--auto_length_threshold`). Median, not mean: one adapter-dimer
//! or one 40 kb read must not move the decision.
//!
//! The rules that keep it honest:
//!
//! - **Never silent.** The caller prints the sampled count, the median, the
//!   threshold and the chosen backend before anything is aligned.
//! - **Never a fallback.** An unreadable / empty / malformed input is an ERROR,
//!   not a quiet default to Bowtie 2 — a wrong backend is a wrong BAM.
//! - **Sampling only.** [`SAMPLE_READS`] records are read from the FIRST input
//!   file; the file is not rewound, re-read, or modified, and the decision never
//!   touches the alignment path beyond selecting the backend.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use flate2::read::MultiGzDecoder;

use crate::aligner::config::{Aligner, ReadFormat};
use crate::aligner::error::{AlignerError, Result};

/// Median read length (bp) at or above which the long-read backend is chosen.
/// 300 bp sits in the empty band between the two lanes: Illumina tops out at
/// 2x250 (and WGBS/EM-seq is usually 50–150), while ONT/PacBio libraries are
/// kilobases. Anything landing near the boundary is an unusual library, which is
/// exactly when the printed decision matters.
pub const DEFAULT_AUTO_LENGTH_THRESHOLD: u32 = 300;

/// Accepted `--auto_length_threshold` range (bp).
pub const AUTO_LENGTH_THRESHOLD_RANGE: std::ops::RangeInclusive<u32> = 50..=100_000;

/// How many reads the sniff reads before deciding. Cheap (a few hundred KB even
/// for long reads) and far more than enough for a median.
pub const SAMPLE_READS: usize = 1000;

/// The outcome of the sniff — everything the never-silent notice prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoDecision {
    /// The backend chosen.
    pub aligner: Aligner,
    /// How many reads were actually sampled (`<= SAMPLE_READS`; a short file
    /// yields fewer).
    pub sampled: usize,
    /// The median read length over the sample.
    pub median_len: u32,
    /// The threshold the median was compared against.
    pub threshold: u32,
}

/// Decide from an already-collected sample. Pure, so the rule is unit-testable
/// without touching the filesystem. `>= threshold` picks the long-read lane —
/// the boundary value itself is "long", matching the flag's documented wording.
pub fn decide(lengths: &mut [u32], threshold: u32) -> Result<AutoDecision> {
    if lengths.is_empty() {
        return Err(AlignerError::Validation(
            "--auto_aligner found no reads to sample in the first input file: the backend \
             cannot be chosen from an empty input. Fix the input, or select a backend \
             explicitly (--bwamem4 / --rammap / --bowtie2 / --hisat2)."
                .into(),
        ));
    }
    lengths.sort_unstable();
    // Lower median for an even count — deterministic, and never interpolates a
    // length that no read actually had.
    let median_len = lengths[(lengths.len() - 1) / 2];
    let aligner = if median_len >= threshold {
        Aligner::Rammap
    } else {
        Aligner::BwaMem4
    };
    Ok(AutoDecision {
        aligner,
        sampled: lengths.len(),
        median_len,
        threshold,
    })
}

/// Read up to [`SAMPLE_READS`] sequence lengths from `path` (plain or `.gz`,
/// FastQ or FastA). Malformed input fails loud — a truncated record must not be
/// silently rounded down to a shorter median.
pub fn sample_read_lengths(path: &Path, format: ReadFormat) -> Result<Vec<u32>> {
    let file = File::open(path).map_err(|e| {
        AlignerError::Validation(format!(
            "--auto_aligner could not open the first input file {} to sample read lengths: {e}",
            path.display()
        ))
    })?;
    let reader: Box<dyn BufRead> = if path.to_string_lossy().ends_with(".gz") {
        Box::new(BufReader::new(MultiGzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };
    match format {
        ReadFormat::FastQ => sample_fastq(reader, path),
        ReadFormat::FastA => sample_fasta(reader),
    }
}

/// FastQ: 4 lines per record, sequence on line 2. A record whose 4 lines are not
/// all present is a truncated file → error.
fn sample_fastq(mut reader: Box<dyn BufRead>, path: &Path) -> Result<Vec<u32>> {
    let mut lengths = Vec::with_capacity(SAMPLE_READS.min(1024));
    let mut line = String::new();
    while lengths.len() < SAMPLE_READS {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break; // clean EOF on a record boundary
        }
        if line.trim_end().is_empty() {
            continue;
        }
        let mut seq = String::new();
        if reader.read_line(&mut seq)? == 0 {
            return Err(AlignerError::Validation(format!(
                "--auto_aligner: {} ends mid-record (a FastQ header with no sequence line); \
                 cannot sample read lengths from a truncated file.",
                path.display()
            )));
        }
        lengths.push(seq.trim_end().len() as u32);
        // Skip `+` and the quality line; a missing quality line is truncation.
        for _ in 0..2 {
            let mut skip = String::new();
            if reader.read_line(&mut skip)? == 0 {
                return Err(AlignerError::Validation(format!(
                    "--auto_aligner: {} ends mid-record (a FastQ record without its quality \
                     line); cannot sample read lengths from a truncated file.",
                    path.display()
                )));
            }
        }
    }
    Ok(lengths)
}

/// FastA: a record's sequence may span several lines, so lengths accumulate until
/// the next `>`.
fn sample_fasta(mut reader: Box<dyn BufRead>) -> Result<Vec<u32>> {
    let mut lengths: Vec<u32> = Vec::new();
    let mut current: Option<u32> = None;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.starts_with('>') {
            if let Some(len) = current.take() {
                lengths.push(len);
                if lengths.len() >= SAMPLE_READS {
                    return Ok(lengths);
                }
            }
            current = Some(0);
        } else if let Some(len) = current.as_mut() {
            *len += trimmed.len() as u32;
        }
    }
    if let Some(len) = current.take() {
        lengths.push(len);
    }
    Ok(lengths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// The rule itself: below the threshold → the short-read backend; at or above
    /// it → the long-read one.
    #[test]
    fn decide_splits_short_and_long_lanes() {
        let short = decide(&mut [100, 150, 150, 151], DEFAULT_AUTO_LENGTH_THRESHOLD).unwrap();
        assert_eq!(short.aligner, Aligner::BwaMem4);
        assert_eq!(short.median_len, 150);
        assert_eq!(short.sampled, 4);

        let long = decide(&mut [8000, 12000, 400], DEFAULT_AUTO_LENGTH_THRESHOLD).unwrap();
        assert_eq!(long.aligner, Aligner::Rammap);
        assert_eq!(long.median_len, 8000);
    }

    /// The boundary is inclusive on the long side (documented wording: "at or above").
    #[test]
    fn threshold_boundary_is_long() {
        assert_eq!(decide(&mut [300], 300).unwrap().aligner, Aligner::Rammap);
        assert_eq!(decide(&mut [299], 300).unwrap().aligner, Aligner::BwaMem4);
    }

    /// One 40 kb read must not drag a 150 bp Illumina library into the long lane —
    /// the reason the statistic is a median, not a mean.
    #[test]
    fn single_outlier_does_not_flip_the_lane() {
        let d = decide(
            &mut [150, 150, 150, 150, 40_000],
            DEFAULT_AUTO_LENGTH_THRESHOLD,
        )
        .unwrap();
        assert_eq!(d.aligner, Aligner::BwaMem4);
        assert_eq!(d.median_len, 150);
    }

    /// Empty input is an ERROR, never a silent default backend.
    #[test]
    fn empty_sample_is_an_error() {
        let err = decide(&mut [], DEFAULT_AUTO_LENGTH_THRESHOLD).unwrap_err();
        assert!(format!("{err}").contains("no reads to sample"));
    }

    #[test]
    fn samples_fastq_lengths() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("r.fastq");
        let mut f = File::create(&p).unwrap();
        for (i, len) in [50usize, 60, 70].iter().enumerate() {
            writeln!(f, "@read{i}").unwrap();
            writeln!(f, "{}", "A".repeat(*len)).unwrap();
            writeln!(f, "+").unwrap();
            writeln!(f, "{}", "I".repeat(*len)).unwrap();
        }
        drop(f);
        let lens = sample_read_lengths(&p, ReadFormat::FastQ).unwrap();
        assert_eq!(lens, vec![50, 60, 70]);
    }

    /// A FastQ header with no sequence line is truncation → fail loud (never a
    /// short read silently pulling the median down).
    #[test]
    fn truncated_fastq_fails_loud() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.fastq");
        std::fs::write(&p, "@read0\n").unwrap();
        let err = sample_read_lengths(&p, ReadFormat::FastQ).unwrap_err();
        assert!(format!("{err}").contains("mid-record"));
    }

    /// Multi-line FastA records accumulate to one length per record.
    #[test]
    fn samples_multiline_fasta_lengths() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("r.fa");
        std::fs::write(&p, ">a\nAAAA\nAA\n>b\nCCC\n").unwrap();
        let lens = sample_read_lengths(&p, ReadFormat::FastA).unwrap();
        assert_eq!(lens, vec![6, 3]);
    }
}
