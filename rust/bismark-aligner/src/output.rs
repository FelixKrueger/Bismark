//! SAM/BAM record + header assembly — a port of Perl `generate_SAM_header`
//! (8452–8484), `single_end_SAM_output` (8489–8711), `make_mismatch_string`
//! (9252–9595), `hemming_dist` (9235), and `revcomp` (9228).
//!
//! Builds the noodles [`Header`] (`@HD`/`@SQ`/`@PG`) and, per `UniqueBest`
//! alignment, a [`BismarkRecord`] carrying FLAG / POS / MAPQ / CIGAR / SEQ /
//! QUAL + the `NM`/`MD`/`XM`/`XR`/`XG` tags — written to a `bismark-io`
//! [`BamWriter`]. The samtools-pipe `@PG` line is **not** reproduced (gate
//! policy P1: normalised out of the comparison); Bismark's own `@PG` is exact.

use std::collections::HashMap;
use std::num::NonZeroUsize;

use bstr::BString;
use noodles_core::Position;
use noodles_sam::Header;
use noodles_sam::alignment::RecordBuf;
use noodles_sam::alignment::record::cigar::Op;
use noodles_sam::alignment::record::cigar::op::Kind;
use noodles_sam::alignment::record::data::field::Tag;
use noodles_sam::alignment::record::{Flags, MappingQuality};
use noodles_sam::alignment::record_buf::data::field::Value;
use noodles_sam::alignment::record_buf::{Cigar, QualityScores, Sequence};
use noodles_sam::header::record::value::Map;
use noodles_sam::header::record::value::map::header::Version;
use noodles_sam::header::record::value::map::header::sort_order;
use noodles_sam::header::record::value::map::header::tag::SORT_ORDER;
use noodles_sam::header::record::value::map::program::tag::{COMMAND_LINE, VERSION};
use noodles_sam::header::record::value::map::{self, Program, ReferenceSequence};

use bismark_io::{BamWriter, BismarkRecord};

use crate::error::{AlignerError, Result};
use crate::genome::Genome;
use crate::merge::BestAlignment;
use crate::methylation::{Conversion, GenomicExtraction, parse_cigar};

/// Map chromosome name → reference id (0-based index into `sq_order`), the
/// `reference_sequence_id` the BAM record needs.
pub fn build_refid(genome: &Genome) -> HashMap<String, usize> {
    genome
        .sq_order
        .iter()
        .enumerate()
        .map(|(i, name)| (name.clone(), i))
        .collect()
}

/// Build the SAM header (Perl `generate_SAM_header`, 8452–8484): `@HD VN:1.0
/// SO:unsorted`, one `@SQ SN:.. LN:..` per chromosome in `sq_order`, and the
/// Bismark `@PG` (`ID:Bismark VN:<version> CL:"bismark <command_line>"`). The
/// samtools-pipe `@PG` is intentionally absent (gate policy P1).
pub fn generate_sam_header(genome: &Genome, command_line: &str) -> Header {
    // @HD: VN typed, SO via other_fields (insertion order = serialised order).
    let mut hd = Map::<map::Header>::new(Version::new(1, 0));
    hd.other_fields_mut()
        .insert(SORT_ORDER, BString::from(sort_order::UNSORTED));

    // @PG: ID then other_fields in insertion order → VN before CL (Perl 8480).
    let mut prog = Map::<Program>::default();
    prog.other_fields_mut()
        .insert(VERSION, BString::from(crate::BISMARK_VERSION.as_bytes()));
    // Perl 8480 emits the CL value WITH literal surrounding double-quotes:
    // `CL:"bismark <argv>"`.
    prog.other_fields_mut().insert(
        COMMAND_LINE,
        BString::from(format!("\"bismark {command_line}\"").into_bytes()),
    );

    let mut header = Header::builder()
        .set_header(hd)
        .add_program(BString::from(&b"Bismark"[..]), prog)
        .build();

    // @SQ in sq_order. noodles serialises @HD, then @SQ, then @PG (SAM order),
    // regardless of insertion order, so adding @SQ after @PG is fine.
    for name in &genome.sq_order {
        let len = genome.chromosomes.get(name).map(Vec::len).unwrap_or(0);
        // LN must be > 0 in noodles; an empty chromosome (Perl LN:0) is
        // pathological and excluded from real test genomes.
        let len_nz = NonZeroUsize::new(len).unwrap_or(NonZeroUsize::MIN);
        header.reference_sequences_mut().insert(
            BString::from(name.as_bytes()),
            Map::<ReferenceSequence>::new(len_nz),
        );
    }
    header
}

/// `hemming_dist` (Perl 9235–9244): count base-by-base inequalities over
/// `actual` vs `ref_seq` (positions past `ref_seq`'s end count as differences;
/// `X` padding bases mismatch — intentionally counted, then `indels` is added by
/// the caller).
pub(crate) fn hemming_dist(actual: &[u8], ref_seq: &[u8]) -> usize {
    let matches = actual
        .iter()
        .zip(ref_seq.iter())
        .filter(|(a, b)| a == b)
        .count();
    actual.len() - matches
}

/// `revcomp` (Perl 9228–9233): `reverse` then `tr/ACTGactg/TGACTGAC/`. Both
/// cases are complemented (lower-case → UPPER-case complement); `N`/`X`/other
/// bytes are unchanged. (Distinct from `methylation::reverse_complement`, 5161.)
pub(crate) fn revcomp(seq: &[u8]) -> Vec<u8> {
    seq.iter()
        .rev()
        .map(|&b| match b {
            b'A' | b'a' => b'T',
            b'C' | b'c' => b'G',
            b'T' | b't' => b'A',
            b'G' | b'g' => b'C',
            other => other,
        })
        .collect()
}

/// `make_mismatch_string` (Perl 9252–9595): build the `MD:Z:` string from the
/// read vs the (trimmed, possibly-revcomp'd) reference, with the deletion
/// re-indexing path (`^<bases>`) and the `X`-padding skip for insertions/
/// soft-clips. Returns the full `"MD:Z:..."` string. Ported verbatim — the
/// deletion machinery is the highest byte-identity risk in this phase.
pub(crate) fn make_mismatch_string(
    actual: &[u8],
    ref_seq: &[u8],
    cigar: &str,
    md_sequence: &[u8],
) -> String {
    // Part 1: match-run / mismatch builder (Perl 9276–9319). `actual` and
    // `ref_seq` have equal length here (both trimmed to the read length); the
    // `None` arm only documents Perl's past-the-end `substr` (= empty string,
    // so: append the run count + an empty ref base, never a NUL).
    let mut md_tag = String::from("MD:Z:");
    let mut prev_matching: i64 = 0;
    for (pos, &actual_base) in actual.iter().enumerate() {
        match ref_seq.get(pos) {
            Some(&ref_base) if actual_base == ref_base => prev_matching += 1,
            Some(&b'X') => { /* insertion / soft-clip padding → ignored (Perl 9295) */ }
            Some(&ref_base) => {
                md_tag.push_str(&prev_matching.to_string());
                md_tag.push(ref_base as char);
                prev_matching = 0;
            }
            None => {
                // Perl: `$ref_base` is "" → mismatch, not 'X' → append count + "".
                md_tag.push_str(&prev_matching.to_string());
                prev_matching = 0;
            }
        }
    }
    md_tag.push_str(&prev_matching.to_string());

    // Part 2: deletion re-indexing (Perl 9325–9591) — only when CIGAR has 'D'.
    if cigar.contains('D') {
        md_tag = rebuild_md_with_deletions(&md_tag, cigar, md_sequence);
    }
    md_tag
}

/// `substr($md_sequence, $offset, $len)` with Perl's lenient bounds.
fn md_substr(md_sequence: &[u8], offset: i64, len: i64) -> String {
    if offset < 0 {
        return String::new();
    }
    let start = (offset as usize).min(md_sequence.len());
    let end = ((offset + len) as usize).min(md_sequence.len());
    String::from_utf8_lossy(&md_sequence[start..end]).into_owned()
}

/// The deletion-handling rebuild (Perl 9325–9591). Ported variable-for-variable.
fn rebuild_md_with_deletions(md_tag_in: &str, cigar: &str, md_sequence: &[u8]) -> String {
    let deletions_total = cigar.bytes().filter(|&b| b == b'D').count() as i64;
    let runs: Vec<(i64, u8)> = parse_cigar(cigar)
        .unwrap_or_default()
        .into_iter()
        .map(|(l, o)| (l as i64, o))
        .collect();

    let mut md_pos_so_far: i64 = 0;
    let mut deletions_processed: i64 = 0;
    let mut del_pos: i64 = 0;

    // new_MD = the part after "MD:Z:" (Perl 9345); @md = its characters.
    let value = md_tag_in
        .strip_prefix("MD:Z:")
        .unwrap_or(md_tag_in)
        .to_string();
    let mut md: Vec<char> = value.chars().collect();
    let mut md_tag = String::from("MD:Z:");
    let mut new_md = String::new();
    let mut md_index_already_processed: Option<i64> = None;

    for (len, op) in &runs {
        let len = *len;
        match op {
            b'M' => del_pos += len,
            b'N' => {} // skipped region: ignored
            b'I' | b'S' => {
                del_pos += len;
                md_pos_so_far += len;
            }
            b'D' => {
                let deleted_bases = md_substr(md_sequence, del_pos, len);

                let mut op_acc: Option<String> = None;
                let mut this_deletion_processed = false;
                let mut current_md_index: Option<i64> = None;

                for &el in &md {
                    current_md_index = Some(match current_md_index {
                        None => 0,
                        Some(v) => v + 1,
                    });
                    let cmi = current_md_index.unwrap();

                    if let Some(mip) = md_index_already_processed
                        && cmi <= mip
                    {
                        new_md.push(el);
                        continue;
                    }
                    if op_acc.is_none() {
                        op_acc = Some(el.to_string());
                        continue;
                    }
                    if deletions_processed == deletions_total {
                        md_tag.push(el);
                        new_md.push(el);
                        continue;
                    }
                    if this_deletion_processed {
                        new_md.push(el);
                        continue;
                    }

                    let op_str = op_acc.clone().unwrap();
                    if !op_str.is_empty() && op_str.bytes().all(|b| b.is_ascii_digit()) {
                        // op so far is a number
                        if el.is_ascii_digit() {
                            op_acc = Some(format!("{op_str}{el}"));
                            continue;
                        }
                        // current element is a word char (mismatch base)
                        let op_num: i64 = op_str.parse().unwrap();
                        md_pos_so_far += op_num;
                        if md_pos_so_far < del_pos {
                            md_tag.push_str(&op_str);
                            new_md.push_str(&op_str);
                            op_acc = Some(el.to_string());
                        } else {
                            let pos_after_deletion = md_pos_so_far - del_pos;
                            let pos_before_deletion = op_num - pos_after_deletion;
                            md_tag.push_str(&format!("{pos_before_deletion}^{deleted_bases}"));
                            new_md.push_str(&format!(
                                "{pos_before_deletion}^{deleted_bases}{pos_after_deletion}"
                            ));
                            md_pos_so_far -= pos_after_deletion;
                            new_md.push(el);
                            deletions_processed += 1;
                            this_deletion_processed = true;
                            if deletions_processed == deletions_total {
                                md_tag.push_str(&pos_after_deletion.to_string());
                                md_tag.push(el);
                                del_pos += len;
                            } else {
                                let delstr_len = format!("{pos_before_deletion}^{deleted_bases}")
                                    .chars()
                                    .count()
                                    as i64;
                                current_md_index =
                                    Some(cmi + delstr_len - op_str.chars().count() as i64);
                                md_index_already_processed = Some(current_md_index.unwrap() - 1);
                                del_pos += len;
                                md_pos_so_far += len;
                                op_acc = Some(String::new());
                            }
                        }
                    } else {
                        // op so far is a word char (mismatch base)
                        if el.is_ascii_digit() {
                            md_tag.push_str(&op_str);
                            new_md.push_str(&op_str);
                            md_pos_so_far += op_str.chars().count() as i64;
                        }
                        // (a non-digit here "should never happen"; Perl dies — unreachable
                        // for a valid MD string, so we carry on like the digit case's tail.)
                        op_acc = Some(el.to_string());
                    }
                }

                // Tail: last element was a digit and a deletion remains (Perl 9526–9578).
                if let Some(op_str) = op_acc.clone()
                    && !op_str.is_empty()
                    && op_str.bytes().all(|b| b.is_ascii_digit())
                    && deletions_processed < deletions_total
                {
                    let op_num: i64 = op_str.parse().unwrap();
                    md_pos_so_far += op_num;
                    if md_pos_so_far >= del_pos {
                        let pos_after_deletion = md_pos_so_far - del_pos;
                        let pos_before_deletion = op_num - pos_after_deletion;
                        md_tag.push_str(&format!("{pos_before_deletion}^{deleted_bases}"));
                        new_md.push_str(&format!(
                            "{pos_before_deletion}^{deleted_bases}{pos_after_deletion}"
                        ));
                        md_pos_so_far -= pos_after_deletion;
                        deletions_processed += 1;
                        if deletions_processed == deletions_total {
                            md_tag.push_str(&pos_after_deletion.to_string());
                        } else {
                            let delstr_len = format!("{pos_before_deletion}^{deleted_bases}")
                                .chars()
                                .count() as i64;
                            let cmi = current_md_index.unwrap_or(0);
                            // NOT -1 here (Perl 9564): not in the loop, so no pre-increment.
                            md_index_already_processed =
                                Some(cmi + delstr_len - op_str.chars().count() as i64);
                            md_pos_so_far += len;
                        }
                        del_pos += len;
                    }
                    // else: Perl dies "Something went wrong" — unreachable for valid data.
                }

                // form a new @md (Perl 9581)
                md = new_md.chars().collect();
                new_md = String::new();
            }
            _ => {} // non-MIDSN: unreachable (extraction already validated the CIGAR)
        }
    }
    md_tag
}

/// Assemble one single-end SAM/BAM record (Perl `single_end_SAM_output`,
/// 8489–8711). `original_seq` is the uc original read; `qual` the raw ASCII
/// quality string from the FastQ. Returns a [`BismarkRecord`] ready for the BAM
/// writer (XR/XG/XM presence + `XM.len()==seq.len()` are re-validated there).
#[allow(clippy::too_many_arguments)]
pub fn single_end_sam_output(
    id: &str,
    original_seq: &[u8],
    qual: &[u8],
    best: &BestAlignment,
    ext: &GenomicExtraction,
    methylation_call: &[u8],
    refid: &HashMap<String, usize>,
    phred64: bool,
) -> Result<BismarkRecord> {
    let strand = ext.alignment_strand;
    let read_conv = ext.read_conversion;
    let genome_conv = ext.genome_conversion;

    // FLAG (Perl 8521–8546).
    let flag: u16 = match (strand, read_conv, genome_conv) {
        (b'+', Conversion::Ct, Conversion::Ct) => 0,
        (b'+', Conversion::Ga, Conversion::Ga) => 16,
        (b'-', Conversion::Ct, Conversion::Ga) => 16,
        (b'-', Conversion::Ga, Conversion::Ct) => 0,
        _ => {
            return Err(AlignerError::Validation(format!(
                "Unexpected strand and read/genome conversion: strand {}, read {}, genome {}",
                strand as char,
                read_conv.as_str(),
                genome_conv.as_str()
            )));
        }
    };

    // ref_seq: drop the +2 padding (Perl 8570–8575). CT → drop last 2; else first 2.
    let g = &ext.unmodified_genomic_sequence;
    let mut ref_seq: Vec<u8> = match read_conv {
        Conversion::Ct => g[..g.len().saturating_sub(2)].to_vec(),
        Conversion::Ga => g.get(2..).unwrap_or(&[]).to_vec(),
    };
    let mut actual_seq = original_seq.to_vec();
    let mut md_seq = ext.genomic_seq_for_md_tag.clone();

    // QUAL → phred SCORES for the BAM (ASCII − offset). phred64 input (Perl 4191)
    // uses offset 64; default phred33 uses 33. `samtools view -h` re-renders ASCII+33.
    let offset: u8 = if phred64 { 64 } else { 33 };
    let mut scores: Vec<u8> = qual.iter().map(|&q| q.wrapping_sub(offset)).collect();

    // Minus-strand reorientation (Perl 8577–8584).
    if strand == b'-' {
        actual_seq = revcomp(&actual_seq);
        ref_seq = revcomp(&ref_seq);
        if best.cigar.contains('D') {
            md_seq = revcomp(&md_seq); // second revcomp (extraction did the first for index 1/2)
        }
        scores.reverse();
    }

    // NM = hemming_dist + indels (Perl 8588–8592).
    let nm = hemming_dist(&actual_seq, &ref_seq) as i64 + ext.indels as i64;

    // MD (Perl 8596).
    let md_full = make_mismatch_string(&actual_seq, &ref_seq, &best.cigar, &md_seq);
    let md_value = md_full
        .strip_prefix("MD:Z:")
        .unwrap_or(&md_full)
        .to_string();

    // XM, reversed if '-' (Perl 8601–8607).
    let xm: Vec<u8> = if strand == b'-' {
        methylation_call.iter().rev().copied().collect()
    } else {
        methylation_call.to_vec()
    };

    // Build the noodles record.
    let mut rec = RecordBuf::default();
    *rec.name_mut() = Some(BString::from(id.as_bytes()));
    *rec.flags_mut() = Flags::from(flag);
    let tid = *refid.get(&best.chromosome).ok_or_else(|| {
        AlignerError::Validation(format!("chromosome {} absent from @SQ", best.chromosome))
    })?;
    *rec.reference_sequence_id_mut() = Some(tid);
    *rec.alignment_start_mut() = Position::new(best.position as usize);
    *rec.mapping_quality_mut() = MappingQuality::new(best.mapq);
    *rec.cigar_mut() = Cigar::from(cigar_to_ops(&best.cigar));
    *rec.sequence_mut() = Sequence::from(actual_seq);
    *rec.quality_scores_mut() = QualityScores::from(scores);

    // Tags in Perl's order: NM, MD, XM, XR, XG (Perl 8706).
    rec.data_mut()
        .insert(Tag::from(*b"NM"), Value::from(nm as i32));
    rec.data_mut()
        .insert(Tag::from(*b"MD"), Value::String(BString::from(md_value)));
    rec.data_mut()
        .insert(Tag::from(*b"XM"), Value::String(BString::from(xm)));
    rec.data_mut().insert(
        Tag::from(*b"XR"),
        Value::String(BString::from(read_conv.as_str())),
    );
    rec.data_mut().insert(
        Tag::from(*b"XG"),
        Value::String(BString::from(genome_conv.as_str())),
    );

    BismarkRecord::from_noodles_record(rec)
        .map_err(|e| AlignerError::Validation(format!("failed to build SAM record: {e}")))
}

/// Parse a CIGAR string into noodles ops (M/I/D/S/N; pre-validated by extraction).
fn cigar_to_ops(cigar: &str) -> Vec<Op> {
    parse_cigar(cigar)
        .unwrap_or_default()
        .into_iter()
        .map(|(len, op)| {
            let kind = match op {
                b'M' => Kind::Match,
                b'I' => Kind::Insertion,
                b'D' => Kind::Deletion,
                b'S' => Kind::SoftClip,
                b'N' => Kind::Skip,
                _ => Kind::Match,
            };
            Op::new(kind, len as usize)
        })
        .collect()
}

/// Convenience: write one record to a [`BamWriter`].
pub fn write_record<W: std::io::Write>(
    writer: &mut BamWriter<W>,
    record: &BismarkRecord,
) -> Result<()> {
    writer
        .write_record(record)
        .map_err(|e| AlignerError::Validation(format!("failed to write BAM record: {e}")))
}

/// Write the first ambiguous alignment's **raw** aligner SAM line to the
/// `--ambig_bam` (Perl 2976). The line is Bowtie 2's own (carrying `AS`/`XS`/…
/// tags, not Bismark `XM`/`XR`/`XG`), so it is parsed into a bare [`RecordBuf`]
/// and written via [`BamWriter::write_raw_record`] (bypassing `BismarkRecord`
/// validation). The RNAME's `_(CT|GA)_converted` suffix is stripped off the
/// field (Perl `s/_(CT|GA)_converted//`); FLAG/POS/MAPQ/CIGAR/SEQ/QUAL and all
/// optional tags are preserved **verbatim, in input order**.
pub fn write_raw_sam_line_to_bam<W: std::io::Write>(
    writer: &mut BamWriter<W>,
    raw_line: &str,
    refid: &HashMap<String, usize>,
) -> Result<()> {
    let rec = build_raw_record(raw_line, refid)?;
    writer
        .write_raw_record(&rec)
        .map_err(|e| AlignerError::Validation(format!("failed to write ambig BAM record: {e}")))
}

/// Parse a raw (de-convertable) SAM line into a bare [`RecordBuf`] — the
/// `--ambig_bam` record assembly, factored out for direct testing.
fn build_raw_record(raw_line: &str, refid: &HashMap<String, usize>) -> Result<RecordBuf> {
    let f: Vec<&str> = raw_line.split('\t').collect();
    if f.len() < 11 {
        return Err(AlignerError::Validation(format!(
            "malformed ambiguous SAM line ({} fields): {raw_line}",
            f.len()
        )));
    }
    let flag: u16 = f[1]
        .parse()
        .map_err(|_| AlignerError::Validation(format!("bad FLAG in ambig line: {}", f[1])))?;
    // De-convert RNAME off the field only (byte-equivalent to Perl's whole-line
    // first-occurrence s/// for any real RNAME).
    let rname = f[2]
        .strip_suffix("_CT_converted")
        .or_else(|| f[2].strip_suffix("_GA_converted"))
        .unwrap_or(f[2]);
    let pos: usize = f[3]
        .parse()
        .map_err(|_| AlignerError::Validation(format!("bad POS in ambig line: {}", f[3])))?;
    let mapq: u8 = f[4]
        .parse()
        .map_err(|_| AlignerError::Validation(format!("bad MAPQ in ambig line: {}", f[4])))?;

    let mut rec = RecordBuf::default();
    *rec.name_mut() = Some(BString::from(f[0].as_bytes()));
    *rec.flags_mut() = Flags::from(flag);
    if let Some(&tid) = refid.get(rname) {
        *rec.reference_sequence_id_mut() = Some(tid);
    }
    if pos > 0 {
        *rec.alignment_start_mut() = Position::new(pos);
    }
    *rec.mapping_quality_mut() = MappingQuality::new(mapq);
    if f[5] != "*" {
        *rec.cigar_mut() = Cigar::from(cigar_to_ops(f[5]));
    }
    if f[9] != "*" {
        *rec.sequence_mut() = Sequence::from(f[9].as_bytes().to_vec());
    }
    if f[10] != "*" {
        // The aligner's raw QUAL is ALWAYS Phred+33 (Bowtie 2's own output), so the
        // offset is `-33` unconditionally — independent of the read's `--phred64`
        // input encoding (which only affects the main BAM's regenerated QUAL). The
        // raw passthrough round-trips through `samtools view -h` as +33.
        let scores: Vec<u8> = f[10].bytes().map(|b| b.wrapping_sub(33)).collect();
        *rec.quality_scores_mut() = QualityScores::from(scores);
    }
    // Optional tags, verbatim + in input order (Bowtie 2 emits `i`/`Z`; `A`/`f`
    // handled defensively).
    for tag in &f[11..] {
        insert_raw_tag(rec.data_mut(), tag)?;
    }
    Ok(rec)
}

/// Parse one `TAG:TYPE:VALUE` SAM tag into the noodles `Data` map, preserving
/// the type so `samtools view -h` renders it identically.
fn insert_raw_tag(data: &mut noodles_sam::alignment::record_buf::Data, field: &str) -> Result<()> {
    // splitn(3): a `Z` value may itself contain ':'.
    let parts: Vec<&str> = field.splitn(3, ':').collect();
    if parts.len() < 3 || parts[0].len() != 2 {
        return Err(AlignerError::Validation(format!(
            "malformed SAM tag in ambig line: {field}"
        )));
    }
    let key = Tag::from([parts[0].as_bytes()[0], parts[0].as_bytes()[1]]);
    let value =
        match parts[1] {
            "i" => Value::Int32(parts[2].parse::<i32>().map_err(|_| {
                AlignerError::Validation(format!("bad integer tag value: {field}"))
            })?),
            "Z" => Value::String(BString::from(parts[2].as_bytes())),
            "A" => Value::Character(parts[2].bytes().next().unwrap_or(b'?')),
            "f" => {
                Value::Float(parts[2].parse::<f32>().map_err(|_| {
                    AlignerError::Validation(format!("bad float tag value: {field}"))
                })?)
            }
            other => {
                return Err(AlignerError::Validation(format!(
                    "unsupported SAM tag type '{other}' in ambig line: {field}"
                )));
            }
        };
    data.insert(key, value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::Genome;
    use crate::merge::Counters;
    use crate::methylation::{extract_corresponding_genomic_sequence_single_end, methylation_call};

    // ---- revcomp / hemming -------------------------------------------------

    #[test]
    fn revcomp_both_cases_uppercase_complement() {
        assert_eq!(revcomp(b"ACGT"), b"ACGT");
        assert_eq!(revcomp(b"AAAA"), b"TTTT");
        assert_eq!(revcomp(b"acgt"), b"ACGT"); // lower-case → UPPER complement
        assert_eq!(revcomp(b"ACGTN"), b"NACGT"); // N unchanged
    }

    #[test]
    fn hemming_counts_inequalities() {
        assert_eq!(hemming_dist(b"ACGT", b"ACGT"), 0);
        assert_eq!(hemming_dist(b"ACGT", b"ATGT"), 1);
        assert_eq!(hemming_dist(b"ACGTX", b"ACGTA"), 1); // X padding mismatches
    }

    // ---- make_mismatch_string ----------------------------------------------

    #[test]
    fn md_clean_match() {
        assert_eq!(make_mismatch_string(b"ACGT", b"ACGT", "4M", b""), "MD:Z:4");
    }

    #[test]
    fn md_single_mismatch() {
        // pos2 ref C vs read T → "2C1"
        assert_eq!(
            make_mismatch_string(b"ACTT", b"ACCT", "4M", b""),
            "MD:Z:2C1"
        );
    }

    #[test]
    fn md_leading_and_adjacent_mismatch_zero_padding() {
        // mismatch at pos0 → "0A...", adjacent mismatches → "0X0Y"
        assert_eq!(make_mismatch_string(b"TC", b"AC", "2M", b""), "MD:Z:0A1");
        assert_eq!(make_mismatch_string(b"TT", b"AC", "2M", b""), "MD:Z:0A0C0");
    }

    #[test]
    fn md_insertion_padding_skipped() {
        // CIGAR 2M1I2M: ref has X at the insertion; X is ignored in the MD tag.
        // actual=ACXGT? no — actual is the read; ref_seq has X padding at insertion.
        // read "ACGGT" (5), ref "AC" + "X" + "GT" = "ACXGT"
        assert_eq!(
            make_mismatch_string(b"ACGGT", b"ACXGT", "2M1I2M", b""),
            "MD:Z:4"
        );
    }

    #[test]
    fn md_single_deletion() {
        // CIGAR 2M1D2M, no mismatches. read "ACGT" (4), ref "ACGT" (4),
        // md_sequence = "AC" + deleted "T" + "GT" = "ACTGT".
        // Expected MD: 2^T2
        assert_eq!(
            make_mismatch_string(b"ACGT", b"ACGT", "2M1D2M", b"ACTGT"),
            "MD:Z:2^T2"
        );
    }

    #[test]
    fn md_two_deletions() {
        // CIGAR 2M1D2M1D2M. read "ACGTAC" (6), ref "ACGTAC" (6).
        // md_sequence = M(2)"AC" + D"T" + M(2)"GT" + D"A" + M(2)"AC" = "ACTGTAAC".
        // Expected: 2^T2^A2
        assert_eq!(
            make_mismatch_string(b"ACGTAC", b"ACGTAC", "2M1D2M1D2M", b"ACTGTAAC"),
            "MD:Z:2^T2^A2"
        );
    }

    #[test]
    fn md_deletion_with_mismatch() {
        // CIGAR 2M1D2M, mismatch in the second M block.
        // read "ACTT" (4), ref "ACGT" (4) → mismatch at read pos2 (ref G).
        // md_sequence = "AC" + "X"(deleted) ... use a concrete deleted base "N".
        // md_sequence = M"AC" + D"N" + M"GT" = "ACNGT".
        // Part1 MD (no del): "2G1" (pos2 ref G mismatch). Then deletion at read-pos 2.
        // Expected: 2^N0G1
        assert_eq!(
            make_mismatch_string(b"ACTT", b"ACGT", "2M1D2M", b"ACNGT"),
            "MD:Z:2^N0G1"
        );
    }

    // ---- FLAG via single_end_sam_output ------------------------------------

    fn refid_of(names: &[&str]) -> HashMap<String, usize> {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.to_string(), i))
            .collect()
    }

    fn best(chr: &str, pos: u32, index: usize, cigar: &str) -> BestAlignment {
        BestAlignment {
            chromosome: chr.to_string(),
            position: pos,
            index,
            alignment_score: 0,
            alignment_score_second_best: None,
            md_tag: String::new(),
            cigar: cigar.to_string(),
            bowtie_sequence: String::new(),
            mapq: 40,
        }
    }

    fn ext_of(strand: u8, rc: Conversion, gc: Conversion, genomic: &[u8]) -> GenomicExtraction {
        GenomicExtraction {
            alignment_strand: strand,
            read_conversion: rc,
            genome_conversion: gc,
            unmodified_genomic_sequence: genomic.to_vec(),
            genomic_seq_for_md_tag: Vec::new(),
            end_position: 0,
            indels: 0,
            extracted: true,
        }
    }

    #[test]
    fn sam_output_plus_strand_index0() {
        // index 0 OT: strand +, CT/CT → FLAG 0, no revcomp.
        // read ACGT, genomic ACGT + "CG" padding (CT → drop last 2 → ref "ACGT").
        let b = best("chr1", 5, 0, "4M");
        let e = ext_of(b'+', Conversion::Ct, Conversion::Ct, b"ACGTCG");
        let r = single_end_sam_output(
            "r1",
            b"ACGT",
            b"IIII",
            &b,
            &e,
            b"....",
            &refid_of(&["chr1"]),
            false,
        )
        .unwrap();
        let inner = r.inner();
        assert_eq!(u16::from(inner.flags()), 0);
        assert_eq!(inner.sequence().as_ref(), b"ACGT"); // not reverse-complemented
        assert_eq!(usize::from(inner.alignment_start().unwrap()), 5);
    }

    #[test]
    fn sam_output_minus_strand_index1_reverses() {
        // index 1 OB: strand -, CT/GA → FLAG 16, SEQ/QUAL/XM reversed.
        let b = best("chr1", 5, 1, "4M");
        // genomic already revcomp'd by extraction; len 6 → CT drops last 2 → "GTAC".
        let e = ext_of(b'-', Conversion::Ct, Conversion::Ga, b"GTACGT");
        let r = single_end_sam_output(
            "r1",
            b"ACGT", // original read
            b"ABCD", // qual ascii
            &b,
            &e,
            b"zh..", // XM
            &refid_of(&["chr1"]),
            false,
        )
        .unwrap();
        let inner = r.inner();
        assert_eq!(u16::from(inner.flags()), 16);
        // SEQ reverse-complemented: revcomp("ACGT") = "ACGT"
        assert_eq!(inner.sequence().as_ref(), b"ACGT");
        // XM reversed: "zh.." → "..hz"
        let xm = bismark_io::tags::xm(inner.data()).unwrap();
        assert_eq!(xm, b"..hz");
    }

    // ---- header ------------------------------------------------------------

    fn genome_of(entries: &[(&str, &[u8])]) -> Genome {
        let mut chromosomes = HashMap::new();
        let mut sq_order = Vec::new();
        for (n, s) in entries {
            chromosomes.insert(n.to_string(), s.to_vec());
            sq_order.push(n.to_string());
        }
        Genome {
            chromosomes,
            sq_order,
        }
    }

    /// Serialise a noodles Header to SAM text (what `samtools view -H` renders).
    fn header_text(header: &Header) -> String {
        let mut buf = Vec::new();
        let mut w = noodles_sam::io::Writer::new(&mut buf);
        w.write_header(header).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn nm_includes_indels_d_only() {
        // NM = hemming_dist(actual, ref) + indels. With a clean match (hemming 0)
        // and ext.indels = 2 (D-only), NM must be 2 (Perl 8588–8590).
        let b = best("chr1", 5, 0, "4M");
        let mut e = ext_of(b'+', Conversion::Ct, Conversion::Ct, b"ACGTAC");
        e.indels = 2;
        let rec = single_end_sam_output(
            "r1",
            b"ACGT",
            b"FFFF",
            &b,
            &e,
            b"....",
            &refid_of(&["chr1"]),
            false,
        )
        .unwrap();
        let nm = rec.inner().data().get(&Tag::from(*b"NM")).unwrap();
        let nm_val = match nm {
            Value::Int8(n) => *n as i64,
            Value::UInt8(n) => *n as i64,
            Value::Int32(n) => *n as i64,
            other => panic!("NM not integer: {other:?}"),
        };
        assert_eq!(nm_val, 2); // hemming 0 + indels 2
    }

    #[test]
    fn minus_strand_index1_deletion_double_revcomp() {
        // §9 #16: an index-1 (OB, '-') read WITH a deletion — the only path that
        // composes the DOUBLE revcomp of genomic_seq_for_md_tag (extraction 4419 +
        // output 8581) with the MD deletion reconstitution. Built through the REAL
        // extraction; expected values hand-derived (and the MD-builder itself is
        // verified byte-identical to Perl by the dual code-review differential).
        //
        // chr1 = AAACCCGGGTTTACGT; index 1, pos 5, CIGAR 3M1D3M, read AACCGG.
        // → unmodified window "AACCGGGT" (8 = read 6 + 2); after the output revcomps
        //   actual == ref == "CCGGTT" (clean), so MD = "3^G3", NM = 1, FLAG = 16.
        let g = genome_of(&[("chr1", b"AAACCCGGGTTTACGT")]);
        let refid = build_refid(&g);
        let b = best("chr1", 5, 1, "3M1D3M");
        let mut c = Counters::default();
        let ext = extract_corresponding_genomic_sequence_single_end(&b, &g, false, &mut c).unwrap();
        assert!(ext.extracted);
        assert_eq!(ext.unmodified_genomic_sequence.len(), 6 + 2);
        assert_eq!(ext.indels, 1);
        assert_eq!(c.ct_ga_count, 1); // index 1 → CT/GA strand bucket

        let read = b"AACCGG";
        let methcall = methylation_call(
            read,
            &ext.unmodified_genomic_sequence,
            ext.read_conversion,
            &mut c,
        );
        let rec = single_end_sam_output("r1", read, b"FFFFFF", &b, &ext, &methcall, &refid, false)
            .unwrap();
        let inner = rec.inner();
        assert_eq!(u16::from(inner.flags()), 16); // OB → '-' → FLAG 16
        assert_eq!(inner.sequence().as_ref(), b"CCGGTT"); // revcomp(read)
        assert_eq!(
            inner.data().get(&Tag::from(*b"MD")).unwrap(),
            &Value::String(BString::from("3^G3"))
        );
        let nm = inner.data().get(&Tag::from(*b"NM")).unwrap();
        let nm_val = match nm {
            Value::Int8(n) => *n as i64,
            Value::UInt8(n) => *n as i64,
            Value::Int32(n) => *n as i64,
            other => panic!("NM not integer: {other:?}"),
        };
        assert_eq!(nm_val, 1); // hemming 0 + 1 deleted base
        assert_eq!(
            inner.data().get(&Tag::from(*b"XR")).unwrap(),
            &Value::String(BString::from("CT"))
        );
        assert_eq!(
            inner.data().get(&Tag::from(*b"XG")).unwrap(),
            &Value::String(BString::from("GA"))
        );
    }

    #[test]
    fn record_roundtrips_through_bam_tag_order_values_qual() {
        // Build a record, write a BAM via BamWriter, read it back via bismark-io
        // (noodles), and assert the encode→decode round-trip: tag ORDER, tag
        // values, FLAG/POS/MAPQ/CIGAR/SEQ/QUAL. (samtools `:i:` rendering is the
        // Phase-10 gate; this pins the noodles half hermetically.)
        let g = genome_of(&[("chr1", b"ACGTACGT")]);
        let header = generate_sam_header(&g, "--genome /g reads.fq");
        let refid = build_refid(&g);
        let b = best("chr1", 5, 0, "4M");
        // read+2 genomic window; CT drops last 2 → ref "ACGT" == read → MD 4, NM 0.
        let e = ext_of(b'+', Conversion::Ct, Conversion::Ct, b"ACGTAC");
        let rec =
            single_end_sam_output("r1", b"ACGT", b"FFFF", &b, &e, b"....", &refid, false).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("out.bam");
        let mut w = BamWriter::from_path(&path, header).unwrap();
        w.write_record(&rec).unwrap();
        w.finish().unwrap();

        let mut r = bismark_io::BamReader::from_path(&path).unwrap();
        let recs: Vec<_> = r.records().map(|x| x.unwrap()).collect();
        assert_eq!(recs.len(), 1);
        let got = recs[0].inner();

        assert_eq!(u16::from(got.flags()), 0);
        assert_eq!(usize::from(got.alignment_start().unwrap()), 5);
        assert_eq!(u8::from(got.mapping_quality().unwrap()), 40);
        assert_eq!(got.sequence().as_ref(), b"ACGT");
        // QUAL: 'F' (70) → phred 37.
        assert_eq!(got.quality_scores().as_ref(), &[37u8; 4]);

        // Tag ORDER preserved through BAM encode→decode.
        let tags: Vec<[u8; 2]> = got.data().keys().map(<[u8; 2]>::from).collect();
        assert_eq!(
            tags,
            vec![*b"NM", *b"MD", *b"XM", *b"XR", *b"XG"],
            "tag order must match Perl (NM MD XM XR XG)"
        );

        // Tag VALUES.
        let nm = got.data().get(&Tag::from(*b"NM")).unwrap();
        let nm_val = match nm {
            Value::Int8(n) => *n as i64,
            Value::UInt8(n) => *n as i64,
            Value::Int16(n) => *n as i64,
            Value::UInt16(n) => *n as i64,
            Value::Int32(n) => *n as i64,
            Value::UInt32(n) => *n as i64,
            other => panic!("NM not an integer: {other:?}"),
        };
        assert_eq!(nm_val, 0);
        assert_eq!(
            got.data().get(&Tag::from(*b"MD")).unwrap(),
            &Value::String(BString::from("4"))
        );
        assert_eq!(
            got.data().get(&Tag::from(*b"XM")).unwrap(),
            &Value::String(BString::from("...."))
        );
        assert_eq!(
            got.data().get(&Tag::from(*b"XR")).unwrap(),
            &Value::String(BString::from("CT"))
        );
        assert_eq!(
            got.data().get(&Tag::from(*b"XG")).unwrap(),
            &Value::String(BString::from("CT"))
        );
    }

    #[test]
    fn header_hd_sq_pg_exact_bytes() {
        let g = genome_of(&[("chr1", b"ACGTACGT"), ("chr2", b"ACG")]);
        let header = generate_sam_header(&g, "--genome /g reads.fq");
        let text = header_text(&header);
        let expected = "@HD\tVN:1.0\tSO:unsorted\n\
                        @SQ\tSN:chr1\tLN:8\n\
                        @SQ\tSN:chr2\tLN:3\n\
                        @PG\tID:Bismark\tVN:v0.25.1\tCL:\"bismark --genome /g reads.fq\"\n";
        assert_eq!(text, expected);
    }

    // ---- --ambig_bam raw record --------------------------------------------

    #[test]
    fn build_raw_ambig_record_deconverts_and_preserves_tag_order() {
        let g = genome_of(&[("chr1", b"ACGTACGT"), ("chr2", b"ACG")]);
        let refid = build_refid(&g);
        // raw Bowtie 2 SE line: RNAME has the _CT_converted suffix; AS/XS/XN
        // i-tags + MD:Z, in Bowtie 2's order.
        let raw = "r1\t16\tchr2_CT_converted\t2\t1\t8M\t*\t0\t0\tACGTACGT\tIIIIIIII\tAS:i:-6\tXS:i:-12\tXN:i:0\tMD:Z:3A4";
        let rec = build_raw_record(raw, &refid).unwrap();
        assert_eq!(rec.name().map(|n| n.to_vec()), Some(b"r1".to_vec()));
        assert_eq!(u16::from(rec.flags()), 16);
        assert_eq!(rec.reference_sequence_id(), Some(1)); // chr2 de-converted → tid 1
        assert_eq!(usize::from(rec.alignment_start().unwrap()), 2);
        assert_eq!(u8::from(rec.mapping_quality().unwrap()), 1);
        assert_eq!(rec.sequence().as_ref(), b"ACGTACGT");
        // tag ORDER preserved verbatim
        let tags: Vec<[u8; 2]> = rec.data().keys().map(<[u8; 2]>::from).collect();
        assert_eq!(tags, vec![*b"AS", *b"XS", *b"XN", *b"MD"]);
        assert_eq!(rec.data().get(&Tag::from(*b"AS")), Some(&Value::Int32(-6)));
        assert_eq!(
            rec.data().get(&Tag::from(*b"MD")),
            Some(&Value::String(BString::from("3A4")))
        );
    }

    #[test]
    fn build_raw_ambig_record_rejects_unsupported_tag_type() {
        let refid = build_refid(&genome_of(&[("chr1", b"AC")]));
        // a `B` (array) tag type is not produced by Bowtie 2 SE and is unsupported.
        let raw = "r\t0\tchr1_CT_converted\t1\t0\t2M\t*\t0\t0\tAC\tII\tZZ:B:i,1,2";
        assert!(build_raw_record(raw, &refid).is_err());
    }
}
