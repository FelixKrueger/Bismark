//! Genomic-sequence extraction + the `XM` methylation call — a port of Perl
//! `extract_corresponding_genomic_sequence_single_end` (4273–4467) and
//! `methylation_call` (4800–5018), plus `reverse_complement` (5161).
//!
//! For one `UniqueBest` alignment, pull the genomic window (read length + 2
//! context bases) matching the reported CIGAR, handling the chromosome-edge
//! guards and the per-strand counters; then call the per-base methylation state
//! (`Z/z X/x H/h U/u .`) by comparing the original read to that window. The
//! result feeds `output::single_end_sam_output` (Phase 5) and the Phase-6 report.

use crate::error::{AlignerError, Result};
use crate::genome::Genome;
use crate::merge::{BestAlignment, BestAlignmentPaired, Counters};

/// Read/genome bisulfite conversion direction (→ `XR:Z`/`XG:Z` tag values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conversion {
    /// `CT`
    Ct,
    /// `GA`
    Ga,
}

impl Conversion {
    /// The tag string (`"CT"`/`"GA"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Conversion::Ct => "CT",
            Conversion::Ga => "GA",
        }
    }
}

/// The genomic window + strand/conversion derived for one alignment.
pub struct GenomicExtraction {
    /// `b'+'` or `b'-'` (Perl `$alignment_strand`).
    pub alignment_strand: u8,
    /// Read conversion (`XR`).
    pub read_conversion: Conversion,
    /// Genome conversion (`XG`).
    pub genome_conversion: Conversion,
    /// The extracted genomic sequence (read length + 2 context bases when not
    /// edge-truncated), already reverse-complemented for index 1/2.
    pub unmodified_genomic_sequence: Vec<u8>,
    /// Genomic sequence for the `MD:Z` builder — populated only when the CIGAR
    /// contains a deletion (else empty).
    pub genomic_seq_for_md_tag: Vec<u8>,
    /// 1-based-equivalent end position after the CIGAR walk (Perl `$pos`).
    pub end_position: u32,
    /// Number of deleted bases (`D` ops only — feeds `NM`).
    pub indels: u32,
    /// DOC-ONLY: `false` when a chromosome-edge guard fired. The driver gates
    /// on the `len == read_len + 2` LENGTH check (Perl 3127), not this flag.
    pub extracted: bool,
}

/// `reverse_complement` (Perl 5161): `tr/CATG/GTAC/` then `reverse`. Only
/// upper-case `CATG` are complemented; every other byte (incl. `N`, `X`,
/// lower-case) is left unchanged. (Distinct from `output::revcomp`, 9228.)
pub fn reverse_complement(seq: &[u8]) -> Vec<u8> {
    seq.iter()
        .rev()
        .map(|&b| match b {
            b'C' => b'G',
            b'A' => b'T',
            b'T' => b'A',
            b'G' => b'C',
            other => other,
        })
        .collect()
}

/// Parse a CIGAR into `(length, op)` runs (Perl 4303–4306: `split /\D+/` for the
/// lengths, `split /\d+/` for the ops). Errors on a malformed string.
pub(crate) fn parse_cigar(cigar: &str) -> Result<Vec<(u32, u8)>> {
    let mut runs = Vec::new();
    let bytes = cigar.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start || i == bytes.len() {
            return Err(AlignerError::Validation(format!(
                "CIGAR string contained a non-matching number of lengths and operations: {cigar}"
            )));
        }
        let len: u32 = cigar[start..i]
            .parse()
            .map_err(|_| AlignerError::Validation(format!("bad CIGAR length in {cigar}")))?;
        let op = bytes[i];
        i += 1;
        runs.push((len, op));
    }
    Ok(runs)
}

/// Extract the genomic window for one SE alignment (Perl 4273–4467).
///
/// Increments the per-strand counter (`CT_CT`/… in `counters`) ONLY when no
/// chromosome-edge guard fired (Perl 4402/4411/4426/4441, behind the 4317/4390
/// guards). `pbat` adds the +2 index modifier (4308–4312); for the SE-directional
/// spine it is `false` and `best.index` is 0 or 1.
pub fn extract_corresponding_genomic_sequence_single_end(
    best: &BestAlignment,
    genome: &Genome,
    pbat: bool,
    counters: &mut Counters,
) -> Result<GenomicExtraction> {
    let chr = genome.get(&best.chromosome).ok_or_else(|| {
        AlignerError::Validation(format!(
            "Chromosome {} not found in the genome",
            best.chromosome
        ))
    })?;

    let contains_deletion = best.cigar.contains('D');
    let runs = parse_cigar(&best.cigar)?;
    let pbat_mod: usize = if pbat { 2 } else { 0 };
    let eff = best.index + pbat_mod;

    // 1-based POS → 0-based offset (Perl 4300).
    let mut pos: usize = (best.position - 1) as usize;
    let mut non_bis: Vec<u8> = Vec::new();
    let mut md_seq: Vec<u8> = Vec::new();
    let mut indels: u32 = 0;

    // Strand/conversion is a pure function of the (effective) index; the only
    // thing the edge guard gates is the per-strand COUNTER (and writing).
    let (alignment_strand, read_conversion, genome_conversion) = match eff {
        0 => (b'+', Conversion::Ct, Conversion::Ct),
        1 => (b'-', Conversion::Ct, Conversion::Ga),
        2 => (b'-', Conversion::Ga, Conversion::Ct),
        3 => (b'+', Conversion::Ga, Conversion::Ga),
        _ => {
            return Err(AlignerError::Validation(
                "Too many Bowtie 2 result filehandles".into(),
            ));
        }
    };

    let edge = |non_bis: Vec<u8>, md_seq: Vec<u8>, pos: usize, indels: u32| GenomicExtraction {
        alignment_strand,
        read_conversion,
        genome_conversion,
        unmodified_genomic_sequence: non_bis,
        genomic_seq_for_md_tag: md_seq,
        end_position: pos as u32,
        indels,
        extracted: false,
    };

    // Index 1/3: prepend 2 genomic bases (Perl 4314–4323).
    if eff == 1 || eff == 3 {
        if pos < 2 {
            return Ok(edge(non_bis, md_seq, pos, indels)); // chromosome-edge guard
        }
        non_bis.extend_from_slice(&chr[pos - 2..pos]);
        // NB: the +2 prepend is NOT added to `genomic_seq_for_md_tag` (Perl 4322).
    }

    // CIGAR walk (Perl 4327–4385).
    for (len, op) in &runs {
        let len = *len as usize;
        match op {
            b'M' => {
                non_bis.extend_from_slice(&chr[pos..(pos + len).min(chr.len())]);
                if contains_deletion {
                    md_seq.extend_from_slice(&chr[pos..(pos + len).min(chr.len())]);
                }
                pos += len;
            }
            b'I' | b'S' => {
                // padding Xs (not used for the call; ignored in MD/hemming)
                non_bis.extend(std::iter::repeat_n(b'X', len));
                if contains_deletion {
                    md_seq.extend(std::iter::repeat_n(b'X', len));
                }
                // no pos change; no indels (Perl 4346/4360)
            }
            b'D' => {
                if contains_deletion {
                    md_seq.extend_from_slice(&chr[pos..(pos + len).min(chr.len())]);
                }
                pos += len;
                indels += len as u32; // D only (Perl 4370)
            }
            b'N' => {
                pos += len; // no indels (Perl 4376)
            }
            _ => {
                return Err(AlignerError::Validation(format!(
                    "The CIGAR string contained illegal CIGAR operations in addition to 'M', 'I', 'D', 'S' or 'N': {}",
                    best.cigar
                )));
            }
        }
    }

    // Index 0/2: append 2 genomic bases (Perl 4387–4397).
    if eff == 0 || eff == 2 {
        if chr.len() < pos + 2 {
            return Ok(edge(non_bis, md_seq, pos, indels)); // chromosome-edge guard
        }
        non_bis.extend_from_slice(&chr[pos..pos + 2]);
    }

    // Past both guards → bump the per-strand counter (Perl 4402/4411/4426/4441).
    match eff {
        0 => counters.ct_ct_count += 1,
        1 => counters.ct_ga_count += 1,
        2 => counters.ga_ct_count += 1,
        3 => counters.ga_ga_count += 1,
        _ => unreachable!(),
    }

    // Reverse-complement for index 1/2 (Perl 4417/4432).
    if eff == 1 || eff == 2 {
        non_bis = reverse_complement(&non_bis);
        if contains_deletion {
            md_seq = reverse_complement(&md_seq);
        }
    }

    Ok(GenomicExtraction {
        alignment_strand,
        read_conversion,
        genome_conversion,
        unmodified_genomic_sequence: non_bis,
        genomic_seq_for_md_tag: md_seq,
        end_position: pos as u32,
        indels,
        extracted: true,
    })
}

// ===========================================================================
// Paired-end genomic-seq extraction (Phase 7) — Perl
// `extract_corresponding_genomic_sequence_paired_end` (4471–4794).
//
// Each mate is extracted INDEPENDENTLY from its own POS+CIGAR (there is NO
// fragment span). The +2 context placement (5′ for index 1/3, 3′ for index 0/2)
// is keyed on the COMBINED PE index, same for both mates. Exactly one mate (the
// `-`-strand one) is reverse-complemented by the index dispatch. The four
// chromosome-edge guards `return` early; the caller gates per mate on
// `len == read_len + 2`, so the failing mate's short sequence localises the miss.
// ===========================================================================

/// The two-mate genomic windows + strand/conversion (≈ the PE `methylation_call_params`
/// fields set in 4779–4793). On a chromosome-edge miss the failing mate's
/// `unmodified_genomic_sequence_*` is left SHORT while the other keeps its full
/// `read_len+2`; the caller's per-mate length check is the could-not-extract gate.
pub struct GenomicExtractionPaired {
    /// Read-1 alignment strand (`b'+'`/`b'-'`).
    pub alignment_read_1: u8,
    /// Read-2 alignment strand (`b'+'`/`b'-'`).
    pub alignment_read_2: u8,
    /// Read-1 conversion (`XR` of mate 1).
    pub read_conversion_1: Conversion,
    /// Read-2 conversion (`XR` of mate 2).
    pub read_conversion_2: Conversion,
    /// Genome conversion (`XG`, shared).
    pub genome_conversion: Conversion,
    /// Read-1 genomic window (read_len + 2 when not edge-truncated; revcomp'd for index 2/3).
    pub unmodified_genomic_sequence_1: Vec<u8>,
    /// Read-2 genomic window (revcomp'd for index 0/1).
    pub unmodified_genomic_sequence_2: Vec<u8>,
    /// Read-1 MD-tag genomic seq (only when CIGAR-1 has a deletion).
    pub genomic_seq_for_md_tag_1: Vec<u8>,
    /// Read-2 MD-tag genomic seq (only when CIGAR-2 has a deletion).
    pub genomic_seq_for_md_tag_2: Vec<u8>,
    /// Read-1 walked end position (Perl `$pos_1`; for TLEN).
    pub end_position_1: u32,
    /// Read-2 walked end position (Perl `$pos_2`; for TLEN).
    pub end_position_2: u32,
    /// Read-1 deleted bases (`D` only — feeds NM).
    pub indels_1: u32,
    /// Read-2 deleted bases (`D` only — feeds NM).
    pub indels_2: u32,
}

/// Per-mate CIGAR walk + index-driven +2 placement. `Edge` ⇒ a chromosome-edge
/// guard fired (the partial sequence is shorter than `read_len + 2`).
enum MateWalk {
    Complete {
        non_bis: Vec<u8>,
        md_seq: Vec<u8>,
        end_pos: u32,
        indels: u32,
    },
    Edge {
        non_bis: Vec<u8>,
        md_seq: Vec<u8>,
        end_pos: u32,
        indels: u32,
    },
}

/// Walk one mate (Perl 4530–4614 for mate 1, 4617–4702 for mate 2). `prepend_5p`
/// adds the 2 context bases at the 5′ end (index 1/3); `append_3p` at the 3′ end
/// (index 0/2). 🔴 `strict_5p` selects mate 1's strict `(pos-2) > 0` guard (Perl
/// 4535) vs mate 2's `(pos-2) >= 0` (4622); the SE port uses `>= 0`, so mate 1
/// must NOT reuse it. The +2 bases are NOT added to `md_seq` (Perl 4540/4613).
fn walk_mate(
    chr: &[u8],
    position_1based: u32,
    cigar: &str,
    contains_deletion: bool,
    prepend_5p: bool,
    append_3p: bool,
    strict_5p: bool,
) -> Result<MateWalk> {
    let runs = parse_cigar(cigar)?;
    let mut pos: usize = (position_1based - 1) as usize; // 1-based → 0-based (Perl 4513)
    let mut non_bis: Vec<u8> = Vec::new();
    let mut md_seq: Vec<u8> = Vec::new();
    let mut indels: u32 = 0;

    // 5′ prepend (index 1/3).
    if prepend_5p {
        let ok = if strict_5p {
            (pos as i64) - 2 > 0 // mate 1: pos >= 3 (Perl 4535)
        } else {
            (pos as i64) - 2 >= 0 // mate 2: pos >= 2 (Perl 4622)
        };
        if !ok {
            return Ok(MateWalk::Edge {
                non_bis,
                md_seq,
                end_pos: pos as u32,
                indels,
            });
        }
        non_bis.extend_from_slice(&chr[pos - 2..pos]);
    }

    // CIGAR walk (Perl 4543–4603 / 4631–4688).
    for (len, op) in &runs {
        let len = *len as usize;
        match op {
            b'M' => {
                non_bis.extend_from_slice(&chr[pos..(pos + len).min(chr.len())]);
                if contains_deletion {
                    md_seq.extend_from_slice(&chr[pos..(pos + len).min(chr.len())]);
                }
                pos += len;
            }
            b'I' | b'S' => {
                non_bis.extend(std::iter::repeat_n(b'X', len));
                if contains_deletion {
                    md_seq.extend(std::iter::repeat_n(b'X', len));
                }
            }
            b'D' => {
                if contains_deletion {
                    md_seq.extend_from_slice(&chr[pos..(pos + len).min(chr.len())]);
                }
                pos += len;
                indels += len as u32;
            }
            b'N' => {
                pos += len;
            }
            _ => {
                return Err(AlignerError::Validation(format!(
                    "The CIGAR string contained illegal CIGAR operations in addition to 'M', 'I', 'D', 'S' or 'N': {cigar}"
                )));
            }
        }
    }

    // 3′ append (index 0/2).
    if append_3p {
        if chr.len() < pos + 2 {
            return Ok(MateWalk::Edge {
                non_bis,
                md_seq,
                end_pos: pos as u32,
                indels,
            });
        }
        non_bis.extend_from_slice(&chr[pos..pos + 2]);
    }

    Ok(MateWalk::Complete {
        non_bis,
        md_seq,
        end_pos: pos as u32,
        indels,
    })
}

/// Extract the two genomic windows for one PE alignment (Perl 4471–4794).
/// Walks mate 1 then mate 2 sequentially; a mate's edge guard short-circuits with
/// that mate's sequence left SHORT (matching Perl's bare `return`). The per-strand
/// counter is bumped — and the `-` mate reverse-complemented — ONLY past all four
/// guards (Perl 4708–4775). The caller gates per mate on `len == read_len + 2`.
pub fn extract_corresponding_genomic_sequence_paired_end(
    best: &BestAlignmentPaired,
    genome: &Genome,
    counters: &mut Counters,
) -> Result<GenomicExtractionPaired> {
    let chr = genome.get(&best.chromosome).ok_or_else(|| {
        AlignerError::Validation(format!(
            "Chromosome {} not found in the genome",
            best.chromosome
        ))
    })?;
    let index = best.index;

    // Strand/conversion/revcomp-target are a pure function of the index (Perl
    // 4708–4772) — computed up-front (as SE does) so the early-return struct can
    // carry them; only the COUNTER + revcomp are gated on passing all guards.
    let (
        alignment_read_1,
        alignment_read_2,
        read_conversion_1,
        read_conversion_2,
        genome_conversion,
    ) = match index {
        0 => (b'+', b'-', Conversion::Ct, Conversion::Ga, Conversion::Ct),
        1 => (b'+', b'-', Conversion::Ga, Conversion::Ct, Conversion::Ga),
        2 => (b'-', b'+', Conversion::Ga, Conversion::Ct, Conversion::Ct),
        3 => (b'-', b'+', Conversion::Ct, Conversion::Ga, Conversion::Ga),
        _ => {
            return Err(AlignerError::Validation(
                "Too many bowtie result filehandles".into(),
            ));
        }
    };
    let prepend_5p = index == 1 || index == 3;
    let append_3p = index == 0 || index == 2;
    let contains_deletion_1 = best.cigar_1.contains('D');
    let contains_deletion_2 = best.cigar_2.contains('D');

    let mk = |nb1: Vec<u8>,
              md1: Vec<u8>,
              ep1: u32,
              in1: u32,
              nb2: Vec<u8>,
              md2: Vec<u8>,
              ep2: u32,
              in2: u32| {
        GenomicExtractionPaired {
            alignment_read_1,
            alignment_read_2,
            read_conversion_1,
            read_conversion_2,
            genome_conversion,
            unmodified_genomic_sequence_1: nb1,
            unmodified_genomic_sequence_2: nb2,
            genomic_seq_for_md_tag_1: md1,
            genomic_seq_for_md_tag_2: md2,
            end_position_1: ep1,
            end_position_2: ep2,
            indels_1: in1,
            indels_2: in2,
        }
    };

    // ---- mate 1 (strict 5′ guard) -----------------------------------------
    let mate1 = walk_mate(
        chr,
        best.position_1,
        &best.cigar_1,
        contains_deletion_1,
        prepend_5p,
        append_3p,
        true,
    )?;
    let (mut nb1, mut md1, ep1, in1) = match mate1 {
        // mate 1 edge → mate 2 is never walked (empty); R1 length check fails first.
        MateWalk::Edge {
            non_bis,
            md_seq,
            end_pos,
            indels,
        } => {
            return Ok(mk(
                non_bis,
                md_seq,
                end_pos,
                indels,
                Vec::new(),
                Vec::new(),
                0,
                0,
            ));
        }
        MateWalk::Complete {
            non_bis,
            md_seq,
            end_pos,
            indels,
        } => (non_bis, md_seq, end_pos, indels),
    };

    // ---- mate 2 (non-strict 5′ guard) -------------------------------------
    let mate2 = walk_mate(
        chr,
        best.position_2,
        &best.cigar_2,
        contains_deletion_2,
        prepend_5p,
        append_3p,
        false,
    )?;
    let (mut nb2, mut md2, ep2, in2) = match mate2 {
        // mate 2 edge → R1 stays FULL (passes), R2 short (fails). One count.
        MateWalk::Edge {
            non_bis,
            md_seq,
            end_pos,
            indels,
        } => {
            return Ok(mk(nb1, md1, ep1, in1, non_bis, md_seq, end_pos, indels));
        }
        MateWalk::Complete {
            non_bis,
            md_seq,
            end_pos,
            indels,
        } => (non_bis, md_seq, end_pos, indels),
    };

    // ---- past all four guards: counter + revcomp the `-` mate (4708–4775) --
    match index {
        0 => counters.ct_ga_ct_count += 1,
        1 => counters.ga_ct_ga_count += 1,
        2 => counters.ga_ct_ct_count += 1,
        3 => counters.ct_ga_ga_count += 1,
        _ => unreachable!(),
    }
    // index 0/1 → mate 2 is the `-` hit; index 2/3 → mate 1 is the `-` hit.
    if index == 0 || index == 1 {
        nb2 = reverse_complement(&nb2);
        if contains_deletion_2 {
            md2 = reverse_complement(&md2);
        }
    } else {
        nb1 = reverse_complement(&nb1);
        if contains_deletion_1 {
            md1 = reverse_complement(&md1);
        }
    }

    Ok(mk(nb1, md1, ep1, in1, nb2, md2, ep2, in2))
}

/// The per-base methylation call (Perl `methylation_call`, 4800–5018).
///
/// Compares the original (upper-case) read `seq` to the genomic window `genomic`
/// (read length + 2). Returns the `XM` match string and accumulates the 8
/// methylation-context counters. `read_conversion` selects the CT branch
/// (4832–4912; the SE-directional spine) or the GA branch (4913–4998; non-dir/
/// pbat — ported for Phase 8, inert here). Context look-ups past the window end
/// behave as Perl's out-of-range access (a non-`G`/non-`C`/non-`N`/`X` sentinel).
pub fn methylation_call(
    seq: &[u8],
    genomic: &[u8],
    read_conversion: Conversion,
    counters: &mut Counters,
) -> Vec<u8> {
    let mut call = Vec::with_capacity(seq.len());
    // sentinel for out-of-range context (Perl empty substr: not G/C/N/X)
    let at = |idx: usize| -> u8 { genomic.get(idx).copied().unwrap_or(0) };

    match read_conversion {
        Conversion::Ct => {
            for (i, &base) in seq.iter().enumerate() {
                let g = at(i);
                if base == g {
                    if g == b'C' {
                        push_ct_context(at(i + 1), at(i + 2), true, &mut call, counters);
                    } else {
                        call.push(b'.');
                    }
                } else if g == b'C' && base == b'T' {
                    push_ct_context(at(i + 1), at(i + 2), false, &mut call, counters);
                } else {
                    call.push(b'.');
                }
            }
        }
        Conversion::Ga => {
            // GA branch (Perl 4916–4998): compares seq[i] to genomic[i+2];
            // context bases look UPSTREAM (i+1, then i).
            for (i, &base) in seq.iter().enumerate() {
                let g = at(i + 2);
                if base == g {
                    if g == b'G' {
                        push_ga_context(at(i + 1), at(i), true, &mut call, counters);
                    } else {
                        call.push(b'.');
                    }
                } else if g == b'G' && base == b'A' {
                    push_ga_context(at(i + 1), at(i), false, &mut call, counters);
                } else {
                    call.push(b'.');
                }
            }
        }
    }
    call
}

/// CT-branch context classification (Perl 4836–4901). `methylated` = the C was
/// protected (read base == genomic C); else it was converted (read `T`).
fn push_ct_context(
    downstream: u8,
    second_downstream: u8,
    methylated: bool,
    call: &mut Vec<u8>,
    counters: &mut Counters,
) {
    let (cpg, chg, chh, unknown) = if methylated {
        (b'Z', b'X', b'H', b'U')
    } else {
        (b'z', b'x', b'h', b'u')
    };
    if downstream == b'G' {
        bump(counters, cpg, methylated);
        call.push(cpg);
    } else if downstream == b'N' || downstream == b'X' {
        bump(counters, unknown, methylated);
        call.push(unknown);
    } else if second_downstream == b'G' {
        bump(counters, chg, methylated);
        call.push(chg);
    } else if second_downstream == b'N' || second_downstream == b'X' {
        bump(counters, unknown, methylated);
        call.push(unknown);
    } else {
        bump(counters, chh, methylated);
        call.push(chh);
    }
}

/// GA-branch context classification (Perl 4919–4988). The protected/converted C
/// is on the opposing strand; context bases look upstream.
fn push_ga_context(
    upstream: u8,
    second_upstream: u8,
    methylated: bool,
    call: &mut Vec<u8>,
    counters: &mut Counters,
) {
    let (cpg, chg, chh, unknown) = if methylated {
        (b'Z', b'X', b'H', b'U')
    } else {
        (b'z', b'x', b'h', b'u')
    };
    if upstream == b'C' {
        bump(counters, cpg, methylated);
        call.push(cpg);
    } else if upstream == b'N' || upstream == b'X' {
        bump(counters, unknown, methylated);
        call.push(unknown);
    } else if second_upstream == b'C' {
        bump(counters, chg, methylated);
        call.push(chg);
    } else if second_upstream == b'N' || second_upstream == b'X' {
        bump(counters, unknown, methylated);
        call.push(unknown);
    } else {
        bump(counters, chh, methylated);
        call.push(chh);
    }
}

/// Accumulate the 8 context counters (Perl 5006–5013), keyed on the call char.
fn bump(counters: &mut Counters, call_char: u8, _methylated: bool) {
    match call_char {
        b'Z' => counters.total_me_cpg += 1,
        b'X' => counters.total_me_chg += 1,
        b'H' => counters.total_me_chh += 1,
        b'U' => counters.total_me_c_unknown += 1,
        b'z' => counters.total_unme_cpg += 1,
        b'x' => counters.total_unme_chg += 1,
        b'h' => counters.total_unme_chh += 1,
        b'u' => counters.total_unme_c_unknown += 1,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn genome_of(chr: &str, seq: &[u8]) -> Genome {
        let mut chromosomes = std::collections::HashMap::new();
        chromosomes.insert(chr.to_string(), seq.to_vec());
        Genome {
            chromosomes,
            sq_order: vec![chr.to_string()],
        }
    }

    #[test]
    fn revcomp_complements_and_reverses_uppercase_only() {
        assert_eq!(reverse_complement(b"ACGT"), b"ACGT"); // palindrome
        assert_eq!(reverse_complement(b"AAAA"), b"TTTT");
        assert_eq!(reverse_complement(b"ACGTN"), b"NACGT"); // N unchanged, reversed
        assert_eq!(reverse_complement(b"CCGG"), b"CCGG");
    }

    #[test]
    fn parse_cigar_basic() {
        assert_eq!(parse_cigar("10M").unwrap(), vec![(10, b'M')]);
        assert_eq!(
            parse_cigar("5M2D3M").unwrap(),
            vec![(5, b'M'), (2, b'D'), (3, b'M')]
        );
        assert!(parse_cigar("10").is_err()); // trailing number, no op
        assert!(parse_cigar("M10").is_err()); // op before number
    }

    #[test]
    fn extract_index0_appends_two_and_counts_ct_ct() {
        // read 4 bp at pos 3 (1-based) on chr; +2 appended at the 3' end.
        let g = genome_of("chr1", b"AAACGTACGTAA"); // 12 bp
        let b = best("chr1", 3, 0, "4M");
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_single_end(&b, &g, false, &mut c).unwrap();
        assert!(e.extracted);
        assert_eq!(e.alignment_strand, b'+');
        assert_eq!(e.read_conversion, Conversion::Ct);
        // pos0 = 2; M(4) = chr[2..6] = "ACGT", +2 = chr[6..8] = "AC"
        assert_eq!(e.unmodified_genomic_sequence, b"ACGTAC");
        assert_eq!(e.unmodified_genomic_sequence.len(), 4 + 2);
        assert_eq!(c.ct_ct_count, 1);
        assert_eq!(c.ct_ga_count, 0);
    }

    #[test]
    fn extract_index1_prepends_two_revcomps_and_counts_ct_ga() {
        let g = genome_of("chr1", b"AAACGTACGTAA");
        let b = best("chr1", 5, 1, "4M"); // pos0 = 4
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_single_end(&b, &g, false, &mut c).unwrap();
        assert!(e.extracted);
        assert_eq!(e.alignment_strand, b'-');
        assert_eq!(e.read_conversion, Conversion::Ct);
        assert_eq!(e.genome_conversion, Conversion::Ga);
        // prepend chr[2..4]="AC", then M(4)=chr[4..8]="GTAC" → "ACGTAC", then revcomp
        assert_eq!(e.unmodified_genomic_sequence, reverse_complement(b"ACGTAC"));
        assert_eq!(c.ct_ga_count, 1);
        assert_eq!(c.ct_ct_count, 0);
    }

    #[test]
    fn extract_index0_edge_at_three_prime_returns_short_no_counter() {
        // read ends within 2 bp of the chromosome end → append guard fires.
        let g = genome_of("chr1", b"AAACGT"); // 6 bp
        let b = best("chr1", 3, 0, "4M"); // pos0=2, M ends at 6, need pos+2=8 > 6
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_single_end(&b, &g, false, &mut c).unwrap();
        assert!(!e.extracted);
        assert_ne!(e.unmodified_genomic_sequence.len(), 4 + 2); // length guard will skip
        assert_eq!(c.ct_ct_count, 0); // NO strand counter on the edge
    }

    #[test]
    fn extract_index1_edge_at_five_prime_returns_short_no_counter() {
        let g = genome_of("chr1", b"ACGTACGT");
        let b = best("chr1", 1, 1, "4M"); // pos0=0 → pos-2 < 0 → prepend guard fires
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_single_end(&b, &g, false, &mut c).unwrap();
        assert!(!e.extracted);
        assert_eq!(c.ct_ga_count, 0);
    }

    #[test]
    fn extract_deletion_builds_md_seq_and_indels() {
        // CIGAR 2M1D2M at pos 1 (index 0): non_bis has no D bases, md_seq does.
        let g = genome_of("chr1", b"ACGTACGTAC"); // 10 bp
        let b = best("chr1", 1, 0, "2M1D2M");
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_single_end(&b, &g, false, &mut c).unwrap();
        // non_bis: M(2)=chr[0..2]="AC", D skips, M(2)=chr[3..5]="TA", +2=chr[5..7]="CG"
        assert_eq!(e.unmodified_genomic_sequence, b"ACTACG");
        // md_seq includes the deleted base: "AC" + chr[2..3]="G" + "TA" = "ACGTA"
        assert_eq!(e.genomic_seq_for_md_tag, b"ACGTA");
        assert_eq!(e.indels, 1);
    }

    #[test]
    fn extract_combined_deletion_insertion_indels_counts_d_only() {
        // CIGAR 2M2D2I2M: `indels` must count the 2 D bases, NOT the 2 I bases
        // (Perl 4370 vs 4346) — feeds NM = hemming + indels.
        let g = genome_of("chr1", b"ACGTACGTACGT");
        let b = best("chr1", 1, 0, "2M2D2I2M");
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_single_end(&b, &g, false, &mut c).unwrap();
        assert_eq!(e.indels, 2); // D(2) only; I(2) excluded
        // read consumes M(2)+I(2)+M(2)=6; window = 6+2.
        assert_eq!(e.unmodified_genomic_sequence.len(), 6 + 2);
    }

    // ---- spliced `N`-CIGAR extraction (HISAT2; Phase 2a; V6) ---------------

    /// V6: a spliced read (`N` op) skips the intron — `pos` advances by the
    /// intron length, NO genomic bases are appended, and the M-flanks are
    /// concatenated (Perl 4376). `N` is not an indel.
    #[test]
    fn extract_spliced_n_skips_intron_index0() {
        let g = genome_of("chr1", b"ACGTTTTTACGTACG"); // 15 bp
        let b = best("chr1", 1, 0, "3M4N3M"); // pos0=0
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_single_end(&b, &g, false, &mut c).unwrap();
        assert!(e.extracted);
        // M(3)=chr[0..3]="ACG", N(4) skips chr[3..7], M(3)=chr[7..10]="TAC", +2=chr[10..12]="GT"
        assert_eq!(e.unmodified_genomic_sequence, b"ACGTACGT");
        assert_eq!(e.indels, 0); // N is NOT an indel
        assert_eq!(c.ct_ct_count, 1);
    }

    /// V6: multiple introns (`M…N…M…N…M`) — every intron is skipped, all M-flanks
    /// concatenated.
    #[test]
    fn extract_multi_n_spliced_index0() {
        let g = genome_of("chr1", b"ACGTTTTTACGTACG");
        let b = best("chr1", 1, 0, "2M2N2M2N2M"); // pos0=0
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_single_end(&b, &g, false, &mut c).unwrap();
        assert!(e.extracted);
        // chr[0..2]="AC", chr[4..6]="TT", chr[8..10]="AC", +2=chr[10..12]="GT"
        assert_eq!(e.unmodified_genomic_sequence, b"ACTTACGT");
        assert_eq!(e.indels, 0);
    }

    /// V6: an `N` (spliced) adjacent to a `D` (deletion) — `indels` counts ONLY
    /// the D bases (N excluded, Perl 4370/4376); the `md_seq` includes the D base
    /// but skips the intron.
    #[test]
    fn extract_n_and_deletion_counts_d_only_index0() {
        let g = genome_of("chr1", b"ACGTTTTTACGTACG");
        let b = best("chr1", 1, 0, "3M2N2M1D2M"); // pos0=0
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_single_end(&b, &g, false, &mut c).unwrap();
        assert!(e.extracted);
        assert_eq!(e.indels, 1); // D(1) only; N excluded
        // md_seq: M(3)=chr[0..3]="ACG", M(2)=chr[5..7]="TT", D(1)=chr[7..8]="T", M(2)=chr[8..10]="AC"
        assert_eq!(e.genomic_seq_for_md_tag, b"ACGTTTAC");
    }

    /// V6: a spliced read on the OB strand (index 3, GA read / GA genome) —
    /// prepend-2 (no append, no revcomp for eff 3), the intron skipped.
    #[test]
    fn extract_spliced_n_on_ga_strand_index3() {
        let g = genome_of("chr1", b"ACGTTTTTACGTACG");
        let b = best("chr1", 3, 3, "3M4N3M"); // pos0=2
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_single_end(&b, &g, false, &mut c).unwrap();
        assert!(e.extracted);
        assert_eq!(e.alignment_strand, b'+');
        assert_eq!(e.read_conversion, Conversion::Ga);
        assert_eq!(e.genome_conversion, Conversion::Ga);
        // prepend chr[0..2]="AC", M(3)=chr[2..5]="GTT", N(4) skips chr[5..9], M(3)=chr[9..12]="CGT"
        assert_eq!(e.unmodified_genomic_sequence, b"ACGTTCGT");
        assert_eq!(e.indels, 0);
        assert_eq!(c.ga_ga_count, 1);
    }

    #[test]
    fn extract_insertion_pads_x_no_indels() {
        let g = genome_of("chr1", b"ACGTACGTAC");
        let b = best("chr1", 1, 0, "2M1I2M"); // read consumes 5, genome consumes 4
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_single_end(&b, &g, false, &mut c).unwrap();
        // M(2)="AC", I(1)="X", M(2)=chr[2..4]="GT", +2=chr[4..6]="AC" → "ACXGTAC"
        assert_eq!(e.unmodified_genomic_sequence, b"ACXGTAC");
        assert_eq!(e.indels, 0); // insertions do NOT add to indels
    }

    #[test]
    fn methylation_call_ct_contexts() {
        // genomic = read+2. read "CCC" vs genomic "CGCACG" exercises Z (CpG), then.
        // pos0: C==C, downstream G → Z (meCpG)
        // pos1: read C vs genomic C? build a precise case below instead.
        let mut c = Counters::default();
        // read "CCC" vs genomic "CGCAAA":
        // i0: seq C == g[0] C, downstream g[1]=G → Z (meCpG)
        // i1: seq C != g[1] G (and not C→T) → '.'
        // i2: seq C == g[2] C, downstream g[3]=A, 2nd g[4]=A → H (meCHH)
        let call = methylation_call(b"CCC", b"CGCAAA", Conversion::Ct, &mut c);
        assert_eq!(call, b"Z.H");
        assert_eq!(c.total_me_cpg, 1);
        assert_eq!(c.total_me_chh, 1);
    }

    #[test]
    fn methylation_call_unmethylated_cpg_lowercase_z() {
        // read T where genomic C, downstream G → converted CpG → 'z'
        let mut c = Counters::default();
        let call = methylation_call(b"T", b"CGA", Conversion::Ct, &mut c);
        assert_eq!(call, b"z");
        assert_eq!(c.total_unme_cpg, 1);
    }

    #[test]
    fn methylation_call_unknown_context_via_n() {
        // C==C, downstream N → unknown methylated → 'U'
        let mut c = Counters::default();
        let call = methylation_call(b"C", b"CNA", Conversion::Ct, &mut c);
        assert_eq!(call, b"U");
        assert_eq!(c.total_me_c_unknown, 1);
    }

    #[test]
    fn methylation_call_unknown_via_padding_x_context() {
        // C==C, downstream X (insertion/soft-clip padding as the context base) → 'U'
        let mut c = Counters::default();
        let call = methylation_call(b"C", b"CXA", Conversion::Ct, &mut c);
        assert_eq!(call, b"U");
        assert_eq!(c.total_me_c_unknown, 1);
    }

    #[test]
    fn methylation_call_non_cytosine_is_dot() {
        let mut c = Counters::default();
        let call = methylation_call(b"AT", b"ATGG", Conversion::Ct, &mut c);
        assert_eq!(call, b"..");
    }

    // ---- PE genomic extraction ---------------------------------------------

    fn best_pe(
        chr: &str,
        index: usize,
        pos1: u32,
        pos2: u32,
        cigar1: &str,
        cigar2: &str,
    ) -> BestAlignmentPaired {
        BestAlignmentPaired {
            chromosome: chr.to_string(),
            index,
            position_1: pos1,
            position_2: pos2,
            cigar_1: cigar1.to_string(),
            cigar_2: cigar2.to_string(),
            md_tag_1: String::new(),
            md_tag_2: String::new(),
            bowtie_sequence_1: String::new(),
            bowtie_sequence_2: String::new(),
            flag_1: 99,
            flag_2: 147,
            sum_of_alignment_scores: 0,
            sum_of_alignment_scores_second_best: None,
            mapq: 40,
        }
    }

    #[test]
    fn pe_extract_index0_appends_two_revcomps_mate2_counts_ct_ga_ct() {
        // index 0 (OT): r1 '+', r2 '-'; +2 at 3' for both; mate2 revcomp'd.
        let g = genome_of("chr1", b"AAACGTACGTAA"); // 12 bp
        let b = best_pe("chr1", 0, 3, 5, "4M", "4M"); // pos0_1=2, pos0_2=4
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_paired_end(&b, &g, &mut c).unwrap();
        assert_eq!(e.alignment_read_1, b'+');
        assert_eq!(e.alignment_read_2, b'-');
        assert_eq!(e.read_conversion_1, Conversion::Ct);
        assert_eq!(e.read_conversion_2, Conversion::Ga);
        assert_eq!(e.genome_conversion, Conversion::Ct);
        // mate1: M(4)=chr[2..6]="ACGT" + 3'chr[6..8]="AC" = "ACGTAC"
        assert_eq!(e.unmodified_genomic_sequence_1, b"ACGTAC");
        // mate2: M(4)=chr[4..8]="GTAC" + 3'chr[8..10]="GT" = "GTACGT", then revcomp
        assert_eq!(
            e.unmodified_genomic_sequence_2,
            reverse_complement(b"GTACGT")
        );
        assert_eq!(c.ct_ga_ct_count, 1);
        assert_eq!(c.ga_ct_ct_count, 0);
    }

    #[test]
    fn pe_extract_index2_revcomps_mate1_counts_ga_ct_ct() {
        // index 2 (CTOT): r1 '-', r2 '+'; +2 at 3'; mate1 revcomp'd.
        let g = genome_of("chr1", b"AAACGTACGTAA");
        let b = best_pe("chr1", 2, 3, 5, "4M", "4M");
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_paired_end(&b, &g, &mut c).unwrap();
        assert_eq!(e.alignment_read_1, b'-');
        assert_eq!(e.alignment_read_2, b'+');
        assert_eq!(e.read_conversion_1, Conversion::Ga);
        assert_eq!(e.genome_conversion, Conversion::Ct);
        // mate1 revcomp'd: revcomp("ACGTAC"); mate2 left forward "GTACGT".
        assert_eq!(
            e.unmodified_genomic_sequence_1,
            reverse_complement(b"ACGTAC")
        );
        assert_eq!(e.unmodified_genomic_sequence_2, b"GTACGT");
        assert_eq!(c.ga_ct_ct_count, 1);
    }

    #[test]
    fn pe_extract_index1_prepends_two_counts_ga_ct_ga() {
        // index 1 (CTOB): +2 at 5' for both; mate2 revcomp'd.
        let g = genome_of("chr1", b"AAACGTACGTAA");
        let b = best_pe("chr1", 1, 5, 5, "4M", "4M"); // pos0=4; (4-2)>0 ok (mate1 strict)
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_paired_end(&b, &g, &mut c).unwrap();
        // mate1: 5' chr[2..4]="AC" + M(4)chr[4..8]="GTAC" = "ACGTAC"
        assert_eq!(e.unmodified_genomic_sequence_1, b"ACGTAC");
        assert_eq!(c.ga_ct_ga_count, 1);
    }

    #[test]
    fn pe_mate1_5prime_guard_is_strict_gt0() {
        // index 1, position_1 = 3 → pos0 = 2 → (2-2) > 0 is FALSE → mate1 Edge
        // (the SE/mate2 `>= 0` guard would PASS here). Read 2 never walked.
        let g = genome_of("chr1", b"AAACGTACGTAA");
        let b = best_pe("chr1", 1, 3, 10, "4M", "1M");
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_paired_end(&b, &g, &mut c).unwrap();
        assert_eq!(e.unmodified_genomic_sequence_1.len(), 0); // edge → empty
        assert_eq!(e.unmodified_genomic_sequence_2.len(), 0); // mate2 never walked
        assert_eq!(c.ga_ct_ga_count, 0); // NO counter past an edge guard
    }

    #[test]
    fn pe_mate1_5prime_passes_at_position_4() {
        // the boundary: position_1 = 4 → pos0 = 3 → (3-2) = 1 > 0 → passes.
        let g = genome_of("chr1", b"AAACGTACGTAA");
        let b = best_pe("chr1", 1, 4, 5, "4M", "4M");
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_paired_end(&b, &g, &mut c).unwrap();
        assert_eq!(e.unmodified_genomic_sequence_1.len(), 4 + 2);
        assert_eq!(c.ga_ct_ga_count, 1);
    }

    #[test]
    fn pe_mate2_chr_edge_leaves_mate1_full_mate2_short() {
        // index 0 (3' append). mate1 fine; mate2 ends within 2 bp of chr end →
        // mate2 3' guard fires → R1 full (read_len+2), R2 short, NO counter.
        let g = genome_of("chr1", b"AAACGTACGT"); // 10 bp
        // mate1 at pos 1 (4M → ends at 4, +2 ok); mate2 at pos 5 (4M → ends at 8, +2 needs 10 ok)...
        // make mate2 end at the edge: pos 6 (0-based 5), 4M → ends 9, +2 needs 11 > 10 → edge.
        let b = best_pe("chr1", 0, 1, 6, "4M", "4M");
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_paired_end(&b, &g, &mut c).unwrap();
        assert_eq!(e.unmodified_genomic_sequence_1.len(), 4 + 2); // mate1 full
        assert_ne!(e.unmodified_genomic_sequence_2.len(), 4 + 2); // mate2 short → driver gate fails
        assert_eq!(c.ct_ga_ct_count, 0); // edge → no strand bucket
    }

    // ---- Phase 8: SE pbat `+2` index modifier + the GA methylation branch ----

    #[test]
    fn extract_pbat_se_index0_eff2_ga_ct() {
        // pbat SE: physical slot 0 + pbat=true → eff 2 → (-, GA, CT). 3' append +
        // revcomp; lands in the ga_ct (CTOT) bucket. (No prior test passes pbat=true.)
        let g = genome_of("chr1", b"TTGCGTACTT"); // 10 bp
        let b = best("chr1", 3, 0, "6M"); // pos0 = 2
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_single_end(&b, &g, true, &mut c).unwrap();
        assert!(e.extracted);
        assert_eq!(e.alignment_strand, b'-');
        assert_eq!(e.read_conversion, Conversion::Ga);
        assert_eq!(e.genome_conversion, Conversion::Ct);
        // M chr[2..8]="GCGTAC" + 3' chr[8..10]="TT" = "GCGTACTT", then revcomp.
        assert_eq!(
            e.unmodified_genomic_sequence,
            reverse_complement(b"GCGTACTT")
        );
        assert_eq!(e.unmodified_genomic_sequence.len(), 6 + 2);
        assert_eq!(c.ga_ct_count, 1);
        assert_eq!(c.ct_ct_count, 0);
    }

    #[test]
    fn extract_pbat_se_index1_eff3_ga_ga() {
        // pbat SE: physical slot 1 + pbat=true → eff 3 → (+, GA, GA). 5' prepend, NO
        // revcomp; lands in the ga_ga (CTOB) bucket.
        let g = genome_of("chr1", b"TTGCGTACTT");
        let b = best("chr1", 3, 1, "6M"); // pos0 = 2
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_single_end(&b, &g, true, &mut c).unwrap();
        assert!(e.extracted);
        assert_eq!(e.alignment_strand, b'+');
        assert_eq!(e.read_conversion, Conversion::Ga);
        assert_eq!(e.genome_conversion, Conversion::Ga);
        // 5' chr[0..2]="TT" + M chr[2..8]="GCGTAC" = "TTGCGTAC" (no revcomp).
        assert_eq!(e.unmodified_genomic_sequence, b"TTGCGTAC");
        assert_eq!(c.ga_ga_count, 1);
        assert_eq!(c.ga_ct_count, 0);
    }

    #[test]
    fn methylation_call_ga_branch_contexts() {
        // GA branch (Perl 4916–4998): compares seq[i] to genomic[i+2]; context looks
        // UPSTREAM (i+1 then i). read "GCGTAC" vs genomic "TTGCGTAC":
        //  i0 g=genomic[2]=G, read G == → meC; upstream genomic[1]=T, genomic[0]=T → CHH 'H'
        //  i1 g=genomic[3]=C, read C ==, g!=G → '.'
        //  i2 g=genomic[4]=G, read G == → meC; upstream genomic[3]=C → CpG 'Z'
        //  i3 g=T '.' ; i4 g=A '.' ; i5 g=C '.'
        let mut c = Counters::default();
        let call = methylation_call(b"GCGTAC", b"TTGCGTAC", Conversion::Ga, &mut c);
        assert_eq!(call, b"H.Z...");
        assert_eq!(c.total_me_chh, 1);
        assert_eq!(c.total_me_cpg, 1);
    }

    #[test]
    fn methylation_call_ga_branch_converted_g_to_a_unmethylated() {
        // GA branch, a converted (unmethylated) base: read 'A' where genomic[i+2]='G'
        // → converted; upstream genomic[1]='C' → CpG → lower-case 'z'.
        let mut c = Counters::default();
        let call = methylation_call(b"A", b"CCG", Conversion::Ga, &mut c);
        assert_eq!(call, b"z");
        assert_eq!(c.total_unme_cpg, 1);
    }

    #[test]
    fn pe_extract_deletion_index0_builds_md_seq_and_indels() {
        // mate1 has a deletion (2M1D2M); md_seq includes the deleted base, indels=1.
        let g = genome_of("chr1", b"ACGTACGTACGT"); // 12 bp
        let b = best_pe("chr1", 0, 1, 1, "2M1D2M", "4M");
        let mut c = Counters::default();
        let e = extract_corresponding_genomic_sequence_paired_end(&b, &g, &mut c).unwrap();
        assert_eq!(e.indels_1, 1);
        assert_eq!(e.indels_2, 0);
        // md_seq_1: M"AC" + D chr[2..3]="G" + M chr[3..5]="TA" = "ACGTA"
        assert_eq!(e.genomic_seq_for_md_tag_1, b"ACGTA");
    }
}
