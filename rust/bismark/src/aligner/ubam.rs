//! #1025 Phase 1 — unaligned BAM (uBAM) read input.
//!
//! Bismark has no native uBAM input (neither did Perl v0.25.1). This module
//! transcodes a uBAM into a temporary FASTQ that byte-matches what
//! `samtools fastq` would emit, so the **existing, byte-frozen** bisulfite-convert
//! → align → merge pipeline consumes it unchanged. The contract is therefore
//! "a uBAM run is identical to the equivalent FASTQ run" (the convert path's
//! Perl byte-identity is inherited transitively).
//!
//! ## Why a temp FASTQ (not a streaming reader)
//! The original (unconverted) reads are read **twice**: once by `convert.rs`
//! (which writes the C→T/G→A temp FASTQ for the aligner) and again by the
//! methylation-call/merge loop (`drive_merge*`), which re-reads the input to
//! recover SEQ/QUAL/QNAME (see `convert.rs` module docs). A materialized temp
//! FASTQ is replayable by both consumers and keeps the convert core untouched.
//!
//! ## Reader choice (load-bearing)
//! Reads via **raw `noodles_bam::io::Reader::record_bufs`** — NOT
//! `bismark-io::BamReader`, which routes through `filter_unmapped_then_classify`
//! and would **silently drop every unmapped (FLAG 0x4) record — i.e. the entire
//! uBAM — and requires XR/XG/XM tags** uBAMs lack (mirrors `bismark-bam2nuc`).
//!
//! ## `samtools fastq` parity (samtools 1.21, pinned by plan review)
//! - skip secondary (0x100) + supplementary (0x800) records (`-F 0x900` default);
//! - QUAL = raw phred `+33`; **missing QUAL (empty) → ASCII `B` (0x42) × seq_len**;
//! - FLAG 0x10 → reverse-complement SEQ (full IUPAC) + reverse QUAL;
//! - bare `+` separator line; QNAME emitted verbatim (no comment/aux).

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use noodles_sam::alignment::RecordBuf;

use crate::aligner::error::{AlignerError, Result};

// SAM FLAG bits we consult.
const FLAG_PAIRED: u16 = 0x1;
const FLAG_REVERSE: u16 = 0x10;
const FLAG_READ1: u16 = 0x40;
const FLAG_READ2: u16 = 0x80;
const FLAG_SECONDARY: u16 = 0x100;
const FLAG_SUPPLEMENTARY: u16 = 0x800;

/// `samtools fastq` missing-quality placeholder (ASCII `B`).
const MISSING_QUAL: u8 = b'B';

/// Is `path` a BAM file? Only an authentic `Ok(Bam)` magic-byte sniff counts as
/// uBAM input. A plain FASTQ sniffs as `Ok(Sam)` (first byte `@`) and a gzipped
/// FASTQ as `Err(UnrecognizedBgzfPayload)` — BOTH must return `false` so the
/// normal FASTQ/FASTA path is never broken (plan-review R4). All other
/// `Ok(_)`/`Err(_)` outcomes are likewise treated as "not a uBAM".
pub fn is_bam_input(path: &Path) -> bool {
    matches!(
        crate::io::AlignmentKind::from_path(path),
        Ok(crate::io::AlignmentKind::Bam)
    )
}

/// Peek a uBAM to classify single- vs paired-end: `true` iff the first **primary**
/// record (secondary/supplementary skipped) carries the paired flag (`0x1`).
/// A header-only / no-primary BAM returns `false` (→ the single-end path, which
/// produces an empty FASTQ → the existing graceful-empty handling).
pub fn is_paired(bam: &Path) -> Result<bool> {
    let mut reader = noodles_bam::io::Reader::new(BufReader::new(File::open(bam)?));
    let header = reader.read_header()?;
    for result in reader.record_bufs(&header) {
        let rec = result?;
        let flags = u16::from(rec.flags());
        if flags & (FLAG_SECONDARY | FLAG_SUPPLEMENTARY) != 0 {
            continue;
        }
        return Ok(flags & FLAG_PAIRED != 0);
    }
    Ok(false)
}

/// Full-IUPAC complement, matching `samtools fastq`'s reverse-complement table.
/// Unknown bytes pass through unchanged.
fn complement(b: u8) -> u8 {
    // noodles decodes BAM's 4-bit SEQ to the uppercase IUPAC alphabet
    // (`=ACMGRSVTWYHKDBN` — no `U`, no lowercase), so this table covers every
    // reachable base and matches `samtools fastq`'s complement; any other byte
    // passes through unchanged (total, but unreachable for real BAM SEQ).
    match b {
        b'A' => b'T',
        b'T' => b'A',
        b'G' => b'C',
        b'C' => b'G',
        b'Y' => b'R',
        b'R' => b'Y',
        b'S' => b'S',
        b'W' => b'W',
        b'K' => b'M',
        b'M' => b'K',
        b'B' => b'V',
        b'V' => b'B',
        b'D' => b'H',
        b'H' => b'D',
        b'N' => b'N',
        other => other,
    }
}

/// Append one record's 4 FASTQ lines (`@name\nSEQ\n+\nQUAL\n`) to `out`,
/// matching `samtools fastq`. Returns `Ok(false)` if the record is skipped
/// (secondary/supplementary), `Ok(true)` if a record was written.
///
/// # Errors
/// [`AlignerError::Validation`] if the record has no QNAME.
fn record_to_fastq_lines(rec: &RecordBuf, out: &mut Vec<u8>) -> Result<bool> {
    let flags = u16::from(rec.flags());
    if flags & (FLAG_SECONDARY | FLAG_SUPPLEMENTARY) != 0 {
        return Ok(false);
    }

    let name: &[u8] = match rec.name() {
        Some(n) => n.as_ref(),
        None => {
            return Err(AlignerError::Validation(
                "uBAM record has no QNAME (read name); cannot transcode to FASTQ".into(),
            ));
        }
    };

    let seq = rec.sequence().as_ref(); // decoded ASCII bases
    let quals = rec.quality_scores().as_ref(); // raw phred (0-based), empty if absent
    let reverse = flags & FLAG_REVERSE != 0;

    // SEQ (reverse-complement under 0x10).
    let seq_out: Vec<u8> = if reverse {
        seq.iter().rev().map(|&b| complement(b)).collect()
    } else {
        seq.to_vec()
    };

    // QUAL: +33, or synthesize `B` × seq_len when absent; reverse under 0x10.
    let qual_out: Vec<u8> = if quals.is_empty() {
        vec![MISSING_QUAL; seq.len()]
    } else if reverse {
        quals.iter().rev().map(|&q| q.wrapping_add(33)).collect()
    } else {
        quals.iter().map(|&q| q.wrapping_add(33)).collect()
    };

    out.push(b'@');
    out.extend_from_slice(name);
    out.push(b'\n');
    out.extend_from_slice(&seq_out);
    out.push(b'\n');
    out.extend_from_slice(b"+\n");
    out.extend_from_slice(&qual_out);
    out.push(b'\n');
    Ok(true)
}

/// Transcode a single-end uBAM into a temp FASTQ. Returns the temp path.
///
/// Reads via raw `noodles_bam::io::Reader` (no tag/unmapped filtering).
pub fn transcode_ubam_to_fastq_se(bam: &Path, temp_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(temp_dir)?;
    // Name the temp `<stem>.fastq` so the downstream output stem
    // (`strip_fastq_suffix(basename)`) equals what the equivalent
    // `samtools fastq > <stem>.fastq` run would produce (plan-review R3).
    let out_path = temp_dir.join(format!("{}.fastq", file_stem(bam)));
    let mut reader = noodles_bam::io::Reader::new(BufReader::new(File::open(bam)?));
    let header = reader.read_header()?;
    let mut w = BufWriter::new(File::create(&out_path)?);
    let mut buf = Vec::new();
    for result in reader.record_bufs(&header) {
        let rec = result?;
        buf.clear();
        if record_to_fastq_lines(&rec, &mut buf)? {
            w.write_all(&buf)?;
        }
    }
    w.flush()?;
    Ok(out_path)
}

/// Transcode a paired-end uBAM into (R1, R2) temp FASTQs. Requires the uBAM to be
/// **name-collated** (mates adjacent, as `samtools fastq` requires); fails loud on
/// a desync — an odd number of primary records, a pair whose QNAMEs differ, or a
/// pair that is not exactly one READ1 (0x40) + one READ2 (0x80) (plan-review R7).
pub fn transcode_ubam_to_fastq_pe(bam: &Path, temp_dir: &Path) -> Result<(PathBuf, PathBuf)> {
    std::fs::create_dir_all(temp_dir)?;
    let stem = file_stem(bam);
    let p1 = temp_dir.join(format!("{stem}_1.fastq"));
    let p2 = temp_dir.join(format!("{stem}_2.fastq"));
    let mut reader = noodles_bam::io::Reader::new(BufReader::new(File::open(bam)?));
    let header = reader.read_header()?;
    let mut w1 = BufWriter::new(File::create(&p1)?);
    let mut w2 = BufWriter::new(File::create(&p2)?);

    // Pair adjacent primary records (secondary/supplementary are skipped, so the
    // surviving stream is exactly READ1+READ2 per template for a collated uBAM).
    let mut pending: Option<RecordBuf> = None;
    for result in reader.record_bufs(&header) {
        let rec = result?;
        let flags = u16::from(rec.flags());
        if flags & (FLAG_SECONDARY | FLAG_SUPPLEMENTARY) != 0 {
            continue;
        }
        match pending.take() {
            None => pending = Some(rec),
            Some(first) => {
                write_pe_pair(first, rec, &mut w1, &mut w2)?;
            }
        }
    }
    if pending.is_some() {
        return Err(AlignerError::Validation(
            "paired-end uBAM has an odd number of primary records — mates are not \
             name-collated (run `samtools collate` first)"
                .into(),
        ));
    }
    w1.flush()?;
    w2.flush()?;
    Ok((p1, p2))
}

/// Write a mate pair: verify same QNAME + complementary READ1/READ2, route by flag.
fn write_pe_pair<W: Write>(a: RecordBuf, b: RecordBuf, w1: &mut W, w2: &mut W) -> Result<()> {
    let name_bytes = |r: &RecordBuf| -> Vec<u8> {
        match r.name() {
            Some(n) => {
                let s: &[u8] = n.as_ref();
                s.to_vec()
            }
            None => Vec::new(),
        }
    };
    let name_a = name_bytes(&a);
    let name_b = name_bytes(&b);
    if name_a != name_b {
        return Err(AlignerError::Validation(format!(
            "paired-end uBAM mates out of sync: adjacent records '{}' and '{}' have \
             different QNAMEs — the uBAM is not name-collated (run `samtools collate`)",
            String::from_utf8_lossy(&name_a),
            String::from_utf8_lossy(&name_b),
        )));
    }
    let fa = u16::from(a.flags());
    let fb = u16::from(b.flags());
    let (r1, r2) = match (fa & FLAG_READ1 != 0, fa & FLAG_READ2 != 0) {
        (true, false) if fb & FLAG_READ2 != 0 => (&a, &b),
        (false, true) if fb & FLAG_READ1 != 0 => (&b, &a),
        _ => {
            return Err(AlignerError::Validation(format!(
                "paired-end uBAM pair '{}' is not exactly one READ1 (0x40) + one READ2 \
                 (0x80) (flags {fa:#x}, {fb:#x})",
                String::from_utf8_lossy(&name_a),
            )));
        }
    };
    let mut buf = Vec::new();
    if record_to_fastq_lines(r1, &mut buf)? {
        w1.write_all(&buf)?;
    }
    buf.clear();
    if record_to_fastq_lines(r2, &mut buf)? {
        w2.write_all(&buf)?;
    }
    Ok(())
}

/// File stem for naming the temp FASTQ (path basename minus the final extension).
fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("reads")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use noodles_sam::alignment::record_buf::{QualityScores, Sequence};

    fn rec(name: &str, flags: u16, seq: &[u8], quals: Vec<u8>) -> RecordBuf {
        let mut r = RecordBuf::default();
        *r.name_mut() = Some(name.as_bytes().into());
        *r.flags_mut() = flags.into();
        *r.sequence_mut() = Sequence::from(seq.to_vec());
        *r.quality_scores_mut() = QualityScores::from(quals);
        r
    }

    fn lines(r: &RecordBuf) -> (bool, Vec<u8>) {
        let mut out = Vec::new();
        let wrote = record_to_fastq_lines(r, &mut out).unwrap();
        (wrote, out)
    }

    #[test]
    fn forward_record_basic() {
        // qual phred [37,37,37,37] → '+33' = 'F'
        let (wrote, out) = lines(&rec("r1", 0, b"ACGT", vec![37, 37, 37, 37]));
        assert!(wrote);
        assert_eq!(out, b"@r1\nACGT\n+\nFFFF\n");
    }

    #[test]
    fn reverse_record_revcomps_seq_and_reverses_qual() {
        // FLAG 0x10: RC of ACGT = ACGT (palindrome-ish? A->T,C->G,G->C,T->A rev) →
        // complement(ACGT)=TGCA, reversed = ACGT; use a NON-palindrome to be safe.
        // seq AACG → complement = TTGC → reversed = CGTT.
        // qual [2,4,6,8] → reversed [8,6,4,2] → '+33' = ')'',''#
        let (wrote, out) = lines(&rec("r2", FLAG_REVERSE, b"AACG", vec![2, 4, 6, 8]));
        assert!(wrote);
        let q: Vec<u8> = [8u8, 6, 4, 2].iter().map(|&x| x + 33).collect();
        let mut expected = b"@r2\nCGTT\n+\n".to_vec();
        expected.extend_from_slice(&q);
        expected.push(b'\n');
        assert_eq!(out, expected);
    }

    #[test]
    fn missing_quality_synthesizes_capital_b() {
        // empty quals → 'B' × seq_len (samtools 1.21 parity)
        let (wrote, out) = lines(&rec("r3", 0, b"ACGTN", vec![]));
        assert!(wrote);
        assert_eq!(out, b"@r3\nACGTN\n+\nBBBBB\n");
    }

    #[test]
    fn n_and_iupac_pass_through_forward() {
        let (_w, out) = lines(&rec(
            "r4",
            0,
            b"ACGTNRYK",
            vec![10, 10, 10, 10, 10, 10, 10, 10],
        ));
        assert!(out.starts_with(b"@r4\nACGTNRYK\n+\n"));
    }

    #[test]
    fn iupac_complement_under_reverse() {
        // seq = ACGTRYKMSWBVDHN ; complement then reverse.
        let seq = b"ACGTN";
        // complement: A->T C->G G->C T->A N->N = TGCAN ; reversed = NACGT
        let (_w, out) = lines(&rec("r5", FLAG_REVERSE, seq, vec![20; 5]));
        assert!(out.starts_with(b"@r5\nNACGT\n+\n"));
    }

    #[test]
    fn secondary_and_supplementary_skipped() {
        let (w_sec, out_sec) = lines(&rec("s", FLAG_SECONDARY, b"ACGT", vec![10; 4]));
        assert!(!w_sec);
        assert!(out_sec.is_empty());
        let (w_sup, _) = lines(&rec("s", FLAG_SUPPLEMENTARY, b"ACGT", vec![10; 4]));
        assert!(!w_sup);
    }

    #[test]
    fn missing_name_errors() {
        let mut r = RecordBuf::default();
        *r.sequence_mut() = Sequence::from(b"ACGT".to_vec());
        *r.quality_scores_mut() = QualityScores::from(vec![10u8; 4]);
        let mut out = Vec::new();
        assert!(record_to_fastq_lines(&r, &mut out).is_err());
    }

    #[test]
    fn complement_is_full_iupac() {
        for (a, b) in [
            (b'A', b'T'),
            (b'C', b'G'),
            (b'G', b'C'),
            (b'T', b'A'),
            (b'R', b'Y'),
            (b'Y', b'R'),
            (b'S', b'S'),
            (b'W', b'W'),
            (b'K', b'M'),
            (b'M', b'K'),
            (b'B', b'V'),
            (b'V', b'B'),
            (b'D', b'H'),
            (b'H', b'D'),
            (b'N', b'N'),
        ] {
            assert_eq!(complement(a), b, "complement({})", a as char);
        }
    }
}
