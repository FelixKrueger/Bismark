//! Dedup pipeline: orchestrates `bismark-io` reader/writer + the
//! [`crate::dedup::DedupState`] primitives into a working dedup loop.
//!
//! The two public entry points:
//!
//! - [`run_single`] — one input file, one output file, one report.
//! - [`run_multiple`] — N input files treated as one combined stream
//!   (matches Perl `deduplicate_bismark`'s `--multiple` mode at lines
//!   193–201).
//!
//! Both share the same internal machinery (chr-name interning, SE/PE
//! streaming, empty-input detection). `run_single` is the
//! single-file specialisation; `run_multiple` extends to N files with
//! `@SQ` name-set validation across inputs.
//!
//! See `PLAN.md` §4.5 + §6 Phase C for the design contract.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::path::PathBuf;

use bismark_io::AnyWriter;
use bismark_io::BismarkPair;
use bismark_io::BismarkRecord;
use bismark_io::BismarkStrand;
use bismark_io::CigarExt;
use bstr::BString;
use noodles_sam::Header;
use rustc_hash::FxHashMap;

use crate::DedupKey;
use crate::DedupReport;
use crate::DedupState;
use crate::error::BismarkDedupError;

/// Concrete writer type returned by `bismark_io::open_writer`.
type Writer = AnyWriter<BufWriter<File>, File>;

/// Build a chr-name → interned-u32 map from a SAM header's `@SQ` order.
///
/// Interning is by chr-name **string** (not noodles refID), because in
/// multi-file `--multiple` mode different inputs may assign different
/// integer refIDs to the same chromosome name.
fn build_chr_intern(header: &Header) -> FxHashMap<BString, u32> {
    header
        .reference_sequences()
        .keys()
        .enumerate()
        .map(|(i, name)| (name.clone(), i as u32))
        .collect()
}

/// Build a per-file table mapping `reference_sequence_id` (refID, the
/// per-header 0-based index into `@SQ`) to the workspace-interned chr_id.
///
/// For single-file input this table is the identity `[0, 1, 2, ...]`.
/// For multi-file `--multiple` with reordered `@SQ` across inputs, the
/// values reflect the file1-derived intern map.
fn build_refid_table(
    header: &Header,
    intern: &FxHashMap<BString, u32>,
) -> Result<Vec<u32>, BismarkDedupError> {
    header
        .reference_sequences()
        .keys()
        .map(|name| {
            intern
                .get(name)
                .copied()
                .ok_or_else(|| BismarkDedupError::MultipleSqMismatch {
                    offending_file: PathBuf::new(),
                    missing_chrs: vec![format!("{name}")],
                })
        })
        .collect()
}

/// Validate that `header`'s `@SQ` name set equals the reference intern's
/// name set. Returns an error tagged with `offending_file` if they
/// disagree (either side has names the other doesn't).
fn validate_chr_consistency(
    offending_file: &Path,
    header: &Header,
    reference_intern: &FxHashMap<BString, u32>,
) -> Result<(), BismarkDedupError> {
    let this_names: std::collections::HashSet<&BString> =
        header.reference_sequences().keys().collect();
    let intern_names: std::collections::HashSet<&BString> = reference_intern.keys().collect();
    let missing: Vec<String> = intern_names
        .difference(&this_names)
        .map(|n| format!("{n}"))
        .collect();
    let extra: Vec<String> = this_names
        .difference(&intern_names)
        .map(|n| format!("{n}"))
        .collect();
    if missing.is_empty() && extra.is_empty() {
        Ok(())
    } else {
        let mut combined = missing;
        combined.extend(extra.into_iter().map(|n| format!("(extra) {n}")));
        Err(BismarkDedupError::MultipleSqMismatch {
            offending_file: offending_file.to_path_buf(),
            missing_chrs: combined,
        })
    }
}

/// Whether a strand is in the "forward" group (alignment_start is the
/// 5'-most position) vs "reverse" (reference_end is the 5'-most position
/// for dedup-key purposes — see Perl lines 343–388 / 397–445).
fn is_forward(strand: BismarkStrand) -> bool {
    matches!(strand, BismarkStrand::OT | BismarkStrand::CTOB)
}

/// Compute the SE dedup key from a `BismarkRecord`.
fn compute_se_key(
    record: &BismarkRecord,
    refid_table: &[u32],
) -> Result<DedupKey, BismarkDedupError> {
    let inner = record.inner();
    let refid =
        inner
            .reference_sequence_id()
            .ok_or_else(|| BismarkDedupError::MissingAlignmentStart {
                qname: qname_lossy(record),
            })?;
    let chr_id = *refid_table
        .get(refid)
        .ok_or(BismarkDedupError::MissingChrInIntern { refid })?;
    let start = u32::try_from(record.alignment_start().ok_or_else(|| {
        BismarkDedupError::MissingAlignmentStart {
            qname: qname_lossy(record),
        }
    })?)
    .expect("alignment_start fits in u32 per BAM spec");
    let key_pos = if is_forward(record.record_strand()) {
        start
    } else {
        // Reverse strand: end position = start + reference_span - 1
        // via CigarExt::reference_end. Matches Perl lines 349–387.
        u32::try_from(record.cigar().reference_end(start as usize))
            .expect("reference_end fits in u32 per BAM spec")
    };
    Ok(DedupKey::se(record.record_strand(), chr_id, key_pos))
}

/// Compute the PE dedup key from a `BismarkPair`.
fn compute_pe_key(pair: &BismarkPair, refid_table: &[u32]) -> Result<DedupKey, BismarkDedupError> {
    // chr_id comes from R1's refID — both mates must agree on chr_id
    // (enforced by BismarkPair::from_mates' qname-equality check).
    let r1 = pair.r1();
    let r2 = pair.r2();
    let refid = r1.inner().reference_sequence_id().ok_or_else(|| {
        BismarkDedupError::MissingAlignmentStart {
            qname: qname_lossy(r1),
        }
    })?;
    let chr_id = *refid_table
        .get(refid)
        .ok_or(BismarkDedupError::MissingChrInIntern { refid })?;
    let r1_start = u32::try_from(r1.alignment_start().ok_or_else(|| {
        BismarkDedupError::MissingAlignmentStart {
            qname: qname_lossy(r1),
        }
    })?)
    .expect("alignment_start fits in u32 per BAM spec");
    let r2_start = u32::try_from(r2.alignment_start().ok_or_else(|| {
        BismarkDedupError::MissingAlignmentStart {
            qname: qname_lossy(r2),
        }
    })?)
    .expect("alignment_start fits in u32 per BAM spec");

    let (start, end) = if is_forward(pair.pair_strand()) {
        // OT / CTOB: start = R1.alignment_start, end = R2 reference_end.
        // Matches Perl lines 398–443.
        let end = u32::try_from(r2.cigar().reference_end(r2_start as usize))
            .expect("reference_end fits in u32 per BAM spec");
        (r1_start, end)
    } else {
        // CTOT / OB: end = R1 reference_end, start = R2.alignment_start.
        // Matches Perl lines 446–492.
        let end = u32::try_from(r1.cigar().reference_end(r1_start as usize))
            .expect("reference_end fits in u32 per BAM spec");
        (r2_start, end)
    };
    Ok(DedupKey::pe(pair.pair_strand(), chr_id, start, end))
}

fn qname_lossy(record: &BismarkRecord) -> String {
    record
        .inner()
        .name()
        .map(|n| String::from_utf8_lossy(AsRef::as_ref(n)).into_owned())
        .unwrap_or_default()
}

/// Stream SE records: per-record, compute key, observe, write on unique.
fn stream_se(
    records: impl Iterator<Item = Result<BismarkRecord, bismark_io::BismarkIoError>>,
    refid_table: &[u32],
    state: &mut DedupState,
    writer: &mut Writer,
) -> Result<(), BismarkDedupError> {
    for record_result in records {
        let record = record_result?;
        let key = compute_se_key(&record, refid_table)?;
        if state.observe(key) {
            writer.write_record(&record)?;
        }
    }
    Ok(())
}

/// Stream PE records: pair two adjacent records at a time via
/// [`BismarkPair::from_mates`] (qname-equality + R1/R2 read-identity
/// enforced there); compute key, observe, write **both** mates on unique.
fn stream_pe(
    records: impl Iterator<Item = Result<BismarkRecord, bismark_io::BismarkIoError>>,
    refid_table: &[u32],
    state: &mut DedupState,
    writer: &mut Writer,
) -> Result<(), BismarkDedupError> {
    let mut iter = records;
    loop {
        let r1 = match iter.next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => return Err(e.into()),
            None => break,
        };
        let r2 = match iter.next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => return Err(e.into()),
            None => {
                return Err(BismarkDedupError::UnpairedFinalRecord {
                    qname: qname_lossy(&r1),
                });
            }
        };
        // BismarkPair::from_mates validates:
        //   - r1.read_identity == R1
        //   - r2.read_identity == R2
        //   - r1.qname == r2.qname
        // (Closes Alan's port's missing PE-mate validation gap.)
        let pair = BismarkPair::from_mates(r1, r2)?;
        let key = compute_pe_key(&pair, refid_table)?;
        if state.observe(key) {
            writer.write_record(pair.r1())?;
            writer.write_record(pair.r2())?;
        }
    }
    Ok(())
}

/// Run dedup on a **single** input file. The simplest path; covers the
/// vast majority of real-world Bismark dedup invocations.
///
/// # Errors
/// Returns [`BismarkDedupError`] on any I/O, format, or contract violation.
///
/// # Behaviour
/// 1. Open reader (BAM/SAM/CRAM auto-detected from extension).
/// 2. Peek the first record. If `None`, return [`BismarkDedupError::EmptyInput`]
///    **before** any writer or report file is created.
/// 3. Clone the header; build chr-name intern + refid table.
/// 4. Open writer (output format mirrors input format).
/// 5. Stream records (SE or PE per `is_paired`).
/// 6. Finalize writer; return [`DedupReport`].
pub fn run_single(
    input: &Path,
    output: &Path,
    cram_ref: Option<&Path>,
    is_paired: bool,
    file_label: String,
) -> Result<DedupReport, BismarkDedupError> {
    let mut reader = bismark_io::open_reader(input, cram_ref)?;
    let header = reader.header().clone();

    // Peek first record before opening writer.
    let mut records = reader.records().peekable();
    if records.peek().is_none() {
        return Err(BismarkDedupError::EmptyInput(input.to_path_buf()));
    }

    let intern = build_chr_intern(&header);
    let refid_table = build_refid_table(&header, &intern)?;

    let mut writer = bismark_io::open_writer(output, header, cram_ref)?;
    let mut state = DedupState::new();

    if is_paired {
        stream_pe(records, &refid_table, &mut state, &mut writer)?;
    } else {
        stream_se(records, &refid_table, &mut state, &mut writer)?;
    }

    writer.finish()?;
    Ok(state.into_report(file_label))
}

/// Run dedup on **multiple** input files treated as one combined sample.
/// Mirrors Perl `deduplicate_bismark`'s `--multiple` mode (lines 193–201).
///
/// All inputs must share:
/// - File format (all BAM, all SAM, or all CRAM — no mixing).
/// - `@SQ` chromosome **name** sets (order may differ; only the set is
///   validated). Mismatches produce [`BismarkDedupError::MultipleSqMismatch`].
///
/// The first file's header is written to the output. All inputs' records
/// flow into a single shared [`DedupState`].
///
/// # Errors
/// Returns [`BismarkDedupError`] on any I/O, format, or contract violation.
pub fn run_multiple(
    inputs: &[PathBuf],
    output: &Path,
    cram_ref: Option<&Path>,
    is_paired: bool,
    file_label: String,
) -> Result<DedupReport, BismarkDedupError> {
    if inputs.is_empty() {
        return Err(BismarkDedupError::EmptyInput(PathBuf::new()));
    }
    if inputs.len() == 1 {
        return run_single(&inputs[0], output, cram_ref, is_paired, file_label);
    }

    // Validate all formats match.
    let first_kind = bismark_io::AlignmentKind::from_path(&inputs[0])?;
    for path in &inputs[1..] {
        if bismark_io::AlignmentKind::from_path(path)? != first_kind {
            return Err(BismarkDedupError::MultipleMixedFormat);
        }
    }

    // Open all readers and capture their headers up front. We need each
    // header for chr-consistency validation before opening the writer.
    let mut readers: Vec<_> = inputs
        .iter()
        .map(|p| bismark_io::open_reader(p, cram_ref))
        .collect::<Result<Vec<_>, _>>()?;
    let headers: Vec<Header> = readers.iter().map(|r| r.header().clone()).collect();

    // Build intern from file1; validate all others.
    let intern = build_chr_intern(&headers[0]);
    for (i, header) in headers.iter().enumerate().skip(1) {
        validate_chr_consistency(&inputs[i], header, &intern)?;
    }

    let refid_tables: Vec<Vec<u32>> = headers
        .iter()
        .map(|h| build_refid_table(h, &intern))
        .collect::<Result<Vec<_>, _>>()?;

    // Peek file1 for empty before opening writer.
    {
        let mut peek_iter = readers[0].records().peekable();
        if peek_iter.peek().is_none() {
            return Err(BismarkDedupError::EmptyInput(inputs[0].clone()));
        }
        // peek_iter dropped at end of scope, releasing borrow of readers[0].
    }

    // Open writer with file1's header. Output format follows input.
    let mut writer = bismark_io::open_writer(output, headers[0].clone(), cram_ref)?;
    let mut state = DedupState::new();

    // Stream each input in order, accumulating into the shared state.
    for (i, reader) in readers.iter_mut().enumerate() {
        let records = reader.records();
        if is_paired {
            stream_pe(records, &refid_tables[i], &mut state, &mut writer)?;
        } else {
            stream_se(records, &refid_tables[i], &mut state, &mut writer)?;
        }
    }

    writer.finish()?;
    Ok(state.into_report(file_label))
}

#[cfg(test)]
mod tests {
    use super::*;
    use noodles_sam::header::record::value::Map;
    use noodles_sam::header::record::value::map::ReferenceSequence;
    use std::num::NonZeroUsize;

    fn header_with_chrs(names: &[&str]) -> Header {
        let mut header = Header::default();
        for name in names {
            header.reference_sequences_mut().insert(
                BString::from(*name),
                Map::<ReferenceSequence>::new(NonZeroUsize::try_from(1000).unwrap()),
            );
        }
        header
    }

    #[test]
    fn build_chr_intern_assigns_zero_based_indices_in_sq_order() {
        let header = header_with_chrs(&["chr1", "chr2", "chrX"]);
        let intern = build_chr_intern(&header);
        assert_eq!(intern[&BString::from("chr1")], 0);
        assert_eq!(intern[&BString::from("chr2")], 1);
        assert_eq!(intern[&BString::from("chrX")], 2);
    }

    #[test]
    fn build_refid_table_single_file_is_identity() {
        let header = header_with_chrs(&["chr1", "chr2", "chr3"]);
        let intern = build_chr_intern(&header);
        let table = build_refid_table(&header, &intern).unwrap();
        assert_eq!(table, vec![0, 1, 2]);
    }

    #[test]
    fn build_refid_table_reordered_header_maps_to_intern_indices() {
        // File1's intern: chr1=0, chr2=1, chr3=2
        // File2's @SQ order: chr2, chr3, chr1 (different)
        // build_refid_table on file2 yields refid → file1's chr_id
        let intern = build_chr_intern(&header_with_chrs(&["chr1", "chr2", "chr3"]));
        let file2_header = header_with_chrs(&["chr2", "chr3", "chr1"]);
        let table = build_refid_table(&file2_header, &intern).unwrap();
        // file2 refid 0 (chr2) → file1's chr_id 1
        // file2 refid 1 (chr3) → file1's chr_id 2
        // file2 refid 2 (chr1) → file1's chr_id 0
        assert_eq!(table, vec![1, 2, 0]);
    }

    #[test]
    fn validate_chr_consistency_passes_on_identical_name_sets() {
        let intern = build_chr_intern(&header_with_chrs(&["chr1", "chr2", "chr3"]));
        let other = header_with_chrs(&["chr3", "chr1", "chr2"]); // reordered OK
        validate_chr_consistency(Path::new("other.bam"), &other, &intern).unwrap();
    }

    #[test]
    fn validate_chr_consistency_fails_on_missing_chr() {
        let intern = build_chr_intern(&header_with_chrs(&["chr1", "chr2", "chr3"]));
        let other = header_with_chrs(&["chr1", "chr2"]); // missing chr3
        let err = validate_chr_consistency(Path::new("other.bam"), &other, &intern).unwrap_err();
        match err {
            BismarkDedupError::MultipleSqMismatch {
                missing_chrs,
                offending_file,
            } => {
                assert_eq!(offending_file, PathBuf::from("other.bam"));
                assert!(
                    missing_chrs.iter().any(|c| c == "chr3"),
                    "got: {missing_chrs:?}"
                );
            }
            other => panic!("expected MultipleSqMismatch, got {other:?}"),
        }
    }

    #[test]
    fn validate_chr_consistency_fails_on_extra_chr() {
        let intern = build_chr_intern(&header_with_chrs(&["chr1", "chr2"]));
        let other = header_with_chrs(&["chr1", "chr2", "chr3"]); // extra chr3
        let err = validate_chr_consistency(Path::new("other.bam"), &other, &intern).unwrap_err();
        match err {
            BismarkDedupError::MultipleSqMismatch { missing_chrs, .. } => {
                assert!(
                    missing_chrs.iter().any(|c| c.contains("chr3")),
                    "got: {missing_chrs:?}"
                );
            }
            other => panic!("expected MultipleSqMismatch, got {other:?}"),
        }
    }

    #[test]
    fn is_forward_classification() {
        assert!(is_forward(BismarkStrand::OT));
        assert!(is_forward(BismarkStrand::CTOB));
        assert!(!is_forward(BismarkStrand::CTOT));
        assert!(!is_forward(BismarkStrand::OB));
    }
}
