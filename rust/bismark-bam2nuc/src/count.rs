//! Per-read genomic-sequence counting.
//!
//! For each read with a clean CIGAR (no `I/D/S/N`), bam2nuc takes the GENOMIC
//! sequence at the read's mapped span — `substr(chr, POS-1, len(SEQ))`, NOT the
//! read's own bases (Perl `generate_nucleotide_report` `:109-142`) — reverse-
//! complements it for reverse-strand reads (Perl `calc_single_end` /
//! `calc_paired_end`), and tallies it via [`crate::freqs::process_sequence`].
//!
//! Records are read with **raw `noodles_bam::io::Reader::record_bufs`** — NOT
//! `bismark-io::BamReader`, which requires XR/XG/XM tags + silently drops
//! unmapped reads. bam2nuc is tag-agnostic, and raw `record_bufs` does not
//! filter unmapped / sort-check, so the SE die-on-stray-flag contract stays
//! faithful (an unmapped SE read carries flag 4 → [`correct_se`] errors).

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use noodles_sam::Header;
use noodles_sam::alignment::RecordBuf;
use noodles_sam::alignment::record::cigar::op::Kind;
use noodles_sam::alignment::record_buf::Cigar;

use crate::error::BismarkBam2nucError;
use crate::freqs::{self, NucCounts};
use crate::genome::Genome;

/// Progress tallies for one input file (NOT byte-gated; informational only).
#[derive(Debug, Default, Clone, Copy)]
pub struct ReadStats {
    /// Records seen (mapped; includes skipped).
    pub total: u64,
    /// Records skipped for an `I/D/S/N` CIGAR.
    pub skipped: u64,
}

/// Build a chromosome-name table indexed by `reference_sequence_id`
/// (the 0-based order of the header's `@SQ` reference sequences). Mirrors
/// `bismark-extractor::header::build_chr_name_table`, returning byte names
/// (bam2nuc indexes the genome by `&[u8]`).
///
/// # Errors
/// [`BismarkBam2nucError::NonAsciiChromosomeName`] if any `@SQ SN:` value
/// contains non-ASCII bytes (Bismark downstream tools can't round-trip them).
pub fn build_chr_name_table(header: &Header) -> Result<Vec<Vec<u8>>, BismarkBam2nucError> {
    let mut out = Vec::with_capacity(header.reference_sequences().len());
    for (name, _ref_seq) in header.reference_sequences() {
        let bytes: &[u8] = name.as_ref();
        if !bytes.is_ascii() {
            return Err(BismarkBam2nucError::NonAsciiChromosomeName {
                name: String::from_utf8_lossy(bytes).into_owned(),
            });
        }
        out.push(bytes.to_vec());
    }
    Ok(out)
}

/// True if the CIGAR contains any Insertion/Deletion/SoftClip/RefSkip op —
/// the structured equivalent of Perl `$cigar =~ /[IDSN]/` (`:126`). `H`/`P`/
/// `=`/`X` are NOT skipped (Perl's regex doesn't match them).
fn cigar_has_indel(cigar: &Cigar) -> bool {
    cigar.as_ref().iter().any(|op| {
        matches!(
            op.kind(),
            Kind::Insertion | Kind::Deletion | Kind::SoftClip | Kind::Skip
        )
    })
}

/// Extract the genomic span: `substr(chr, POS-1, len(SEQ))` with Perl's
/// saturation semantics (`:133`). A missing chromosome, a start at/after the
/// chromosome end, or a span overrunning the end all yield a shorter or empty
/// slice — never a panic. (Perl returns `undef`/`""` here; both contribute zero
/// counts in `process_sequence`, so the empty slice is observably identical.)
fn extract_span(genome: &Genome, chr: &[u8], pos1: usize, read_len: usize) -> Vec<u8> {
    // POS is 1-based for mapped reads. `pos1 == 0` means the record has no
    // alignment start (unmapped / malformed) — contribute nothing. We do NOT
    // replicate Perl's `substr($seq, 0-1, len)` here, which indexes from the
    // chromosome's END via the negative offset (returning the last base); such a
    // record (a real chromosome with POS 0) cannot occur in Bismark BAM output,
    // and tallying the chromosome tail for a positionless read would be garbage.
    // (Documented robustness divergence; byte-neutral on real data — code review
    // A1/M-1.)
    if pos1 == 0 {
        return Vec::new();
    }
    let Some(seq) = genome.get(chr) else {
        return Vec::new();
    };
    let n = seq.len();
    let p = pos1 - 1; // pos1 >= 1 here
    let start = p.min(n);
    let end = p.saturating_add(read_len).min(n);
    seq[start..end].to_vec()
}

/// Reverse-complement: Perl `reverse` then `tr/GATC/CTAG/`. `N` and every other
/// (e.g. IUPAC) byte pass through untouched.
fn revcomp(seq: &[u8]) -> Vec<u8> {
    seq.iter()
        .rev()
        .map(|&b| match b {
            b'G' => b'C',
            b'A' => b'T',
            b'T' => b'A',
            b'C' => b'G',
            other => other,
        })
        .collect()
}

/// Single-end strand correction (Perl `calc_single_end` `:237-252`): flag 0 →
/// as-is, flag 16 → reverse-complement, anything else → **reachable error**
/// (Perl `die "failed to detect valid Bismark FLAG tag"`).
fn correct_se(span: Vec<u8>, flag: u16) -> Result<Vec<u8>, BismarkBam2nucError> {
    match flag {
        0 => Ok(span),
        16 => Ok(revcomp(&span)),
        f => Err(BismarkBam2nucError::InvalidSeFlag { flag: f }),
    }
}

/// Paired-end strand correction (Perl `calc_paired_end` `:219-235`): flag 99 or
/// 147 → as-is, **ALL ELSE → reverse-complement**.
///
/// This replicates Perl's latent bug: `elsif ($flag == 83 or 163)` parses as
/// `($flag == 83) or 163`, and the bare `163` is a truthy constant, so the
/// `elsif` is **always true** and the `else{die}` is dead code. PE therefore
/// NEVER errors on an unexpected flag — every non-99/147 flag is revcomp'd.
fn correct_pe(span: Vec<u8>, flag: u16) -> Vec<u8> {
    if flag == 99 || flag == 147 {
        span
    } else {
        revcomp(&span)
    }
}

/// Count the genomic-sequence composition of one BAM file.
///
/// Opens with raw `noodles_bam::io::Reader` (no XR/XG/XM validation, no unmapped
/// filter, no coordinate-sort check), detects SE/PE from the `@PG` header, and
/// tallies each clean-CIGAR read's strand-corrected genomic span.
pub fn count_reads_in_file(
    genome: &Genome,
    path: &Path,
) -> Result<(NucCounts, ReadStats), BismarkBam2nucError> {
    let mut reader = noodles_bam::io::Reader::new(BufReader::new(File::open(path)?));
    let header = reader.read_header()?;
    let chr_names = build_chr_name_table(&header)?;
    let paired = bismark_io::detect_paired_from_header(&header)
        .ok_or(BismarkBam2nucError::SePeUndetermined)?;
    count_records(reader.record_bufs(&header), paired, &chr_names, genome)
}

/// The per-record loop, factored out of [`count_reads_in_file`] so it can be
/// unit-tested with synthetic [`RecordBuf`]s (no BAM I/O).
fn count_records<I>(
    records: I,
    paired: bool,
    chr_names: &[Vec<u8>],
    genome: &Genome,
) -> Result<(NucCounts, ReadStats), BismarkBam2nucError>
where
    I: Iterator<Item = std::io::Result<RecordBuf>>,
{
    let mut counts = NucCounts::default();
    let mut stats = ReadStats::default();

    for result in records {
        let record = result?;
        stats.total += 1;

        // CIGAR I/D/S/N → skip (Perl `:126-130`).
        if cigar_has_indel(record.cigar()) {
            stats.skipped += 1;
            continue;
        }

        // chr name from reference_sequence_id; None / out-of-range → absent chr
        // (empty span). Mirrors Perl `substr($chromosomes{$chr}, ...)` on an
        // undefined `$chromosomes{$chr}`.
        let chr: &[u8] = record
            .reference_sequence_id()
            .and_then(|id| chr_names.get(id))
            .map_or(&b""[..], |v| v.as_slice());
        // 1-based POS; None (unmapped) → 0 so the span is empty AND the flag
        // still flows to the strand correction (keeps SE die-on-stray-flag).
        let pos1 = record.alignment_start().map_or(0, usize::from);
        let read_len = record.sequence().as_ref().len();

        let span = extract_span(genome, chr, pos1, read_len);
        let corrected = if paired {
            correct_pe(span, flag_of(&record))
        } else {
            correct_se(span, flag_of(&record))?
        };
        freqs::process_sequence(&corrected, &mut counts);
    }

    Ok((counts, stats))
}

#[inline]
fn flag_of(record: &RecordBuf) -> u16 {
    u16::from(record.flags())
}

#[cfg(test)]
mod tests {
    use super::*;
    use noodles_core::Position;
    use noodles_sam::alignment::record::Flags;
    use noodles_sam::alignment::record::cigar::Op;
    use noodles_sam::alignment::record_buf::Sequence;

    // ── pure helpers ──

    #[test]
    fn cigar_has_indel_detects_idsn_keeps_others() {
        let c = |ops: &[(Kind, usize)]| {
            Cigar::from(ops.iter().map(|(k, n)| Op::new(*k, *n)).collect::<Vec<_>>())
        };
        assert!(!cigar_has_indel(&c(&[(Kind::Match, 5)])));
        assert!(cigar_has_indel(&c(&[
            (Kind::Match, 5),
            (Kind::Insertion, 2),
            (Kind::Match, 3)
        ])));
        assert!(cigar_has_indel(&c(&[
            (Kind::Match, 5),
            (Kind::Deletion, 2)
        ])));
        assert!(cigar_has_indel(&c(&[
            (Kind::SoftClip, 2),
            (Kind::Match, 6)
        ])));
        assert!(cigar_has_indel(&c(&[
            (Kind::Match, 5),
            (Kind::Skip, 2),
            (Kind::Match, 5)
        ])));
        // H / P / = / X are NOT [IDSN].
        assert!(!cigar_has_indel(&c(&[
            (Kind::Match, 50),
            (Kind::HardClip, 2)
        ])));
        assert!(!cigar_has_indel(&c(&[(Kind::SequenceMatch, 5)])));
        // empty CIGAR (`*`) → no I/D/S/N → keep.
        assert!(!cigar_has_indel(&c(&[])));
    }

    #[test]
    fn extract_span_saturation() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("g.fa"), ">chr1\nACGTACGT\n").unwrap(); // len 8
        let g = Genome::load(t.path()).unwrap();
        assert_eq!(extract_span(&g, b"chr1", 1, 4), b"ACGT"); // normal
        assert_eq!(extract_span(&g, b"chr1", 6, 4), b"CGT"); // runs off end → truncated
        assert_eq!(extract_span(&g, b"chr1", 9, 2), b""); // start exactly at end
        assert_eq!(extract_span(&g, b"chr1", 20, 2), b""); // start STRICTLY past end (Perl undef)
        assert_eq!(extract_span(&g, b"chrZ", 1, 4), b""); // missing chr
        assert_eq!(extract_span(&g, b"chr1", 0, 4), b""); // POS=0 (no alignment_start) → empty, NOT the front
    }

    #[test]
    fn revcomp_maps_gatc_leaves_others() {
        assert_eq!(revcomp(b"ACGTN"), b"NACGT"); // N untouched
        assert_eq!(revcomp(b"GATC"), b"GATC"); // rev=CTAG, tr→GATC
        assert_eq!(revcomp(b"AACG"), b"CGTT"); // asymmetric
        // IUPAC survives: rev(ARGY)=YGRA → tr (G→C, A→T; R,Y untouched) → YCRT
        assert_eq!(revcomp(b"ARGY"), b"YCRT");
    }

    #[test]
    fn correct_se_flag_table() {
        assert_eq!(correct_se(b"ACGT".to_vec(), 0).unwrap(), b"ACGT");
        assert_eq!(correct_se(b"AACG".to_vec(), 16).unwrap(), b"CGTT");
        // Any other SE flag is a reachable error (Perl die).
        assert!(matches!(
            correct_se(b"ACGT".to_vec(), 4).unwrap_err(),
            BismarkBam2nucError::InvalidSeFlag { flag: 4 }
        ));
        assert!(matches!(
            correct_se(b"ACGT".to_vec(), 256).unwrap_err(),
            BismarkBam2nucError::InvalidSeFlag { flag: 256 }
        ));
    }

    #[test]
    fn correct_pe_replicates_or163_bug() {
        // 99/147 → as-is; everything else → revcomp; NEVER errors.
        assert_eq!(correct_pe(b"AACG".to_vec(), 99), b"AACG");
        assert_eq!(correct_pe(b"AACG".to_vec(), 147), b"AACG");
        assert_eq!(correct_pe(b"AACG".to_vec(), 83), b"CGTT"); // revcomp
        assert_eq!(correct_pe(b"AACG".to_vec(), 163), b"CGTT"); // revcomp
        // The bug: a non-canonical flag (0, 256, 1024) also revcomps — no die.
        assert_eq!(correct_pe(b"AACG".to_vec(), 0), b"CGTT");
        assert_eq!(correct_pe(b"AACG".to_vec(), 256), b"CGTT");
    }

    // ── count_records driver (synthetic RecordBufs) ──

    fn rec(
        flag: u16,
        ref_id: Option<usize>,
        pos: usize,
        cigar: &[(Kind, usize)],
        seq: &[u8],
    ) -> RecordBuf {
        let mut r = RecordBuf::default();
        *r.flags_mut() = Flags::from(flag);
        if let Some(id) = ref_id {
            *r.reference_sequence_id_mut() = Some(id);
        }
        if pos > 0 {
            *r.alignment_start_mut() = Some(Position::try_from(pos).unwrap());
        }
        *r.sequence_mut() = Sequence::from(seq.to_vec());
        *r.cigar_mut() = Cigar::from(
            cigar
                .iter()
                .map(|(k, n)| Op::new(*k, *n))
                .collect::<Vec<_>>(),
        );
        r
    }

    fn genome_chr1_acgtacgt() -> (tempfile::TempDir, Genome) {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("g.fa"), ">chr1\nACGTACGT\n").unwrap();
        let g = Genome::load(t.path()).unwrap();
        (t, g)
    }

    #[test]
    fn count_records_se_forward_and_reverse() {
        let (_t, g) = genome_chr1_acgtacgt();
        let chr_names = vec![b"chr1".to_vec()];
        // flag 0 at POS 1, 4bp → genomic "ACGT" as-is.
        // flag 16 at POS 1, 4bp → revcomp("ACGT") = "ACGT" (palindrome-ish: rev=TGCA, tr→ACGT).
        let recs = vec![
            Ok(rec(0, Some(0), 1, &[(Kind::Match, 4)], b"ACGT")),
            Ok(rec(16, Some(0), 1, &[(Kind::Match, 4)], b"ACGT")),
        ];
        let (counts, stats) = count_records(recs.into_iter(), false, &chr_names, &g).unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.skipped, 0);
        // forward "ACGT": A,C,G,T each 1. reverse-of-ACGT: rev=TGCA→tr→ACGT, so
        // also A,C,G,T each 1. Total mono each = 2.
        for b in [b'A', b'C', b'G', b'T'] {
            assert_eq!(counts.mono(b), 2, "mono {}", b as char);
        }
    }

    #[test]
    fn count_records_skips_indel_reads() {
        let (_t, g) = genome_chr1_acgtacgt();
        let chr_names = vec![b"chr1".to_vec()];
        let recs = vec![
            Ok(rec(
                0,
                Some(0),
                1,
                &[(Kind::Match, 2), (Kind::Insertion, 1), (Kind::Match, 1)],
                b"ACGT",
            )),
            Ok(rec(0, Some(0), 1, &[(Kind::Match, 4)], b"ACGT")),
        ];
        let (counts, stats) = count_records(recs.into_iter(), false, &chr_names, &g).unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.skipped, 1);
        // Only the 2nd (clean) read counted → A=1.
        assert_eq!(counts.mono(b'A'), 1);
    }

    #[test]
    fn count_records_pe_canonical_and_revcomp() {
        let (_t, g) = genome_chr1_acgtacgt();
        let chr_names = vec![b"chr1".to_vec()];
        // POS 5, 4bp → genomic substr = "ACGT" (positions 5..8 = ACGT).
        // flag 99 → as-is "ACGT"; flag 83 → revcomp("ACGT")="ACGT".
        let recs = vec![
            Ok(rec(99, Some(0), 5, &[(Kind::Match, 4)], b"ACGT")),
            Ok(rec(83, Some(0), 5, &[(Kind::Match, 4)], b"ACGT")),
        ];
        let (counts, _stats) = count_records(recs.into_iter(), true, &chr_names, &g).unwrap();
        for b in [b'A', b'C', b'G', b'T'] {
            assert_eq!(counts.mono(b), 2, "mono {}", b as char);
        }
    }

    #[test]
    fn count_records_missing_chr_contributes_nothing_pe_revcomp_of_empty() {
        let (_t, g) = genome_chr1_acgtacgt();
        // chr_names[1] is a chromosome the genome does NOT have.
        let chr_names = vec![b"chr1".to_vec(), b"chrMissing".to_vec()];
        // PE flag 83 (revcomp path) on the missing chr → empty span → no counts.
        let recs = vec![Ok(rec(83, Some(1), 1, &[(Kind::Match, 4)], b"ACGT"))];
        let (counts, stats) = count_records(recs.into_iter(), true, &chr_names, &g).unwrap();
        assert_eq!(stats.total, 1);
        for b in [b'A', b'C', b'G', b'T'] {
            assert_eq!(counts.mono(b), 0);
        }
    }

    #[test]
    fn count_records_se_stray_flag_errors() {
        // The raw reader does NOT drop unmapped reads; an SE stray flag (4)
        // reaches correct_se and errors faithfully (Perl die).
        let (_t, g) = genome_chr1_acgtacgt();
        let chr_names = vec![b"chr1".to_vec()];
        let recs = vec![Ok(rec(4, Some(0), 1, &[(Kind::Match, 4)], b"ACGT"))];
        let err = count_records(recs.into_iter(), false, &chr_names, &g).unwrap_err();
        assert!(matches!(
            err,
            BismarkBam2nucError::InvalidSeFlag { flag: 4 }
        ));
    }

    #[test]
    fn count_records_none_alignment_start_contributes_nothing() {
        // Code review A1/M-1: a record with a VALID chr but NO alignment_start
        // (POS=0) must yield an empty span (contribute nothing), NOT the
        // chromosome front. `rec(.., pos=0, ..)` leaves alignment_start unset →
        // count_records maps it to pos1=0. PE flag 83 exercises the revcomp path
        // (revcomp of empty = empty). Unreachable on real Bismark BAMs.
        let (_t, g) = genome_chr1_acgtacgt();
        let chr_names = vec![b"chr1".to_vec()];
        let recs = vec![Ok(rec(83, Some(0), 0, &[(Kind::Match, 4)], b"ACGT"))];
        let (counts, stats) = count_records(recs.into_iter(), true, &chr_names, &g).unwrap();
        assert_eq!(stats.total, 1);
        for b in [b'A', b'C', b'G', b'T'] {
            assert_eq!(counts.mono(b), 0, "POS=0 record must contribute nothing");
        }
    }
}
