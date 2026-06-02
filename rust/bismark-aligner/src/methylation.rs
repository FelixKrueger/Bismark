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
use crate::merge::{BestAlignment, Counters};

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
}
