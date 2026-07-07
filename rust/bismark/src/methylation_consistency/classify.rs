//! Read-level methylation-consistency classification — the core algorithm.
//!
//! Mirrors Perl `methylation_consistency` lines 259–301. The flow per read
//! (PE: per pair, counts summed across both mates):
//!
//! 1. Count methylated / unmethylated cytosine calls in the `XM:Z:` string.
//! 2. If `meth + unmeth < min_count` → **Discard** (`++$discarded_count`).
//! 3. If `meth + unmeth == 0` (only reachable when `min_count == 0`) →
//!    **Skip** (Perl `next`; counted in no bucket).
//! 4. Compute `percent_methylated = sprintf("%.1f", meth/total*100)` and
//!    **compare the rounded value** (not the raw fraction) to the integer
//!    thresholds:
//!    - `<= lower` → AllUnmeth, `>= upper` → AllMeth, else Mixed.
//!
//! **Round-then-compare is load-bearing** (SPEC §2.5): a read near 10.04%
//! rounds to `"10.0"` → unmethylated, but near 10.05% rounds to `"10.1"` →
//! mixed. We format to one decimal, parse back, then compare — exactly
//! mirroring Perl's "stringify then numeric-compare". Spike 1 confirmed Rust
//! `{:.1}` is decision-identical to Perl `sprintf` (both round-half-to-even
//! on the same `f64`), **given the pinned op-order** `meth/total*100`.

/// Methylated / unmethylated cytosine-call counts for one read (or, in PE
/// mode, a read pair — see [`Counts::add`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// Methylated calls: count of `Z` (CpG) or `H` (CHH).
    pub meth: u32,
    /// Unmethylated calls: count of `z` (CpG) or `h` (CHH).
    pub unmeth: u32,
}

impl Counts {
    /// Total cytosine calls considered (`meth + unmeth`).
    #[must_use]
    pub fn total(&self) -> u32 {
        self.meth + self.unmeth
    }
}

impl std::ops::Add for Counts {
    type Output = Counts;
    /// Sum two reads' counts — used to combine R1 + R2 in paired-end mode
    /// (Perl simply adds both mates' `Z`/`z` counts: lines 242–248).
    fn add(self, rhs: Counts) -> Counts {
        Counts {
            meth: self.meth + rhs.meth,
            unmeth: self.unmeth + rhs.unmeth,
        }
    }
}

/// One of the three consistency buckets a read can be routed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    /// Consistently methylated (`>= upper_threshold`).
    AllMeth,
    /// Consistently unmethylated (`<= lower_threshold`).
    AllUnmeth,
    /// Mixed methylation (between the thresholds).
    Mixed,
}

/// What to do with a read after classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routing {
    /// Too few cytosine calls (`total < min_count`) — increment the
    /// discarded counter (Perl line 259–262).
    Discard,
    /// Zero calls with `min_count == 0` — counted in NO bucket (Perl's
    /// `next` at line 269; only reachable when `min_count == 0`).
    Skip,
    /// Route the read(s) to a bucket and increment that bucket's counter.
    Route(Bucket),
}

/// Count methylation calls in an `XM:Z:` string.
///
/// CpG (default): `meth` = count of `Z`, `unmeth` = count of `z`.
/// CHH (`chh = true`): `meth` = count of `H`, `unmeth` = count of `h`.
/// All other bytes (`.`, `x`, `X`, `u`, `U`, and the off-context pair) are
/// ignored — matching Perl's `tr/Z//` / `tr/z//` (or `tr/H//` / `tr/h//`).
#[must_use]
pub fn count_xm(xm: &[u8], chh: bool) -> Counts {
    let (meth_byte, unmeth_byte) = if chh { (b'H', b'h') } else { (b'Z', b'z') };
    let mut counts = Counts::default();
    for &b in xm {
        if b == meth_byte {
            counts.meth += 1;
        } else if b == unmeth_byte {
            counts.unmeth += 1;
        }
    }
    counts
}

/// Classify a read's counts into a [`Routing`] decision.
///
/// `lower`/`upper` are the (validated) integer thresholds (Perl defaults
/// 10 / 90). See the module docs for the round-then-compare contract.
#[must_use]
pub fn classify(counts: Counts, min_count: u32, lower: i64, upper: i64) -> Routing {
    let total = counts.total();
    if total < min_count {
        return Routing::Discard;
    }
    if total == 0 {
        // Only reachable when min_count == 0 (else `total < min_count` caught
        // it above). Perl line 269: `next` — counted in no bucket.
        return Routing::Skip;
    }
    let pct = rounded_percent(counts.meth, total);
    if pct <= lower as f64 {
        Routing::Route(Bucket::AllUnmeth)
    } else if pct >= upper as f64 {
        Routing::Route(Bucket::AllMeth)
    } else {
        Routing::Route(Bucket::Mixed)
    }
}

/// `sprintf("%.1f", meth/total*100)` parsed back to an `f64`.
///
/// The op-order `meth as f64 / total as f64 * 100.0` is **pinned**: Spike 1
/// proved Perl/Rust parity holds because the underlying `f64` is computed
/// identically. `total` must be non-zero (callers gate on it).
#[must_use]
fn rounded_percent(meth: u32, total: u32) -> f64 {
    format!("{:.1}", meth as f64 / total as f64 * 100.0)
        .parse()
        .expect("a value formatted with {:.1} always parses back to f64")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_xm_cpg_counts_z_and_lowercase_z_only() {
        // `.` unmethylated non-CpG, `x`/`X` CHG, `h`/`H` CHH, `u`/`U` unknown
        // must all be ignored in CpG mode. The string has `Z`×2 and `z`×2.
        let c = count_xm(b"Z.z.X.x.H.h.U.u.Zz", false);
        assert_eq!(c, Counts { meth: 2, unmeth: 2 });
    }

    #[test]
    fn count_xm_chh_counts_h_and_lowercase_h_only() {
        let c = count_xm(b"Z.z.X.x.H.h.H.h.", true);
        assert_eq!(c, Counts { meth: 2, unmeth: 2 });
    }

    #[test]
    fn count_xm_empty_is_zero() {
        assert_eq!(count_xm(b"", false), Counts { meth: 0, unmeth: 0 });
    }

    #[test]
    fn counts_add_combines_mates() {
        let r1 = Counts { meth: 3, unmeth: 1 };
        let r2 = Counts { meth: 2, unmeth: 4 };
        assert_eq!(r1 + r2, Counts { meth: 5, unmeth: 5 });
    }

    // ── discard / skip gates ──────────────────────────────────────────

    #[test]
    fn below_min_count_is_discarded() {
        // total 4 < 5 → Discard (default min_count).
        assert_eq!(
            classify(Counts { meth: 3, unmeth: 1 }, 5, 10, 90),
            Routing::Discard
        );
    }

    #[test]
    fn zero_calls_with_default_min_count_is_discarded_not_skipped() {
        // total 0 < 5 → Discard (NOT Skip — Skip only when min_count == 0).
        assert_eq!(
            classify(Counts { meth: 0, unmeth: 0 }, 5, 10, 90),
            Routing::Discard
        );
    }

    #[test]
    fn zero_calls_with_min_count_zero_is_skipped() {
        // min_count 0: total 0 not < 0; then total == 0 → Skip (Perl line 269).
        assert_eq!(
            classify(Counts { meth: 0, unmeth: 0 }, 0, 10, 90),
            Routing::Skip
        );
    }

    #[test]
    fn nonzero_calls_with_min_count_zero_classifies_normally() {
        assert_eq!(
            classify(Counts { meth: 2, unmeth: 3 }, 0, 10, 90),
            Routing::Route(Bucket::Mixed)
        );
    }

    // ── inclusive boundaries (rounded value exactly on a threshold) ────

    #[test]
    fn exactly_lower_threshold_is_all_unmeth() {
        // 1/10 → 10.0% → "10.0" → 10.0 <= 10 → AllUnmeth (inclusive).
        assert_eq!(
            classify(Counts { meth: 1, unmeth: 9 }, 5, 10, 90),
            Routing::Route(Bucket::AllUnmeth)
        );
    }

    #[test]
    fn exactly_upper_threshold_is_all_meth() {
        // 9/10 → 90.0% → "90.0" → 90.0 >= 90 → AllMeth (inclusive).
        assert_eq!(
            classify(Counts { meth: 9, unmeth: 1 }, 5, 10, 90),
            Routing::Route(Bucket::AllMeth)
        );
    }

    #[test]
    fn mid_range_is_mixed() {
        // 5/10 → 50.0% → Mixed.
        assert_eq!(
            classify(Counts { meth: 5, unmeth: 5 }, 5, 10, 90),
            Routing::Route(Bucket::Mixed)
        );
    }

    // ── round-THEN-compare crossing the lower threshold ───────────────

    #[test]
    fn round_then_compare_10_04_rounds_down_to_unmeth() {
        // 1004/10000 = 10.04% → "10.0" → 10.0 <= 10 → AllUnmeth.
        assert_eq!(
            classify(
                Counts {
                    meth: 1004,
                    unmeth: 8996
                },
                5,
                10,
                90
            ),
            Routing::Route(Bucket::AllUnmeth)
        );
    }

    #[test]
    fn round_then_compare_10_05_rounds_up_to_mixed() {
        // 1005/10000 = 10.05% → "10.1" → 10.1 > 10 → Mixed.
        // This is the load-bearing case: comparing the RAW fraction (10.05)
        // to lower=10 would (wrongly) give AllUnmeth.
        assert_eq!(
            classify(
                Counts {
                    meth: 1005,
                    unmeth: 8995
                },
                5,
                10,
                90
            ),
            Routing::Route(Bucket::Mixed)
        );
    }

    // ── power-of-two ties (round-half-to-even); classification robust ──
    // Spike 1 proved Rust `{:.1}` matches Perl `sprintf` on these exact
    // f64 values; here we assert the (rounding-direction-robust) bucket.

    #[test]
    fn tie_6_25_percent_is_all_unmeth() {
        // 1/16 = 6.25% → "6.2"/"6.3", both <= 10 → AllUnmeth.
        assert_eq!(
            classify(
                Counts {
                    meth: 1,
                    unmeth: 15
                },
                5,
                10,
                90
            ),
            Routing::Route(Bucket::AllUnmeth)
        );
    }

    #[test]
    fn tie_12_5_percent_is_mixed() {
        // 1/8 = 12.5% → "12.5" → Mixed.
        assert_eq!(
            classify(Counts { meth: 1, unmeth: 7 }, 5, 10, 90),
            Routing::Route(Bucket::Mixed)
        );
    }

    #[test]
    fn tie_87_5_percent_is_mixed() {
        // 7/8 = 87.5% → "87.5" → Mixed.
        assert_eq!(
            classify(Counts { meth: 7, unmeth: 1 }, 5, 10, 90),
            Routing::Route(Bucket::Mixed)
        );
    }

    #[test]
    fn tie_90_05_percent_is_all_meth() {
        // 1801/2000 = 90.05% → "90.0" or "90.1" — both >= 90 → AllMeth.
        assert_eq!(
            classify(
                Counts {
                    meth: 1801,
                    unmeth: 199
                },
                5,
                10,
                90
            ),
            Routing::Route(Bucket::AllMeth)
        );
    }

    // ── custom thresholds ─────────────────────────────────────────────

    #[test]
    fn respects_custom_thresholds() {
        // lower=20, upper=80; 25% → Mixed, 15% → AllUnmeth, 85% → AllMeth.
        assert_eq!(
            classify(
                Counts {
                    meth: 25,
                    unmeth: 75
                },
                5,
                20,
                80
            ),
            Routing::Route(Bucket::Mixed)
        );
        assert_eq!(
            classify(
                Counts {
                    meth: 15,
                    unmeth: 85
                },
                5,
                20,
                80
            ),
            Routing::Route(Bucket::AllUnmeth)
        );
        assert_eq!(
            classify(
                Counts {
                    meth: 85,
                    unmeth: 15
                },
                5,
                20,
                80
            ),
            Routing::Route(Bucket::AllMeth)
        );
    }
}
