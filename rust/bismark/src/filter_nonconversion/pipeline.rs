//! Per-file orchestration: read a Bismark BAM with raw noodles `RecordBuf`s,
//! route each read (SE) / read-pair (PE) to the kept or removed output by the
//! XM decision, and write the byte-exact report.
//!
//! ## Why raw `RecordBuf` (not `crate::io::BismarkRecord`)
//!
//! `filter_non_conversion` is a verbatim pass-through that needs only the XM
//! tag, computes no strand, and must preserve **every** record including
//! unmapped reads. `bismark_io`'s reader silently drops unmapped reads
//! (`FLAG & 0x4`) and its `BismarkRecord` errors when XR/XG are absent —
//! neither is faithful here. So we read/write through noodles directly
//! (`noodles_bam::io::Reader::record_bufs` + `noodles_bam::io::Writer`). This
//! mirrors the `bam2nuc` C-1 decision. `bismark_io` is still used for the
//! `@PG` SE/PE auto-detect (`detect_paired_from_header`).
//!
//! The noodles `RecordBuf` round-trip is body-byte-identical to Perl's
//! `samtools view -h | … | samtools view -bS -` pipe — proven by the spike
//! (`spikes/SPIKE_noodles_roundtrip.md`).

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use noodles_bam as bam;
use noodles_sam::Header;
use noodles_sam::alignment::RecordBuf;
use noodles_sam::alignment::io::Write as _;
use noodles_sam::alignment::record::data::field::Tag;
use noodles_sam::alignment::record_buf::data::field::Value;
use noodles_sam::header::record::value::map::header::sort_order::COORDINATE;
use noodles_sam::header::record::value::map::header::tag::SORT_ORDER;

use crate::filter_nonconversion::error::BismarkFilterError;
use crate::filter_nonconversion::filename;
use crate::filter_nonconversion::filter::{FilterMode, read_fails};
use crate::filter_nonconversion::report::FilterReport;

/// Concrete BAM writer type (noodles wraps the `File` in a BGZF encoder).
type BamWriter = bam::io::Writer<noodles_bgzf::io::Writer<File>>;

/// Filter a single input BAM. Writes the kept BAM, removed BAM, and the
/// report file (SUMMARY block — the run-time line is appended later, only to
/// the last file, by [`crate::filter_nonconversion::run`]). Returns the [`FilterReport`] for the
/// caller's STDERR echo + timing-line bookkeeping.
///
/// `explicit_mode`: `Some(true)` = PE, `Some(false)` = SE, `None` = detect
/// from the `@PG` header.
pub fn filter_one(
    infile: &Path,
    mode: FilterMode,
    explicit_mode: Option<bool>,
) -> Result<FilterReport, BismarkFilterError> {
    let infile_str = infile.to_string_lossy().into_owned();

    // 4.1 BAM filename gate (Perl line 37, `=~ /bam$/`, no dot anchor).
    if !infile_str.ends_with("bam") {
        return Err(BismarkFilterError::NotABamFile);
    }
    // Truncation/emptiness checks are gated on the *dotted* `\.bam$`
    // (Perl lines 42/47) — a `bam`-but-not-`.bam` name skips them.
    let dotted_bam = infile_str.ends_with(".bam");

    if dotted_bam {
        // Perl `bam_isTruncated` line 635 emits this notice (STDERR, not gated)
        // before scanning for truncation; the scan itself is noodles-native here.
        eprintln!("Checking file >>{infile_str}<< for signs of file truncation...");
    }

    // Open the reader + header. An I/O error here on a `.bam` file is
    // reported as truncation (Perl `bam_isTruncated`, noodles-native §4.2).
    let mut reader = bam::io::Reader::new(BufReader::new(File::open(infile)?));
    let header = reader
        .read_header()
        .map_err(|e| map_initial_read_err(e, dotted_bam))?;

    // 4.4 SE/PE determination.
    let is_paired = match explicit_mode {
        Some(p) => p,
        None => crate::io::detect_paired_from_header(&header).ok_or_else(|| {
            BismarkFilterError::CannotAutoDetectMode {
                input: infile.to_path_buf(),
            }
        })?,
    };

    // 4.5 PE positional-sort: reject coordinate-sorted input BEFORE opening
    // writers (faithful no-output for the common case; the rare no-SO-but-
    // misordered case is caught mid-stream by the adjacent-qname check).
    if is_paired && is_coordinate_sorted(&header) {
        return Err(BismarkFilterError::CoordinateSorted);
    }

    // 4.3 Emptiness: peek the first alignment record. The peek block releases
    // the reader borrow on scope exit; the underlying reader is left
    // positioned just past record 1, so streaming resumes from record 2 and
    // we prepend the stashed first record.
    let first = {
        let mut peek = reader.record_bufs(&header);
        peek.next()
    };
    let first_rec: Option<RecordBuf> = match first {
        None => {
            if dotted_bam {
                // Header-only `.bam` → die before any output (Perl bam_isEmpty).
                return Err(BismarkFilterError::EmptyInput);
            }
            // Header-only `*bam` (no dot) skips the empty check → process with
            // count == 0 → an N/A report (SPEC §4.3 C1). Output BAMs are
            // header-only.
            None
        }
        Some(Ok(r)) => Some(r),
        Some(Err(e)) => return Err(map_initial_read_err(e, dotted_bam)),
    };

    // Open both output writers + create the report file empty. Perl opens
    // OUT/REMOVED/REPORT upfront (lines 88–98); a mid-stream die therefore
    // leaves valid partial BAMs and a 0-byte report.
    let kept_path = filename::kept_bam_name(&infile_str);
    let removed_path = filename::removed_bam_name(&infile_str);
    let report_path = filename::report_name(&infile_str);

    let mut kept_w = bam::io::Writer::new(File::create(&kept_path)?);
    kept_w.write_header(&header)?;
    let mut removed_w = bam::io::Writer::new(File::create(&removed_path)?);
    removed_w.write_header(&header)?;
    File::create(&report_path)?; // empty (0 bytes) until the SUMMARY is written

    // Stream records (prepending the stashed first record).
    let records = reader.record_bufs(&header);
    let stream_result: Result<(u64, u64), BismarkFilterError> = match first_rec {
        None => Ok((0, 0)),
        Some(fr) => {
            let combined = std::iter::once(Ok(fr)).chain(records);
            if is_paired {
                stream_pe(combined, mode, &header, &mut kept_w, &mut removed_w)
            } else {
                stream_se(combined, mode, &header, &mut kept_w, &mut removed_w)
            }
        }
    };

    // Finalise both writers regardless of the stream result, so that on a
    // mid-stream die (PE lone-R1 / missing-XM / qname mismatch) the partial
    // BAMs are still valid (matching samtools finalising on pipe close).
    kept_w.try_finish()?;
    removed_w.try_finish()?;

    // Propagate any streaming error AFTER finalising the writers; the report
    // stays 0 bytes (already created empty above).
    let (count, kicked) = stream_result?;

    // Success → write the SUMMARY block (Line A + Line B). The run-time line
    // is appended by `run` to the last file only.
    let report = FilterReport {
        infile: infile_str,
        is_paired,
        count,
        kicked,
        mode,
    };
    std::fs::write(&report_path, report.format())?;
    Ok(report)
}

/// Stream single-end records: count each, route by the XM decision. A read
/// with no XM tag has an empty call string → never fails → kept (Perl SE
/// `split //, undef`).
fn stream_se<I>(
    records: I,
    mode: FilterMode,
    header: &Header,
    kept: &mut BamWriter,
    removed: &mut BamWriter,
) -> Result<(u64, u64), BismarkFilterError>
where
    I: Iterator<Item = std::io::Result<RecordBuf>>,
{
    let mut count: u64 = 0;
    let mut kicked: u64 = 0;
    for rr in records {
        let rec = rr?;
        count += 1;
        let xm = extract_xm(&rec).unwrap_or(b"");
        if read_fails(xm, mode) {
            kicked += 1;
            removed.write_alignment_record(header, &rec)?;
        } else {
            kept.write_alignment_record(header, &rec)?;
        }
    }
    Ok((count, kicked))
}

/// Stream paired-end records two-at-a-time. Either mate failing removes the
/// whole pair. Faithful order (Perl `process_file` 186–306):
/// 1. read R1; read R2 (`None` at a pair boundary → lone-R1 die);
/// 2. adjacent-qname check (sort detection, folded in from the Perl pre-pass);
/// 3. both mates must have a non-empty XM (Perl `and`-truthiness) else die;
/// 4. count the pair; R1 fails → pair fails (R2 not examined, via `||`
///    short-circuit); route both mates together.
fn stream_pe<I>(
    mut records: I,
    mode: FilterMode,
    header: &Header,
    kept: &mut BamWriter,
    removed: &mut BamWriter,
) -> Result<(u64, u64), BismarkFilterError>
where
    I: Iterator<Item = std::io::Result<RecordBuf>>,
{
    let mut count: u64 = 0;
    let mut kicked: u64 = 0;
    loop {
        let r1 = match records.next() {
            None => break,
            Some(rr) => rr?,
        };
        let r2 = match records.next() {
            // Lone trailing R1 (odd record count): Perl dies at line 194 with
            // R2 undef, prior pairs already written, report left 0 bytes.
            None => return Err(BismarkFilterError::PairedMissingMethCall),
            Some(rr) => rr?,
        };

        if !qnames_match(&r1, &r2) {
            return Err(BismarkFilterError::PairedIdMismatch {
                read1: qname_string(&r1),
                read2: qname_string(&r2),
            });
        }

        // Perl `unless($meth_call_1 and $meth_call_2)`: absent OR empty XM in
        // either mate (both falsy in Perl) triggers the die.
        let xm1 = extract_xm(&r1).filter(|s| !s.is_empty());
        let xm2 = extract_xm(&r2).filter(|s| !s.is_empty());
        let (xm1, xm2) = match (xm1, xm2) {
            (Some(a), Some(b)) => (a, b),
            _ => return Err(BismarkFilterError::PairedMissingMethCall),
        };

        count += 1;
        // R1 fails → pair fails; `||` short-circuits so R2 is not examined
        // when R1 already fails (Perl `unless ($sequence_fails)` guard).
        let fails = read_fails(xm1, mode) || read_fails(xm2, mode);
        if fails {
            kicked += 1;
            removed.write_alignment_record(header, &r1)?;
            removed.write_alignment_record(header, &r2)?;
        } else {
            kept.write_alignment_record(header, &r1)?;
            kept.write_alignment_record(header, &r2)?;
        }
    }
    Ok((count, kicked))
}

/// Extract the `XM:Z:` value bytes, or `None` if absent / non-string.
fn extract_xm(rec: &RecordBuf) -> Option<&[u8]> {
    match rec.data().get(&Tag::from(*b"XM")) {
        Some(Value::String(s)) => Some(s.as_ref()),
        _ => None,
    }
}

/// The record's qname bytes (empty if unset).
fn record_qname(rec: &RecordBuf) -> &[u8] {
    rec.name().map(AsRef::<[u8]>::as_ref).unwrap_or(b"")
}

/// Adjacent-qname equality after stripping legacy `/1`,`/2` suffixes (Perl
/// `test_positional_sorting` 447–459).
fn qnames_match(r1: &RecordBuf, r2: &RecordBuf) -> bool {
    let n1 = record_qname(r1);
    let n2 = record_qname(r2);
    if n1 == n2 {
        return true;
    }
    let n1t = n1.strip_suffix(b"/1").unwrap_or(n1);
    let n2t = n2.strip_suffix(b"/2").unwrap_or(n2);
    n1t == n2t
}

fn qname_string(rec: &RecordBuf) -> String {
    String::from_utf8_lossy(record_qname(rec)).into_owned()
}

/// True if the header declares `@HD SO:coordinate`.
fn is_coordinate_sorted(header: &Header) -> bool {
    header
        .header()
        .and_then(|hd| hd.other_fields().get(&SORT_ORDER))
        .map(|so| AsRef::<[u8]>::as_ref(so) == COORDINATE)
        .unwrap_or(false)
}

/// Map an I/O error from the initial header/first-record read to
/// [`BismarkFilterError::Truncated`] for a `.bam`-named input (Perl's
/// truncation check is gated on `\.bam$`), else a plain I/O error.
fn map_initial_read_err(e: std::io::Error, dotted_bam: bool) -> BismarkFilterError {
    if dotted_bam {
        BismarkFilterError::Truncated { source: e }
    } else {
        BismarkFilterError::Io(e)
    }
}
