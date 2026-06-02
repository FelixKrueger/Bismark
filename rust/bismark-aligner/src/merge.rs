//! Bisulfite best-alignment merge — a port of Perl `check_results_single_end`
//! (2702–3151) for single-end directional reads (2 instances).
//!
//! For one original read, drive the per-instance [`SamStream`]s in read-ID
//! lockstep, pick the unique best alignment by `AS` across instances (handling
//! same-thread and cross-instance ambiguity), assign the strand via the instance
//! index, and compute MAPQ. Produces a [`Decision`]; the genomic-sequence
//! extraction, `XM` call, BAM output (Phase 5), report counters per-strand
//! (Phase 5), and unmapped/ambiguous file routing (Phase 6) are NOT done here.

use std::collections::HashMap;

use crate::align::SamStream;
use crate::error::{AlignerError, Result};
use crate::mapq::calc_mapq;

/// The chosen unique-best alignment (≈ Perl `methylation_call_params->{$id}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BestAlignment {
    /// RNAME with the `_CT_converted`/`_GA_converted` suffix stripped.
    pub chromosome: String,
    /// 1-based POS.
    pub position: u32,
    /// Instance index: 0 = CTreadCTgenome (OT), 1 = CTreadGAgenome (OB), 2/3 = PE/non-dir.
    pub index: usize,
    /// `AS:i:` of the chosen alignment.
    pub alignment_score: i64,
    /// Second-best score fed to MAPQ (per the 3075–80 conditional).
    pub alignment_score_second_best: Option<i64>,
    /// `MD:Z:` of the chosen alignment.
    pub md_tag: String,
    /// CIGAR of the chosen alignment.
    pub cigar: String,
    /// The (converted) read sequence as Bowtie 2 reported it.
    pub bowtie_sequence: String,
    /// Computed MAPQ.
    pub mapq: u8,
}

/// Per-read outcome of the merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// A single best alignment (→ Phase 5 genomic-seq + `XM` + BAM).
    UniqueBest(BestAlignment),
    /// Multiple equally-good alignments (→ Phase 6 routes to `--ambiguous`/`--unmapped`/none).
    ///
    /// `first_ambig` carries the raw (suffix-intact) SAM line of the alignment
    /// that established the best score, **only** when the read was booted on the
    /// *within-thread* ambiguity path (Perl writes `--ambig_bam` at 2976) AND
    /// `--ambig_bam` was requested. The *cross-instance-tie* path carries `None`
    /// (Perl's 3091 block has no `AMBIBAM` write). Phase 6 writes the ambig BAM
    /// iff this is `Some`.
    Ambiguous { first_ambig: Option<String> },
    /// No alignment in any instance (→ Phase 6 routes to `--unmapped`/none).
    NoAlignment,
    /// `--directional` wrong-strand rejection (chosen index 2/3).
    Rejected,
}

/// Run counters. Phase 4 fills the alignment-outcome counts; Phase 5 adds the
/// per-strand counts (behind the chromosome-edge guards), the
/// could-not-extract count, and the methylation-context tallies.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Counters {
    /// Reads processed (driver-incremented).
    pub sequences_count: u64,
    /// Reads with a unique best alignment.
    pub unique_best_alignment_count: u64,
    /// Reads booted as ambiguous.
    pub unsuitable_sequence_count: u64,
    /// Reads with no alignment in any instance.
    pub no_single_alignment_found: u64,
    /// Reads rejected by `--directional` (index 2/3).
    pub alignments_rejected_count: u64,

    // ---- Phase 5: per-strand counts (Perl 4402/4411/4426/4441) -------------
    // Incremented in genomic extraction, ONLY when no chromosome-edge guard
    // fired (so an edge read counts in `unique_best` but in no strand bucket).
    /// CT-read vs CT-genome (OT, index 0).
    pub ct_ct_count: u64,
    /// CT-read vs GA-genome (CTOB/OB, index 1).
    pub ct_ga_count: u64,
    /// GA-read vs CT-genome (CTOT, index 2 — non-dir/pbat).
    pub ga_ct_count: u64,
    /// GA-read vs GA-genome (OB, index 3 — non-dir/pbat).
    pub ga_ga_count: u64,

    /// Reads whose genomic sequence could not be extracted (Perl 3129) — failed
    /// the `len == read_len + 2` guard (chromosome edge); counted but NOT written.
    pub genomic_sequence_could_not_be_extracted_count: u64,

    // ---- Phase 5: methylation-context tallies (Perl 5006–5013) -------------
    // Incremented in `methylation_call`; REPORTED in the Phase-6 report.
    /// Methylated C in CpG context (`Z`).
    pub total_me_cpg: u64,
    /// Methylated C in CHG context (`X`).
    pub total_me_chg: u64,
    /// Methylated C in CHH context (`H`).
    pub total_me_chh: u64,
    /// Methylated C in unknown context (`U`).
    pub total_me_c_unknown: u64,
    /// Unmethylated C in CpG context (`z`).
    pub total_unme_cpg: u64,
    /// Unmethylated C in CHG context (`x`).
    pub total_unme_chg: u64,
    /// Unmethylated C in CHH context (`h`).
    pub total_unme_chh: u64,
    /// Unmethylated C in unknown context (`u`).
    pub total_unme_c_unknown: u64,
}

/// An alignment stored at a `chromosome:position` key during the merge.
struct Stored {
    alignment_score: i64,
    second_best: Option<i64>,
    index: usize,
    chromosome: String,
    position: u32,
    cigar: String,
    md_tag: String,
    bowtie_sequence: String,
}

/// Run the merge for one read across the instances; advances the matching
/// streams past this read. `sequence` is the original (uc) read (for MAPQ length).
#[allow(clippy::too_many_arguments)]
pub fn check_results_single_end<S: SamStream>(
    identifier: &str,
    sequence: &str,
    streams: &mut [S],
    directional: bool,
    score_min_intercept: f64,
    score_min_slope: f64,
    want_ambig: bool,
    counters: &mut Counters,
) -> Result<Decision> {
    let mut best_as_so_far: Option<i64> = None;
    let mut amb_same_thread = false;
    let mut alignments: HashMap<String, Stored> = HashMap::new();
    // The raw SAM line that established the best score (Perl `$first_ambig_alignment`,
    // set at 2806 + 2822). Captured only when `--ambig_bam` is on; de-converted
    // at write time (output.rs). Used only on the within-thread ambiguity path.
    let mut first_ambig: Option<String> = None;

    for (index, stream) in streams.iter_mut().enumerate() {
        // Only instances whose current record is for THIS read (lockstep).
        if stream.current().is_none_or(|r| r.qname != identifier) {
            continue;
        }
        let rec = stream.current().unwrap().clone();

        // Unmapped (flag == 4): advance once; the next line must be a different
        // read (Perl 2738–58 die), then move to the next instance.
        if rec.is_unmapped() {
            stream.advance()?;
            if stream.current().is_some_and(|r| r.qname == identifier) {
                return Err(AlignerError::Validation(format!(
                    "Sequence with ID {identifier} did not produce any alignment, but next seq-ID was also {identifier}!"
                )));
            }
            continue;
        }

        // De-convert RNAME (2763–68).
        let chromosome = rec
            .rname
            .strip_suffix("_CT_converted")
            .or_else(|| rec.rname.strip_suffix("_GA_converted"))
            .ok_or_else(|| {
                AlignerError::Validation(format!(
                    "Chromosome number extraction failed for {}",
                    rec.rname
                ))
            })?
            .to_string();

        // AS + MD are mandatory on a mapped record (Perl die 2838).
        let alignment_score = rec.alignment_score.ok_or_else(|| {
            AlignerError::Validation(format!(
                "Failed to extract alignment score from line {}",
                rec.raw_line
            ))
        })?;
        let md_tag = rec.md_tag.clone().ok_or_else(|| {
            AlignerError::Validation(format!(
                "Failed to extract MD tag from line {}",
                rec.raw_line
            ))
        })?;
        let second_best = rec.second_best;

        // overwrite / best_AS_so_far (2802–2834): `>=` keeps equally-good
        // alignments; a strictly-better one resets amb_same_thread.
        let mut overwrite = false;
        match best_as_so_far {
            None => {
                best_as_so_far = Some(alignment_score);
                overwrite = true;
                // First alignment seen sets `first_ambig` (Perl 2806–2810).
                if want_ambig {
                    first_ambig = Some(rec.raw_line.clone());
                }
            }
            Some(best) => {
                if alignment_score >= best {
                    overwrite = true;
                    if alignment_score > best {
                        amb_same_thread = false;
                        // A strictly-better alignment resets `first_ambig` (Perl 2822–2826);
                        // an EQUAL alignment does NOT (no re-capture).
                        if want_ambig {
                            first_ambig = Some(rec.raw_line.clone());
                        }
                    }
                    best_as_so_far = Some(alignment_score);
                }
            }
        }

        // second-best handling (2840–2953).
        if let Some(sb) = second_best {
            if alignment_score == sb {
                // this thread is itself ambiguous; store nothing.
                if best_as_so_far == Some(alignment_score) {
                    amb_same_thread = true;
                }
            } else if overwrite {
                insert_alignment(
                    &mut alignments,
                    &chromosome,
                    &rec,
                    index,
                    alignment_score,
                    Some(sb),
                    &md_tag,
                );
            }
        } else if overwrite {
            insert_alignment(
                &mut alignments,
                &chromosome,
                &rec,
                index,
                alignment_score,
                None,
                &md_tag,
            );
        }

        // Discard the rest of this read's lines in this stream (advance-until-qname-changes).
        while stream.current().is_some_and(|r| r.qname == identifier) {
            stream.advance()?;
        }
    }

    // Same-thread ambiguity → boot (2957–2988). This is the ONLY SE path that
    // writes `--ambig_bam` (Perl 2976), so it carries the captured `first_ambig`.
    if amb_same_thread {
        counters.unsuitable_sequence_count += 1;
        return Ok(Decision::Ambiguous { first_ambig });
    }
    // No alignment anywhere (2991).
    if alignments.is_empty() {
        counters.no_single_alignment_found += 1;
        return Ok(Decision::NoAlignment);
    }

    // Unique-best selection (3033–3088).
    let mut entries: Vec<Stored> = alignments.into_values().collect();
    let (best, second_for_mapq) = if entries.len() == 1 {
        let b = entries.pop().unwrap();
        let s = b.second_best;
        (b, s)
    } else if entries.len() <= 4 {
        entries.sort_by_key(|s| std::cmp::Reverse(s.alignment_score));
        if entries[0].alignment_score == entries[1].alignment_score {
            counters.unsuitable_sequence_count += 1; // 3060–63
            // Cross-instance tie: Perl's 3091 block has NO `AMBIBAM` write →
            // no ambig-BAM record for this read (it still goes to the FastQ aux).
            return Ok(Decision::Ambiguous { first_ambig: None });
        }
        let runner_up = entries[1].alignment_score;
        let b = entries.into_iter().next().unwrap();
        // second-best for MAPQ (3075–80): best's own second-best only if it is
        // strictly greater than the runner-up's AS; otherwise the runner-up's AS.
        let s = match b.second_best {
            Some(sb) if sb > runner_up => Some(sb),
            _ => Some(runner_up),
        };
        (b, s)
    } else {
        return Err(AlignerError::Validation(format!(
            "There are too many potential hits for this sequence (1-4 expected, but found: {})",
            entries.len()
        )));
    };

    // --directional rejection (3112–18): chosen index 2/3 (inert on SE-directional).
    if directional && (best.index == 2 || best.index == 3) {
        counters.alignments_rejected_count += 1;
        return Ok(Decision::Rejected);
    }

    counters.unique_best_alignment_count += 1; // 3121
    let mapq = calc_mapq(
        sequence.len(),
        None,
        best.alignment_score,
        second_for_mapq,
        score_min_intercept,
        score_min_slope,
    );

    Ok(Decision::UniqueBest(BestAlignment {
        chromosome: best.chromosome,
        position: best.position,
        index: best.index,
        alignment_score: best.alignment_score,
        alignment_score_second_best: second_for_mapq,
        md_tag: best.md_tag,
        cigar: best.cigar,
        bowtie_sequence: best.bowtie_sequence,
        mapq,
    }))
}

#[allow(clippy::too_many_arguments)]
fn insert_alignment(
    alignments: &mut HashMap<String, Stored>,
    chromosome: &str,
    rec: &crate::align::SamRecord,
    index: usize,
    alignment_score: i64,
    second_best: Option<i64>,
    md_tag: &str,
) {
    // Keyed by chromosome:position → same-location alignments dedup (2877–2894).
    let loc = format!("{chromosome}:{}", rec.pos);
    alignments.insert(
        loc,
        Stored {
            alignment_score,
            second_best,
            index,
            chromosome: chromosome.to_string(),
            position: rec.pos,
            cigar: rec.cigar.clone(),
            md_tag: md_tag.to_string(),
            bowtie_sequence: rec.seq.clone(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::align::SamRecord;

    /// Canned stream double for unit-testing the merge without a subprocess.
    struct VecStream {
        records: Vec<SamRecord>,
        pos: usize,
    }
    impl VecStream {
        fn new(lines: &[&str]) -> Self {
            VecStream {
                records: lines.iter().map(|l| SamRecord::parse(l).unwrap()).collect(),
                pos: 0,
            }
        }
    }
    impl SamStream for VecStream {
        fn current(&self) -> Option<&SamRecord> {
            self.records.get(self.pos)
        }
        fn advance(&mut self) -> Result<()> {
            self.pos += 1;
            Ok(())
        }
    }

    // helpers to build canned SAM lines
    fn mapped(qname: &str, rname: &str, pos: u32, as_i: i64, md: &str, xs: Option<i64>) -> String {
        let xs = xs.map(|v| format!("\tXS:i:{v}")).unwrap_or_default();
        format!(
            "{qname}\t0\t{rname}\t{pos}\t40\t10M\t*\t0\t0\tACGTACGTAC\tIIIIIIIIII\tAS:i:{as_i}{xs}\tMD:Z:{md}"
        )
    }
    fn unmapped(qname: &str) -> String {
        format!("{qname}\t4\t*\t0\t0\t*\t*\t0\t0\tACGTACGTAC\tIIIIIIIIII")
    }

    fn run(id: &str, s0: &[&str], s1: &[&str], directional: bool) -> (Decision, Counters) {
        run_amb(id, s0, s1, directional, false)
    }

    fn run_amb(
        id: &str,
        s0: &[&str],
        s1: &[&str],
        directional: bool,
        want_ambig: bool,
    ) -> (Decision, Counters) {
        let mut streams = vec![VecStream::new(s0), VecStream::new(s1)];
        let mut c = Counters::default();
        let d = check_results_single_end(
            id,
            "ACGTACGTAC",
            &mut streams,
            directional,
            0.0,
            -0.2,
            want_ambig,
            &mut c,
        )
        .unwrap();
        (d, c)
    }

    #[test]
    fn unique_best_one_instance_other_unmapped() {
        let (d, c) = run(
            "r1",
            &[&mapped("r1", "chr1_CT_converted", 100, 0, "10", None)],
            &[&unmapped("r1")],
            true,
        );
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.index, 0);
                assert_eq!(b.chromosome, "chr1"); // de-converted
                assert_eq!(b.position, 100);
            }
            other => panic!("expected UniqueBest, got {other:?}"),
        }
        assert_eq!(c.unique_best_alignment_count, 1);
    }

    #[test]
    fn best_across_instances_by_score() {
        // instance 1 has the better (higher) AS → it wins.
        let (d, _) = run(
            "r1",
            &[&mapped("r1", "chr1_CT_converted", 100, -6, "10", None)],
            &[&mapped("r1", "chr2_GA_converted", 200, 0, "10", None)],
            false,
        );
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.index, 1);
                assert_eq!(b.chromosome, "chr2");
                assert_eq!(b.alignment_score, 0);
                assert_eq!(b.alignment_score_second_best, Some(-6)); // runner-up AS
            }
            other => panic!("expected UniqueBest, got {other:?}"),
        }
    }

    #[test]
    fn cross_instance_tie_is_ambiguous() {
        let (d, c) = run(
            "r1",
            &[&mapped("r1", "chr1_CT_converted", 100, 0, "10", None)],
            &[&mapped("r1", "chr2_GA_converted", 200, 0, "10", None)],
            false,
        );
        assert!(matches!(d, Decision::Ambiguous { .. }));
        assert_eq!(c.unsuitable_sequence_count, 1);
    }

    #[test]
    fn same_thread_ambiguity_boots() {
        // one instance reports AS == XS (second-best equal) at the best score.
        let (d, c) = run(
            "r1",
            &[&mapped("r1", "chr1_CT_converted", 100, 0, "10", Some(0))],
            &[&unmapped("r1")],
            true,
        );
        assert!(matches!(d, Decision::Ambiguous { .. }));
        assert_eq!(c.unsuitable_sequence_count, 1);
    }

    #[test]
    fn same_location_in_both_instances_dedups() {
        // both instances map to the SAME chr:pos → one entry → unique best.
        let (d, _) = run(
            "r1",
            &[&mapped("r1", "chr1_CT_converted", 100, 0, "10", None)],
            &[&mapped("r1", "chr1_GA_converted", 100, 0, "10", None)],
            false,
        );
        // both de-convert to chr1:100 → single alignments entry → UniqueBest.
        assert!(matches!(d, Decision::UniqueBest(_)));
    }

    #[test]
    fn no_alignment_when_both_unmapped() {
        let (d, c) = run("r1", &[&unmapped("r1")], &[&unmapped("r1")], true);
        assert_eq!(d, Decision::NoAlignment);
        assert_eq!(c.no_single_alignment_found, 1);
    }

    #[test]
    fn missing_converted_suffix_errors() {
        let mut streams = vec![
            VecStream::new(&[&mapped("r1", "chr1", 100, 0, "10", None)]), // no _CT/_GA suffix
            VecStream::new(&[&unmapped("r1")]),
        ];
        let mut c = Counters::default();
        let r = check_results_single_end(
            "r1",
            "ACGTACGTAC",
            &mut streams,
            true,
            0.0,
            -0.2,
            false,
            &mut c,
        );
        assert!(r.is_err());
    }

    #[test]
    fn flag4_then_same_id_dies() {
        // an unmapped marker followed by another line for the same read → die.
        let mut streams = vec![
            VecStream::new(&[
                &unmapped("r1"),
                &mapped("r1", "chr1_CT_converted", 100, 0, "10", None),
            ]),
            VecStream::new(&[&unmapped("r1")]),
        ];
        let mut c = Counters::default();
        let r = check_results_single_end(
            "r1",
            "ACGTACGTAC",
            &mut streams,
            true,
            0.0,
            -0.2,
            false,
            &mut c,
        );
        assert!(r.is_err());
    }

    #[test]
    fn directional_rejection_index_2() {
        // 4-instance setup; the best alignment is in instance 2 (CTOT) → rejected.
        let mut streams = vec![
            VecStream::new(&[&unmapped("r1")]),
            VecStream::new(&[&unmapped("r1")]),
            VecStream::new(&[&mapped("r1", "chr1_CT_converted", 100, 0, "10", None)]),
            VecStream::new(&[&unmapped("r1")]),
        ];
        let mut c = Counters::default();
        let d = check_results_single_end(
            "r1",
            "ACGTACGTAC",
            &mut streams,
            true,
            0.0,
            -0.2,
            false,
            &mut c,
        )
        .unwrap();
        assert_eq!(d, Decision::Rejected);
        assert_eq!(c.alignments_rejected_count, 1);
    }

    #[test]
    fn second_best_uses_best_own_when_greater_than_runner_up() {
        // instance 0: AS 0 with its own XS -2; instance 1: AS -5 (runner-up).
        // 3075 arm: best.second_best (-2) > runner-up AS (-5) → MAPQ second = -2.
        let mut streams = vec![
            VecStream::new(&[&mapped("r1", "chr1_CT_converted", 100, 0, "10", Some(-2))]),
            VecStream::new(&[&mapped("r1", "chr2_GA_converted", 200, -5, "10", None)]),
        ];
        let mut c = Counters::default();
        let d = check_results_single_end(
            "r1",
            "ACGTACGTAC",
            &mut streams,
            false,
            0.0,
            -0.2,
            false,
            &mut c,
        )
        .unwrap();
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.index, 0);
                assert_eq!(b.alignment_score, 0);
                assert_eq!(b.alignment_score_second_best, Some(-2)); // best's own, not runner-up -5
            }
            other => panic!("expected UniqueBest, got {other:?}"),
        }
    }

    #[test]
    fn three_instances_picks_highest() {
        // ascending AS by instance order so all three are stored (overwrite on >=).
        let mut streams = vec![
            VecStream::new(&[&mapped("r1", "chrA_CT_converted", 1, -8, "10", None)]),
            VecStream::new(&[&mapped("r1", "chrB_CT_converted", 2, -5, "10", None)]),
            VecStream::new(&[&mapped("r1", "chrC_CT_converted", 3, -2, "10", None)]),
        ];
        let mut c = Counters::default();
        let d = check_results_single_end(
            "r1",
            "ACGTACGTAC",
            &mut streams,
            false,
            0.0,
            -0.2,
            false,
            &mut c,
        )
        .unwrap();
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.index, 2); // AS -2 is highest
                assert_eq!(b.alignment_score, -2);
                assert_eq!(b.alignment_score_second_best, Some(-5)); // runner-up AS
            }
            other => panic!("expected UniqueBest, got {other:?}"),
        }
        assert_eq!(c.unique_best_alignment_count, 1);
    }

    #[test]
    fn too_many_hits_errors() {
        // 5 instances, distinct loci, equal AS → 5 stored entries → die (>4).
        let mut streams = vec![
            VecStream::new(&[&mapped("r1", "chrA_CT_converted", 1, 0, "10", None)]),
            VecStream::new(&[&mapped("r1", "chrB_CT_converted", 2, 0, "10", None)]),
            VecStream::new(&[&mapped("r1", "chrC_CT_converted", 3, 0, "10", None)]),
            VecStream::new(&[&mapped("r1", "chrD_CT_converted", 4, 0, "10", None)]),
            VecStream::new(&[&mapped("r1", "chrE_CT_converted", 5, 0, "10", None)]),
        ];
        let mut c = Counters::default();
        let r = check_results_single_end(
            "r1",
            "ACGTACGTAC",
            &mut streams,
            false,
            0.0,
            -0.2,
            false,
            &mut c,
        );
        assert!(r.is_err());
    }

    // ---- Phase 6: --ambig_bam first_ambig capture ----------------------------

    #[test]
    fn within_thread_ambiguity_captures_first_ambig() {
        // instance 0: AS == XS (within-thread ambiguous). want_ambig → Some(line).
        let (d, _) = run_amb(
            "r1",
            &[&mapped("r1", "chr1_CT_converted", 100, 0, "10", Some(0))],
            &[&unmapped("r1")],
            true,
            true,
        );
        match d {
            Decision::Ambiguous { first_ambig } => {
                let line = first_ambig.expect("within-thread ambiguity must carry first_ambig");
                assert!(line.contains("chr1_CT_converted")); // raw, suffix intact
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn within_thread_ambiguity_no_capture_when_flag_off() {
        // same read, but want_ambig=false → no clone, first_ambig None.
        let (d, _) = run_amb(
            "r1",
            &[&mapped("r1", "chr1_CT_converted", 100, 0, "10", Some(0))],
            &[&unmapped("r1")],
            true,
            false,
        );
        assert_eq!(d, Decision::Ambiguous { first_ambig: None });
    }

    #[test]
    fn cross_instance_tie_has_no_first_ambig() {
        // cross-instance tie (different loci, equal AS) → NO ambig-BAM record
        // even with want_ambig (Perl 3091 block has no AMBIBAM write).
        let (d, _) = run_amb(
            "r1",
            &[&mapped("r1", "chr1_CT_converted", 100, 0, "10", None)],
            &[&mapped("r1", "chr2_GA_converted", 200, 0, "10", None)],
            false,
            true,
        );
        assert_eq!(d, Decision::Ambiguous { first_ambig: None });
    }

    #[test]
    fn first_ambig_captures_strict_improvement_instance() {
        // instance 0 AS -5 sets first_ambig; instance 1 AS 0 strictly improves
        // (re-captures) and ties itself (XS 0 → within-thread ambiguous). The
        // captured line must be instance 1's, not instance 0's (Perl 2822).
        let (d, _) = run_amb(
            "r1",
            &[&mapped("r1", "chr1_CT_converted", 100, -5, "10", None)],
            &[&mapped("r1", "chr2_GA_converted", 200, 0, "10", Some(0))],
            false,
            true,
        );
        match d {
            Decision::Ambiguous { first_ambig } => {
                let line = first_ambig.expect("must capture");
                assert!(
                    line.contains("chr2_GA_converted"),
                    "should be instance 1's line, got: {line}"
                );
                assert!(!line.contains("chr1_CT_converted"));
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }
}
