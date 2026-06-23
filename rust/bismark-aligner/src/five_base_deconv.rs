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

/// Per-reference-`C` two-strand base tally (forward-orientation SAM bases).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StrandPileup {
    /// OT (`+`-strand-origin) reads showing `C` at this reference C (unmethylated).
    pub ot_c: u32,
    /// OT reads showing `T` (5mC **or** the C>T variant allele).
    pub ot_t: u32,
    /// OB (`-`-strand-origin) reads showing `C` (forward orientation ⇒ intact `C:G`).
    pub ob_c: u32,
    /// OB reads showing `T` (forward orientation ⇒ the `-` strand lost its `G` ⇒ C>T).
    pub ob_t: u32,
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
    /// Add one observation from a read covering this reference `C`.
    pub fn observe(&mut self, ot_strand: bool, base: u8) {
        match (ot_strand, base.to_ascii_uppercase()) {
            (true, b'C') => self.ot_c += 1,
            (true, b'T') => self.ot_t += 1,
            (false, b'C') => self.ob_c += 1,
            (false, b'T') => self.ob_t += 1,
            _ => {} // other bases (A/G/N) are uninformative for the C/T decision
        }
    }

    /// Total informative OB-strand depth (the deconvolution evidence).
    fn ob_total(&self) -> u32 {
        self.ob_c + self.ob_t
    }

    /// Classify this cytosine.
    ///
    /// - **Variant** when the OB strand has at least `min_ob_depth` informative reads
    ///   AND the OB `T` fraction is ≥ `variant_ob_frac` (the cytosine is gone on the
    ///   `-` strand too).
    /// - **Methylation** otherwise, when there is enough OB evidence of an intact pair.
    /// - **Undetermined** when OB coverage is below `min_ob_depth` (cannot deconvolute).
    ///
    /// `methylated`/`total` always report the OT-strand 5mC evidence (`ot_t` of
    /// `ot_c + ot_t`) so the caller can tally a methylation level when the verdict is
    /// not `Variant`.
    pub fn classify(&self, min_ob_depth: u32, variant_ob_frac: f64) -> CytosineVerdict {
        let total = self.ot_c + self.ot_t;
        if self.ob_total() < min_ob_depth {
            return CytosineVerdict::Undetermined {
                methylated: self.ot_t,
                total,
            };
        }
        let ob_t_frac = self.ob_t as f64 / self.ob_total() as f64;
        if ob_t_frac >= variant_ob_frac {
            CytosineVerdict::Variant
        } else {
            CytosineVerdict::Methylation {
                methylated: self.ot_t,
                total,
            }
        }
    }
}

/// Default deconvolution thresholds (conservative): need ≥2 informative OB reads, and
/// call a variant when ≥60% of them show `T`.
pub const DEFAULT_MIN_OB_DEPTH: u32 = 2;
pub const DEFAULT_VARIANT_OB_FRAC: f64 = 0.6;

#[cfg(test)]
mod tests {
    use super::*;

    /// Methylation locus: OT reads carry the 5mC signal (`T`), OB reads show the intact
    /// pair (`C`). Verdict = Methylation; NOT a variant.
    #[test]
    fn classify_methylation_when_ob_is_c() {
        let p = StrandPileup {
            ot_c: 2,
            ot_t: 8, // 80% methylated on the OT strand
            ob_c: 6, // OB intact ⇒ C
            ob_t: 0,
        };
        assert_eq!(
            p.classify(DEFAULT_MIN_OB_DEPTH, DEFAULT_VARIANT_OB_FRAC),
            CytosineVerdict::Methylation {
                methylated: 8,
                total: 10
            }
        );
    }

    /// Homozygous C>T variant: the cytosine is gone on BOTH strands → OT all `T`, OB all
    /// `T` (forward orientation). Verdict = Variant (the OT `T`s are NOT 5mC).
    #[test]
    fn classify_variant_when_ob_is_t() {
        let p = StrandPileup {
            ot_c: 0,
            ot_t: 9,
            ob_c: 0,
            ob_t: 7,
        };
        assert_eq!(
            p.classify(DEFAULT_MIN_OB_DEPTH, DEFAULT_VARIANT_OB_FRAC),
            CytosineVerdict::Variant
        );
    }

    /// Below the OB-depth floor → Undetermined (cannot deconvolute), but the OT 5mC
    /// evidence is still reported.
    #[test]
    fn classify_undetermined_without_ob_evidence() {
        let p = StrandPileup {
            ot_c: 1,
            ot_t: 4,
            ob_c: 1,
            ob_t: 0, // ob_total = 1 < 2
        };
        assert_eq!(
            p.classify(DEFAULT_MIN_OB_DEPTH, DEFAULT_VARIANT_OB_FRAC),
            CytosineVerdict::Undetermined {
                methylated: 4,
                total: 5
            }
        );
    }

    /// A partial OB `T` fraction below the variant threshold stays Methylation (e.g. a
    /// stray sequencing error on the OB strand does not flip a real methylation locus).
    #[test]
    fn classify_methylation_when_ob_t_below_fraction() {
        let p = StrandPileup {
            ot_c: 3,
            ot_t: 7,
            ob_c: 8,
            ob_t: 2, // 20% < 60% → not a variant
        };
        assert_eq!(
            p.classify(DEFAULT_MIN_OB_DEPTH, DEFAULT_VARIANT_OB_FRAC),
            CytosineVerdict::Methylation {
                methylated: 7,
                total: 10
            }
        );
    }

    #[test]
    fn observe_routes_bases_by_strand() {
        let mut p = StrandPileup::default();
        p.observe(true, b'C');
        p.observe(true, b't'); // lowercase tolerated
        p.observe(false, b'C');
        p.observe(false, b'T');
        p.observe(true, b'A'); // uninformative
        assert_eq!(
            p,
            StrandPileup {
                ot_c: 1,
                ot_t: 1,
                ob_c: 1,
                ob_t: 1
            }
        );
    }
}
