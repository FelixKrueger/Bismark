//! #787 Illumina 5-Base DUPLEX-consensus pairing + per-molecule reconciliation.
//!
//! DRAGEN's 5-Base library is `nonrandom-duplex`: each original DNA molecule yields a
//! top-strand (OT) read and a bottom-strand (OB) read, tagged with complementary
//! ("swapped") UMIs. Pairing those two strands into a *duplex family* lets us
//! reconcile the asymmetric 5mC->T signal **per molecule** rather than over a
//! population pileup (as [`crate::five_base_deconv`] does): within one family, a
//! cytosine that lost the C on only ONE strand is 5mC, while a cytosine gone on BOTH
//! strands is a genetic C>T/G>A variant. This removes cross-molecule contamination and
//! sharpens the call at low depth.
//!
//! This is distinct from the UMI-position dedup (`--five_base_umi_len`, which collapses
//! PCR copies of ONE strand) and from the population deconvolution. The module is pure
//! (no I/O): the BAM walk in the driver fills [`DuplexFamilies`]; the verdict reuses
//! [`crate::five_base_deconv::StrandPileup`].
//!
//! ## SE limitation
//!
//! In single-end, the OT and OB members sequence opposite ends of the fragment and
//! overlap only partially; per-base reconciliation applies only where both members
//! cover a site (others fall back to single-strand `Undetermined`). Full per-base
//! reconciliation is the paired-end follow-up (R1/R2 give both ends).

use std::collections::BTreeMap;
use std::io::Write;

use crate::five_base_deconv::{CytosineVerdict, StrandPileup};

/// nonrandom-duplex UMI swap model. The two strands of one molecule carry related
/// UMIs; canonicalizing both to the same value lets them hash into one family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UmiSwap {
    /// Both members carry the SAME UMI (simplest model / symmetric tagging).
    Identity,
    /// The two members carry reverse-complementary UMIs (the nonrandom-duplex swap):
    /// the bottom strand's UMI is the reverse complement of the top strand's.
    RevComp,
}

/// Reverse complement of an ASCII nucleotide UMI (non-ACGT bytes pass through).
fn revcomp(umi: &[u8]) -> Vec<u8> {
    umi.iter()
        .rev()
        .map(|&b| match b.to_ascii_uppercase() {
            b'A' => b'T',
            b'C' => b'G',
            b'G' => b'C',
            b'T' => b'A',
            other => other,
        })
        .collect()
}

/// Canonical UMI: the byte-wise minimum of the UMI and its swap-transform, so the two
/// duplex members (which carry the swapped pair) both map to the SAME canonical bytes.
pub fn canonical_umi(umi: &[u8], swap: UmiSwap) -> Vec<u8> {
    let umi_uc: Vec<u8> = umi.to_ascii_uppercase();
    let other = match swap {
        UmiSwap::Identity => umi_uc.clone(),
        UmiSwap::RevComp => revcomp(&umi_uc),
    };
    if umi_uc <= other { umi_uc } else { other }
}

/// The duplex family key: a genomic span + the canonical (swap-collapsed) UMI. The two
/// opposite-strand members of one molecule share this key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DuplexKey {
    pub chrom: String,
    /// 0-based reference start of the member's alignment.
    pub start: u32,
    /// 0-based reference end (exclusive) of the member's alignment.
    pub end: u32,
    /// Canonical UMI (empty when `umi_len == 0`: span-only key, collision-prone).
    pub canon_umi: Vec<u8>,
}

/// One CpG-cytosine observation a member contributes (filled by the deconv-style CIGAR
/// walk in the driver). `plus` = the cytosine is the genomic `C` of the CpG (OT own
/// strand) vs the genomic `G` (OB own strand). `t_equivalent` = the read shows the
/// converted/variant allele (`T` opposite a `C`, `A` opposite a `G`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SiteObs {
    pub pos0: u32,
    pub plus: bool,
    pub t_equivalent: bool,
}

/// A duplex family: the OT-member and OB-member observations of one molecule, plus the
/// number of reads contributing to each strand (a strand may have several PCR copies).
#[derive(Debug, Default, Clone)]
pub struct DuplexFamily {
    pub ot_reads: u32,
    pub ob_reads: u32,
    pub ot: Vec<SiteObs>,
    pub ob: Vec<SiteObs>,
}

impl DuplexFamily {
    /// `true` once both strands of the molecule are present (the duplex is complete).
    pub fn is_paired(&self) -> bool {
        self.ot_reads > 0 && self.ob_reads > 0
    }
}

/// A single opposite-strand read is exactly the duplex partner, so one informative
/// opposite read suffices to reconcile (unlike the population deconvolution, which wants
/// depth ≥2). Per-molecule resolution is the whole point of duplex.
pub const DUPLEX_MIN_OPP_DEPTH: u32 = 1;

/// Accumulates per-family observations keyed by [`DuplexKey`] in deterministic order.
#[derive(Debug, Default)]
pub struct DuplexFamilies {
    fams: BTreeMap<DuplexKey, DuplexFamily>,
}

/// Summary of a duplex run (the unit-test surface + report footer).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DuplexSummary {
    pub total_families: u64,
    pub duplex_paired: u64,
    pub singletons: u64,
    pub variant_sites: u64,
    pub methylation_sites: u64,
    pub undetermined_sites: u64,
    pub methylated_calls: u64,
    pub total_calls: u64,
}

impl DuplexFamilies {
    /// Record one read's CpG observations for a family member (`is_ot` selects the
    /// strand bucket). Counts as one read on that strand, regardless of how many CpG
    /// sites it covers.
    pub fn add_read(&mut self, key: DuplexKey, is_ot: bool, obs: Vec<SiteObs>) {
        let fam = self.fams.entry(key).or_default();
        if is_ot {
            fam.ot_reads += 1;
            fam.ot.extend(obs);
        } else {
            fam.ob_reads += 1;
            fam.ob.extend(obs);
        }
    }

    /// Number of families accumulated.
    pub fn len(&self) -> usize {
        self.fams.len()
    }

    /// `true` when no families were accumulated.
    pub fn is_empty(&self) -> bool {
        self.fams.is_empty()
    }

    /// Build the per-site two-strand tally for one paired family: for each CpG position
    /// the family covers, the OT member's bases go to the "own" side and the OB
    /// member's to the "opposite" side (or vice versa for a genomic `G` site). Returns
    /// `(pos0, plus, StrandPileup)` per site, in position order.
    fn family_sites(fam: &DuplexFamily) -> BTreeMap<u32, (bool, StrandPileup)> {
        let mut sites: BTreeMap<u32, (bool, StrandPileup)> = BTreeMap::new();
        // OT member: at a `+` (genomic C) site OT is the OWN strand; at a `-` (genomic
        // G) site OT is the OPPOSITE strand. OB is the mirror.
        for o in &fam.ot {
            let (_, p) = sites
                .entry(o.pos0)
                .or_insert((o.plus, StrandPileup::default()));
            p.observe(o.plus, o.t_equivalent); // OT own ⇔ plus site
        }
        for o in &fam.ob {
            let (_, p) = sites
                .entry(o.pos0)
                .or_insert((o.plus, StrandPileup::default()));
            p.observe(!o.plus, o.t_equivalent); // OB own ⇔ minus site
        }
        sites
    }

    /// Reconcile every family and roll up the summary (no I/O).
    pub fn reconcile(&self, min_opp_depth: u32, variant_opp_frac: f64) -> DuplexSummary {
        let mut s = DuplexSummary::default();
        for fam in self.fams.values() {
            s.total_families += 1;
            if !fam.is_paired() {
                s.singletons += 1;
                continue;
            }
            s.duplex_paired += 1;
            for (_pos0, (_plus, pileup)) in Self::family_sites(fam) {
                // A site with no OWN-strand coverage carries no methylation evidence (the
                // own strand holds the called cytosine); skip it.
                if pileup.own_c + pileup.own_t == 0 {
                    continue;
                }
                match pileup.classify(min_opp_depth, variant_opp_frac) {
                    CytosineVerdict::Variant => s.variant_sites += 1,
                    CytosineVerdict::Methylation { methylated, total } => {
                        s.methylation_sites += 1;
                        s.methylated_calls += methylated as u64;
                        s.total_calls += total as u64;
                    }
                    CytosineVerdict::Undetermined { methylated, total } => {
                        s.undetermined_sites += 1;
                        s.methylated_calls += methylated as u64;
                        s.total_calls += total as u64;
                    }
                }
            }
        }
        s
    }

    /// Write a per-family report and return the summary. Columns (paired families):
    /// chromosome, start(0-based), end, canonical-UMI, members, verdict-counts.
    pub fn write_report<W: Write>(
        &self,
        w: &mut W,
        min_opp_depth: u32,
        variant_opp_frac: f64,
    ) -> std::io::Result<DuplexSummary> {
        writeln!(
            w,
            "# Illumina 5-Base duplex-consensus families (#787)\n\
             # columns: chromosome\tstart(0-based)\tend\tcanonical_umi\tmembers\t\
             variant\tmethylation\tundetermined"
        )?;
        let mut s = DuplexSummary::default();
        for (key, fam) in &self.fams {
            s.total_families += 1;
            if !fam.is_paired() {
                s.singletons += 1;
                continue;
            }
            s.duplex_paired += 1;
            let (mut v, mut m, mut u) = (0u32, 0u32, 0u32);
            for (_pos0, (_plus, pileup)) in Self::family_sites(fam) {
                if pileup.own_c + pileup.own_t == 0 {
                    continue; // no own-strand coverage → no methylation call here
                }
                match pileup.classify(min_opp_depth, variant_opp_frac) {
                    CytosineVerdict::Variant => {
                        v += 1;
                        s.variant_sites += 1;
                    }
                    CytosineVerdict::Methylation { methylated, total } => {
                        m += 1;
                        s.methylation_sites += 1;
                        s.methylated_calls += methylated as u64;
                        s.total_calls += total as u64;
                    }
                    CytosineVerdict::Undetermined { methylated, total } => {
                        u += 1;
                        s.undetermined_sites += 1;
                        s.methylated_calls += methylated as u64;
                        s.total_calls += total as u64;
                    }
                }
            }
            let umi = if key.canon_umi.is_empty() {
                "NA".to_string()
            } else {
                String::from_utf8_lossy(&key.canon_umi).into_owned()
            };
            writeln!(
                w,
                "{}\t{}\t{}\t{}\t{}+{}\t{v}\t{m}\t{u}",
                key.chrom, key.start, key.end, umi, fam.ot_reads, fam.ob_reads,
            )?;
        }
        writeln!(
            w,
            "# families {} duplex-paired {} singletons {}",
            s.total_families, s.duplex_paired, s.singletons
        )?;
        Ok(s)
    }
}

// ===========================================================================
// Commit 2: duplex CONSENSUS base reconciliation (collapse a family to one read).
// ===========================================================================

/// What a reference position is, for consensus reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteKind {
    /// A `+`-strand CpG cytosine (genomic `C` followed by `G`): OT is the own strand.
    PlusCpG,
    /// A `-`-strand CpG cytosine (genomic `G` preceded by `C`): OB is the own strand.
    MinusCpG,
    /// Any other position: generic agree/quality/N reconciliation.
    Other,
}

/// Generic two-strand base reconciliation at a non-CpG position. Both bases are in
/// reference-forward orientation; each is `Some((base, qual))` or `None` (not covered).
fn reconcile_generic(ot: Option<(u8, u8)>, ob: Option<(u8, u8)>) -> (u8, u8) {
    match (ot, ob) {
        (None, None) => (b'N', 0),
        (Some(x), None) | (None, Some(x)) => x,
        (Some((ba, qa)), Some((bb, qb))) => {
            if ba.eq_ignore_ascii_case(&bb) {
                (ba, qa.max(qb)) // agreement
            } else if qa >= qb {
                if qa == qb { (b'N', qa) } else { (ba, qa) } // tie → N, else higher-Q
            } else {
                (bb, qb)
            }
        }
    }
}

/// THE asymmetric 5mC>T consensus base at one reference position. At a CpG cytosine the
/// OWN strand carries the methylation call (5mC reads as the T-equivalent) and the
/// OPPOSITE strand is the variant check: if the opposite strand ALSO shows the
/// T-equivalent the cytosine is gone on both strands (a C>T/G>A variant), so the
/// consensus is masked to `N` (excluded from methylation). Otherwise the consensus is
/// the own-strand base, so a true 5mC (own `T`, opposite intact) survives as a call.
/// Non-CpG positions fall back to [`reconcile_generic`].
///
/// `ot`/`ob` are each `Some((base, qual))` (reference-forward) or `None` (not covered).
pub fn consensus_base(kind: SiteKind, ot: Option<(u8, u8)>, ob: Option<(u8, u8)>) -> (u8, u8) {
    let cpg = |own: Option<(u8, u8)>, opp: Option<(u8, u8)>, t_equiv: u8| -> (u8, u8) {
        match own {
            None => (b'N', opp.map(|x| x.1).unwrap_or(0)), // no own strand → no call
            Some((b, q)) => match opp {
                // opposite also shows the T-equivalent ⇒ variant ⇒ mask out of methylation.
                Some((ob_b, _)) if ob_b.eq_ignore_ascii_case(&t_equiv) => (b'N', q),
                _ => (b, q), // own-strand base carries the (possibly methylated) call
            },
        }
    };
    match kind {
        SiteKind::PlusCpG => cpg(ot, ob, b'T'),
        SiteKind::MinusCpG => cpg(ob, ot, b'A'),
        SiteKind::Other => reconcile_generic(ot, ob),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::five_base_deconv::DEFAULT_VARIANT_OPP_FRAC;

    #[test]
    fn revcomp_basic() {
        assert_eq!(revcomp(b"ACGT"), b"ACGT"); // palindrome
        assert_eq!(revcomp(b"AAAT"), b"ATTT");
        assert_eq!(revcomp(b"GGCN"), b"NGCC"); // N passes through
    }

    /// The two members carry swapped (revcomp) UMIs but canonicalize equal.
    #[test]
    fn canonical_umi_revcomp_collapses_pair() {
        let top = b"AACCG";
        let bottom = revcomp(top); // what the other strand carries
        assert_eq!(
            canonical_umi(top, UmiSwap::RevComp),
            canonical_umi(&bottom, UmiSwap::RevComp)
        );
    }

    /// Identity swap: only the SAME UMI collapses to the same canonical value.
    #[test]
    fn canonical_umi_identity_is_uppercase_passthrough() {
        assert_eq!(canonical_umi(b"acgt", UmiSwap::Identity), b"ACGT");
        assert_ne!(
            canonical_umi(b"AAAA", UmiSwap::Identity),
            canonical_umi(b"CCCC", UmiSwap::Identity)
        );
    }

    fn key(umi: &[u8]) -> DuplexKey {
        DuplexKey {
            chrom: "chr1".into(),
            start: 100,
            end: 160,
            canon_umi: umi.to_vec(),
        }
    }

    /// One OT read + one OB read at the same key → a paired family; the 5mC site (own
    /// T-equiv, opposite intact) → methylation, the C>T site (both strands T-equiv) →
    /// variant. A single opposite read suffices (DUPLEX_MIN_OPP_DEPTH = 1).
    #[test]
    fn paired_family_separates_methylation_from_variant() {
        let mut d = DuplexFamilies::default();
        let k = key(b"AACCG");
        // OT read carries both sites; OB read carries both sites.
        d.add_read(
            k.clone(),
            true,
            vec![
                SiteObs {
                    pos0: 110,
                    plus: true,
                    t_equivalent: true,
                }, // 5mC
                SiteObs {
                    pos0: 130,
                    plus: true,
                    t_equivalent: true,
                }, // C>T (own)
            ],
        );
        d.add_read(
            k.clone(),
            false,
            vec![
                SiteObs {
                    pos0: 110,
                    plus: true,
                    t_equivalent: false,
                }, // intact (5mC)
                SiteObs {
                    pos0: 130,
                    plus: true,
                    t_equivalent: true,
                }, // T-equiv (variant)
            ],
        );

        let s = d.reconcile(DUPLEX_MIN_OPP_DEPTH, DEFAULT_VARIANT_OPP_FRAC);
        assert_eq!(s.total_families, 1);
        assert_eq!(s.duplex_paired, 1);
        assert_eq!(s.singletons, 0);
        assert_eq!(s.methylation_sites, 1);
        assert_eq!(s.variant_sites, 1);
        assert_eq!(s.methylated_calls, 1); // the 5mC site's own T
        assert_eq!(s.total_calls, 1);
    }

    /// Mismatched UMIs → two different keys → two singletons, not a pair.
    #[test]
    fn mismatched_umi_yields_two_singletons() {
        let mut d = DuplexFamilies::default();
        d.add_read(
            key(b"AAAAA"),
            true,
            vec![SiteObs {
                pos0: 110,
                plus: true,
                t_equivalent: true,
            }],
        );
        d.add_read(
            key(b"GGGGG"),
            false,
            vec![SiteObs {
                pos0: 110,
                plus: true,
                t_equivalent: false,
            }],
        );
        let s = d.reconcile(DUPLEX_MIN_OPP_DEPTH, DEFAULT_VARIANT_OPP_FRAC);
        assert_eq!(s.total_families, 2);
        assert_eq!(s.duplex_paired, 0);
        assert_eq!(s.singletons, 2);
    }

    /// Sites covered by only ONE strand of a family: an OWN-only `+` site (OT, no OB) →
    /// Undetermined (no opposite to deconvolute). An OPPOSITE-only `+` site (OB, no OT) →
    /// skipped entirely (no own-strand cytosine evidence). So only the OT-only site is
    /// counted, as Undetermined.
    #[test]
    fn single_strand_site_is_undetermined() {
        let mut d = DuplexFamilies::default();
        let k = key(b"AACCG");
        // pos 110: OT only (own at + site). pos 200: OB only (opposite at + site).
        d.add_read(
            k.clone(),
            true,
            vec![SiteObs {
                pos0: 110,
                plus: true,
                t_equivalent: true,
            }],
        );
        d.add_read(
            k.clone(),
            false,
            vec![SiteObs {
                pos0: 200,
                plus: true,
                t_equivalent: false,
            }],
        );
        let s = d.reconcile(DUPLEX_MIN_OPP_DEPTH, DEFAULT_VARIANT_OPP_FRAC);
        assert_eq!(s.duplex_paired, 1);
        assert_eq!(s.undetermined_sites, 1); // only pos 110 (OT own, no opposite)
        assert_eq!(s.methylation_sites, 0);
        assert_eq!(s.variant_sites, 0);
    }

    /// `-` strand (genomic G) site: OB is the own strand. A 5mC there → methylation.
    #[test]
    fn minus_strand_site_uses_ob_as_own() {
        let mut d = DuplexFamilies::default();
        let k = key(b"AACCG");
        // pos 140 (- strand G): OB own shows A-equiv (5mC), OT opposite shows intact.
        d.add_read(
            k.clone(),
            false,
            vec![SiteObs {
                pos0: 140,
                plus: false,
                t_equivalent: true,
            }],
        );
        d.add_read(
            k.clone(),
            true,
            vec![SiteObs {
                pos0: 140,
                plus: false,
                t_equivalent: false,
            }],
        );
        let s = d.reconcile(DUPLEX_MIN_OPP_DEPTH, DEFAULT_VARIANT_OPP_FRAC);
        assert_eq!(s.methylation_sites, 1);
        assert_eq!(s.methylated_calls, 1);
        assert_eq!(s.variant_sites, 0);
    }

    // ---- consensus_base (commit 2) ----------------------------------------

    /// Plus-strand CpG, 5mC: own (OT) shows T, opposite (OB) intact C; consensus keeps
    /// the T so the methylation call survives (NOT masked to N).
    #[test]
    fn consensus_plus_cpg_methylated_keeps_t() {
        assert_eq!(
            consensus_base(SiteKind::PlusCpG, Some((b'T', 40)), Some((b'C', 40))),
            (b'T', 40)
        );
    }

    /// Plus-strand CpG, unmethylated: own C, opposite C, consensus C.
    #[test]
    fn consensus_plus_cpg_unmethylated_keeps_c() {
        assert_eq!(
            consensus_base(SiteKind::PlusCpG, Some((b'C', 40)), Some((b'C', 40))),
            (b'C', 40)
        );
    }

    /// Plus-strand CpG, homozygous C>T variant: own T AND opposite T, masked to N
    /// (excluded from methylation) even though both strands agree on T.
    #[test]
    fn consensus_plus_cpg_variant_masks_to_n() {
        assert_eq!(
            consensus_base(SiteKind::PlusCpG, Some((b'T', 40)), Some((b'T', 40))),
            (b'N', 40)
        );
    }

    /// Minus-strand CpG, 5mC: own (OB) shows A (T-equiv for G), opposite (OT) intact G,
    /// consensus keeps A.
    #[test]
    fn consensus_minus_cpg_methylated_keeps_a() {
        assert_eq!(
            consensus_base(SiteKind::MinusCpG, Some((b'G', 40)), Some((b'A', 40))),
            (b'A', 40)
        );
    }

    /// Minus-strand CpG variant: own A and opposite A, masked to N.
    #[test]
    fn consensus_minus_cpg_variant_masks_to_n() {
        assert_eq!(
            consensus_base(SiteKind::MinusCpG, Some((b'A', 40)), Some((b'A', 40))),
            (b'N', 40)
        );
    }

    /// Generic position: agreement keeps the base (max quality); disagreement takes the
    /// higher-quality base; an equal-quality conflict → N; one-sided coverage passes through.
    #[test]
    fn consensus_generic_rules() {
        assert_eq!(
            consensus_base(SiteKind::Other, Some((b'A', 20)), Some((b'A', 35))),
            (b'A', 35)
        );
        assert_eq!(
            consensus_base(SiteKind::Other, Some((b'A', 10)), Some((b'G', 30))),
            (b'G', 30)
        );
        assert_eq!(
            consensus_base(SiteKind::Other, Some((b'A', 30)), Some((b'G', 30))),
            (b'N', 30)
        );
        assert_eq!(
            consensus_base(SiteKind::Other, Some((b'C', 25)), None),
            (b'C', 25)
        );
        assert_eq!(consensus_base(SiteKind::Other, None, None), (b'N', 0));
    }

    /// A CpG covered by only the own strand still emits the own base (its methylation
    /// call), since with no opposite strand there is no variant evidence to mask it.
    #[test]
    fn consensus_plus_cpg_own_only_keeps_call() {
        assert_eq!(
            consensus_base(SiteKind::PlusCpG, Some((b'T', 30)), None),
            (b'T', 30)
        );
    }

    #[test]
    fn report_lists_paired_family_and_footer() {
        let mut d = DuplexFamilies::default();
        let k = key(b"AACCG");
        d.add_read(
            k.clone(),
            true,
            vec![SiteObs {
                pos0: 110,
                plus: true,
                t_equivalent: true,
            }],
        );
        d.add_read(
            k.clone(),
            false,
            vec![SiteObs {
                pos0: 110,
                plus: true,
                t_equivalent: false,
            }],
        );
        // a singleton family at another locus (OT only)
        d.add_read(
            DuplexKey {
                chrom: "chr1".into(),
                start: 300,
                end: 360,
                canon_umi: b"TTTTT".to_vec(),
            },
            true,
            vec![SiteObs {
                pos0: 310,
                plus: true,
                t_equivalent: true,
            }],
        );
        let mut out = Vec::new();
        let s = d
            .write_report(&mut out, DUPLEX_MIN_OPP_DEPTH, DEFAULT_VARIANT_OPP_FRAC)
            .unwrap();
        assert_eq!(s.duplex_paired, 1);
        assert_eq!(s.singletons, 1);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("chr1\t100\t160\tAACCG\t1+1\t0\t1\t0"));
        assert!(text.contains("# families 2 duplex-paired 1 singletons 1"));
    }
}
