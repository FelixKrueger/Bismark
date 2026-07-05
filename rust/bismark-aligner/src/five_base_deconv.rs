//! #787 Illumina 5-Base variant-vs-methylation DECONVOLUTION (the SNP-aware caller).
//!
//! The 5-Base SE/PE drivers call methylation per read: a read `T` at a genomic C is
//! "methylated". That is SNP-naive — a genuine **C>T genetic variant** also reads as
//! `T` and would be miscalled as 5mC. DRAGEN deconvolutes the two using the opposite
//! strand ("for a methylated cytosine the complementary base is G; for a C>T variant
//! it is A"). This module reproduces that, post-alignment, over the 5-Base BAM.
//!
//! ## The signal (why it works, and why it is checkable)
//!
//! At a reference `C` (the `+`-strand cytosine of a CpG), with both reads stored in
//! the SAM **forward (reference) orientation**:
//! - reads from the **OT** strand show the `+`-strand base directly: `C` (unmethylated)
//!   or `T` (5mC **or** a C>T variant);
//! - reads from the **OB** strand show, in forward orientation, the *complement of the
//!   `-`-strand base*: for an intact `C:G` pair the `-` strand is `G`, whose complement
//!   is `C`, so OB reads show **`C`**; for a homozygous C>T variant the locus is `T:A`,
//!   the `-` strand is `A`, whose complement is `T`, so OB reads show **`T`**.
//!
//! So **methylation moves only the OT base** (C→T), while a **genetic C>T moves BOTH
//! strands**. If the OB reads at a reference `C` show `T`, the cytosine is gone on both
//! strands ⇒ a C>T variant, and the OT `T`s there are NOT 5mC. This is a clean,
//! depth-gated decision that synthetic ground truth can confirm without DRAGEN.
//!
//! This module is feature-independent and pure (no I/O): the [`StrandPileup`] counters
//! are filled by the caller's BAM walk; [`StrandPileup::classify`] makes the call.

/// Per-cytosine two-strand base tally. "own" = the strand carrying this cytosine
/// (OT for a genomic `C`, OB for a genomic `G`); "opposite" = the other strand. A
/// "T-equivalent" base is the converted/variant allele (`T` opposite a genomic `C`,
/// `A` opposite a genomic `G`); "C-equivalent" is the intact allele. Stating the
/// counts in own/opposite terms lets ONE classifier serve both strands of a CpG.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StrandPileup {
    /// own-strand reads showing the intact base (unmethylated cytosine).
    pub own_c: u32,
    /// own-strand reads showing the T-equivalent (5mC **or** the variant allele).
    pub own_t: u32,
    /// opposite-strand reads showing the intact base ⇒ the pair is intact ⇒ methylation.
    pub opp_c: u32,
    /// opposite-strand reads showing the T-equivalent ⇒ the cytosine is gone on BOTH
    /// strands ⇒ a genetic variant (C>T / G>A), not 5mC.
    pub opp_t: u32,
}

/// The deconvoluted verdict at a reference cytosine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CytosineVerdict {
    /// A C>T (or G>A on the `-` strand) genetic variant: the cytosine is absent on
    /// BOTH strands; any `T` here is the variant allele, NOT 5mC. Excluded from
    /// methylation.
    Variant,
    /// A methylation locus (the `C:G` pair is intact): `methylated` / `total` are the
    /// OT-strand 5mC evidence (`ot_t` methylated of `ot_c + ot_t` informative).
    Methylation { methylated: u32, total: u32 },
    /// Too little opposite-strand coverage to deconvolute confidently (kept as a
    /// methylation locus by the caller, but flagged low-confidence).
    Undetermined { methylated: u32, total: u32 },
}

impl StrandPileup {
    /// Add one observation. `own_strand` = the read is on this cytosine's own strand
    /// (OT for a genomic `C`, OB for a genomic `G`); `t_equivalent` = the read shows the
    /// converted/variant allele (`T` for a `C`, `A` for a `G`) rather than the intact base.
    pub fn observe(&mut self, own_strand: bool, t_equivalent: bool) {
        match (own_strand, t_equivalent) {
            (true, false) => self.own_c += 1,
            (true, true) => self.own_t += 1,
            (false, false) => self.opp_c += 1,
            (false, true) => self.opp_t += 1,
        }
    }

    /// Total informative opposite-strand depth (the deconvolution evidence).
    fn opp_total(&self) -> u32 {
        self.opp_c + self.opp_t
    }

    /// Classify this cytosine.
    ///
    /// - **Variant** when the opposite strand has ≥ `min_opp_depth` informative reads
    ///   AND its T-equivalent fraction is ≥ `variant_opp_frac` (the cytosine is gone on
    ///   the opposite strand too ⇒ a genetic variant).
    /// - **Methylation** otherwise, when there is enough opposite-strand evidence of an
    ///   intact pair.
    /// - **Undetermined** when opposite coverage is below `min_opp_depth`.
    ///
    /// `methylated`/`total` always report the own-strand 5mC evidence (`own_t` of
    /// `own_c + own_t`).
    pub fn classify(&self, min_opp_depth: u32, variant_opp_frac: f64) -> CytosineVerdict {
        let total = self.own_c + self.own_t;
        if self.opp_total() < min_opp_depth {
            return CytosineVerdict::Undetermined {
                methylated: self.own_t,
                total,
            };
        }
        let opp_t_frac = self.opp_t as f64 / self.opp_total() as f64;
        if opp_t_frac >= variant_opp_frac {
            CytosineVerdict::Variant
        } else {
            CytosineVerdict::Methylation {
                methylated: self.own_t,
                total,
            }
        }
    }
}

/// Default deconvolution thresholds (conservative): need ≥2 informative opposite-strand
/// reads, and call a variant when ≥60% of them show the T-equivalent.
pub const DEFAULT_MIN_OPP_DEPTH: u32 = 2;
pub const DEFAULT_VARIANT_OPP_FRAC: f64 = 0.6;

use std::collections::BTreeMap;
use std::io::Write;

/// One CpG-cytosine site: its two-strand tally and which strand carries the cytosine
/// (`+` for a genomic `C`, `-` for a genomic `G`).
#[derive(Debug, Clone, Copy)]
struct Site {
    pileup: StrandPileup,
    plus: bool,
}

/// Accumulates per-CpG-cytosine two-strand tallies across a 5-Base BAM, then
/// deconvolutes each site (variant vs methylation) and writes a report. Keyed by
/// `(chromosome, 0-based position)` in `BTreeMap` order for deterministic output.
#[derive(Debug, Default)]
pub struct Deconvoluter {
    sites: BTreeMap<(String, u32), Site>,
}

/// Summary of a deconvolution run (also the unit-test surface).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DeconvSummary {
    /// CpG cytosines classified as a genetic variant (excluded from methylation).
    pub variants: u64,
    /// CpG cytosines kept as methylation loci (intact pair, enough opposite evidence).
    pub methylation_sites: u64,
    /// CpG cytosines with too little opposite-strand coverage to deconvolute.
    pub undetermined: u64,
    /// Methylated calls summed over the kept (non-variant) sites.
    pub methylated_calls: u64,
    /// Informative calls summed over the kept (non-variant) sites.
    pub total_calls: u64,
}

impl Deconvoluter {
    /// Record one read base over a CpG cytosine. `plus` = the cytosine is the genomic
    /// `C` (own strand OT) vs the genomic `G` (own strand OB). `own` = the read is on
    /// that own strand. `t_equivalent` = the read shows the converted/variant allele.
    pub fn observe(&mut self, chrom: &str, pos0: u32, plus: bool, own: bool, t_equivalent: bool) {
        let site = self.sites.entry((chrom.to_string(), pos0)).or_insert(Site {
            pileup: StrandPileup::default(),
            plus,
        });
        site.pileup.observe(own, t_equivalent);
    }

    /// Deconvolute every site and write a per-site report; returns the summary.
    /// Columns: chromosome, 1-based position, strand, verdict, methylated, total, %.
    pub fn write_report<W: Write>(
        &self,
        w: &mut W,
        min_opp_depth: u32,
        variant_opp_frac: f64,
    ) -> std::io::Result<DeconvSummary> {
        writeln!(
            w,
            "# Illumina 5-Base variant/methylation deconvolution (#787)\n\
             # columns: chromosome\tposition(1-based)\tstrand\tverdict\tmethylated\ttotal\tpercent"
        )?;
        let mut s = DeconvSummary::default();
        for ((chrom, pos0), site) in &self.sites {
            let verdict = site.pileup.classify(min_opp_depth, variant_opp_frac);
            let strand = if site.plus { '+' } else { '-' };
            let pos1 = pos0 + 1;
            let (label, meth, total) = match verdict {
                CytosineVerdict::Variant => {
                    s.variants += 1;
                    ("variant", 0, 0)
                }
                CytosineVerdict::Methylation { methylated, total } => {
                    s.methylation_sites += 1;
                    s.methylated_calls += methylated as u64;
                    s.total_calls += total as u64;
                    ("methylation", methylated, total)
                }
                CytosineVerdict::Undetermined { methylated, total } => {
                    s.undetermined += 1;
                    s.methylated_calls += methylated as u64;
                    s.total_calls += total as u64;
                    ("undetermined", methylated, total)
                }
            };
            let pct = if total > 0 {
                format!("{:.2}", 100.0 * meth as f64 / total as f64)
            } else {
                "NA".to_string()
            };
            writeln!(
                w,
                "{chrom}\t{pos1}\t{strand}\t{label}\t{meth}\t{total}\t{pct}"
            )?;
        }
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Methylation locus: own-strand reads carry the 5mC signal (T-equiv), opposite
    /// reads show the intact pair (C-equiv). Verdict = Methylation; NOT a variant.
    #[test]
    fn classify_methylation_when_opposite_is_intact() {
        let p = StrandPileup {
            own_c: 2,
            own_t: 8, // 80% methylated on the own strand
            opp_c: 6, // opposite intact ⇒ C-equivalent
            opp_t: 0,
        };
        assert_eq!(
            p.classify(DEFAULT_MIN_OPP_DEPTH, DEFAULT_VARIANT_OPP_FRAC),
            CytosineVerdict::Methylation {
                methylated: 8,
                total: 10
            }
        );
    }

    /// Homozygous variant: the cytosine is gone on BOTH strands → own all T-equiv,
    /// opposite all T-equiv. Verdict = Variant (the own T-equivs are NOT 5mC).
    #[test]
    fn classify_variant_when_opposite_is_t() {
        let p = StrandPileup {
            own_c: 0,
            own_t: 9,
            opp_c: 0,
            opp_t: 7,
        };
        assert_eq!(
            p.classify(DEFAULT_MIN_OPP_DEPTH, DEFAULT_VARIANT_OPP_FRAC),
            CytosineVerdict::Variant
        );
    }

    /// Below the opposite-depth floor → Undetermined, but the own 5mC evidence is still
    /// reported.
    #[test]
    fn classify_undetermined_without_opposite_evidence() {
        let p = StrandPileup {
            own_c: 1,
            own_t: 4,
            opp_c: 1,
            opp_t: 0, // opp_total = 1 < 2
        };
        assert_eq!(
            p.classify(DEFAULT_MIN_OPP_DEPTH, DEFAULT_VARIANT_OPP_FRAC),
            CytosineVerdict::Undetermined {
                methylated: 4,
                total: 5
            }
        );
    }

    /// A partial opposite T-equiv fraction below the threshold stays Methylation (a
    /// stray sequencing error on the opposite strand does not flip a real locus).
    #[test]
    fn classify_methylation_when_opposite_t_below_fraction() {
        let p = StrandPileup {
            own_c: 3,
            own_t: 7,
            opp_c: 8,
            opp_t: 2, // 20% < 60% → not a variant
        };
        assert_eq!(
            p.classify(DEFAULT_MIN_OPP_DEPTH, DEFAULT_VARIANT_OPP_FRAC),
            CytosineVerdict::Methylation {
                methylated: 7,
                total: 10
            }
        );
    }

    #[test]
    fn observe_routes_bases_by_strand() {
        let mut p = StrandPileup::default();
        p.observe(true, false); // own, intact
        p.observe(true, true); // own, T-equiv
        p.observe(false, false); // opposite, intact
        p.observe(false, true); // opposite, T-equiv
        assert_eq!(
            p,
            StrandPileup {
                own_c: 1,
                own_t: 1,
                opp_c: 1,
                opp_t: 1
            }
        );
    }

    /// The accumulator + report: a methylation CpG (own T, opposite intact) and a
    /// variant CpG (both strands T-equiv) at two positions → one of each in the summary,
    /// and the variant contributes nothing to the methylation totals.
    #[test]
    fn deconvoluter_summary_separates_variant_from_methylation() {
        let mut d = Deconvoluter::default();
        // pos 100 (+ strand C): methylation — 4 own T (5mC), 3 opposite intact.
        for _ in 0..4 {
            d.observe("chr1", 100, true, true, true);
        }
        d.observe("chr1", 100, true, true, false); // 1 own unmethylated
        for _ in 0..3 {
            d.observe("chr1", 100, true, false, false); // opposite intact
        }
        // pos 200 (+ strand C): homozygous variant — both strands all T-equiv.
        for _ in 0..5 {
            d.observe("chr1", 200, true, true, true);
        }
        for _ in 0..4 {
            d.observe("chr1", 200, true, false, true); // opposite T-equiv ⇒ variant
        }
        let mut out = Vec::new();
        let s = d
            .write_report(&mut out, DEFAULT_MIN_OPP_DEPTH, DEFAULT_VARIANT_OPP_FRAC)
            .unwrap();
        assert_eq!(s.variants, 1);
        assert_eq!(s.methylation_sites, 1);
        assert_eq!(s.methylated_calls, 4); // only the methylation site's 4 own T
        assert_eq!(s.total_calls, 5); // 4 T + 1 C at the methylation site
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("chr1\t101\t+\tmethylation\t4\t5\t80.00"));
        assert!(text.contains("chr1\t201\t+\tvariant\t0\t0\tNA"));
    }
}
