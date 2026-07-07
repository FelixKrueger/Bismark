//! The pure non-CG-conversion filtering decision over an XM call string.
//!
//! This module is the heart of the byte-identity contract: given a read's
//! `XM:Z:` methylation-call string and a [`FilterMode`], [`read_fails`]
//! returns whether the read (or, in PE, that mate) should be **removed**.
//! It is a faithful port of the per-character loop in Perl
//! `filter_non_conversion` (`process_file`, lines 138–176 for SE; the PE
//! mate loops at 205–245 / 252–293 are identical).
//!
//! ## Character semantics (Bismark XM alphabet)
//!
//! | char | meaning | `non_cpg_count` | `total_non_cg` | consecutive-reset |
//! |------|---------|:---------------:|:--------------:|:-----------------:|
//! | `H`  | methylated CHH | +1 | +1 | no |
//! | `X`  | methylated CHG | +1 | +1 | no |
//! | `h`  | unmethylated CHH | — | +1 | **yes** |
//! | `x`  | unmethylated CHG | — | +1 | **yes** |
//! | `z`  | unmethylated CpG | — | — | **yes** |
//! | `Z`  | methylated CpG | — | — | no (transparent) |
//! | `u`/`U` | unknown context | — | — | no (transparent) |
//! | `.`  | no call | — | — | no (transparent) |
//!
//! Only `H`/`X` increment the methylated-non-CG counter; `H`/`X`/`h`/`x`
//! increment the total-non-CG counter. In `--consecutive` mode any
//! unmethylated cytosine call (`z`/`h`/`x`) resets the methylated-non-CG
//! counter to 0 — `Z`, `u`/`U`, `.` are transparent (do NOT reset).

/// The resolved filtering mode and its parameters.
///
/// Constructed from the validated CLI (`--threshold`/`--consecutive` vs
/// `--percentage_cutoff`/`--minimum_count`). The two are mutually exclusive
/// in the Perl (`process_commandline` line 521), so a single enum models the
/// decision exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    /// Absolute-count mode (`--threshold N`, default 3). Remove when the
    /// methylated-non-CG count reaches `threshold`. With `consecutive`, any
    /// `z`/`h`/`x` resets the counter (so the threshold counts *consecutive*
    /// methylated non-CG calls).
    Threshold {
        /// The count at which the read fails (Perl default 3; validated > 0).
        /// `u64` (not `u32`) so an absurdly large `--threshold` can never wrap
        /// to a small value: any threshold above a read's call count simply
        /// never triggers, matching Perl's arbitrary-precision comparison.
        threshold: u64,
        /// `--consecutive`: reset the counter on any unmethylated cytosine call.
        consecutive: bool,
    },
    /// Percentage mode (`--percentage_cutoff P` + `--minimum_count M`).
    /// Remove when the non-CG methylation percentage (rounded to one decimal,
    /// like Perl `sprintf("%.1f", …)`) is `>=` `cutoff`, but only once the
    /// total non-CG count reaches `minimum_count`.
    Percentage {
        /// Percentage cutoff, 0–100 (validated).
        cutoff: u32,
        /// Minimum total non-CG calls before the percentage filter applies
        /// (Perl default 5; validated > 0). `u64` for the same no-wrap reason
        /// as `threshold`.
        minimum_count: u64,
    },
}

/// Round to one decimal place exactly like Perl `sprintf("%.1f", x)`.
///
/// Implemented by formatting with Rust's `{:.1}` (round-half-to-even, the
/// same rule C `printf` uses on glibc/macOS) and parsing back. This mirrors
/// Perl's behaviour of using the `%.1f` *string* in the numeric `>=`
/// comparison (`$perc >= $percentage_cutoff`, line 169) — the comparison is
/// on the **rounded** value, so e.g. 19.96 → "20.0" ≥ 20 fails.
fn round_1dp(x: f64) -> f64 {
    format!("{x:.1}")
        .parse::<f64>()
        .expect("a `{:.1}`-formatted float always parses back to f64")
}

/// Decide whether a read with the given `XM` call string fails the filter
/// (i.e. should be **removed** as apparent incomplete bisulfite conversion).
///
/// `xm` is the raw bytes of the `XM:Z:` value. An empty slice (a read with
/// no XM tag — legal in SE, see SPEC §6.1) never fails: no non-CG calls are
/// counted, so neither the threshold nor the percentage condition is met.
///
/// Faithful to Perl `filter_non_conversion`:
/// - **Threshold:** per-character, after the (optional) consecutive reset,
///   `last` (early-return) as soon as `non_cpg_count >= threshold`.
/// - **Percentage:** scan the whole string, then — only if
///   `total_non_cg >= minimum_count` — fail when the `%.1f`-rounded
///   percentage is `>= cutoff`. The per-character threshold check is *not*
///   applied in percentage mode (Perl `unless (defined $percentage_cutoff)`
///   guard, line 154).
#[must_use]
pub fn read_fails(xm: &[u8], mode: FilterMode) -> bool {
    match mode {
        FilterMode::Threshold {
            threshold,
            consecutive,
        } => {
            let mut non_cpg_count: u32 = 0;
            for &c in xm {
                if c == b'H' || c == b'X' {
                    non_cpg_count += 1;
                }
                // Consecutive reset: any unmethylated cytosine call (z/h/x)
                // zeroes the methylated-non-CG counter. Order matches Perl:
                // increment → (maybe) reset → threshold-check.
                if consecutive && (c == b'z' || c == b'h' || c == b'x') {
                    non_cpg_count = 0;
                }
                if u64::from(non_cpg_count) >= threshold {
                    return true; // Perl `last` + `$sequence_fails = 1`.
                }
            }
            false
        }
        FilterMode::Percentage {
            cutoff,
            minimum_count,
        } => {
            let mut non_cpg_count: u32 = 0;
            let mut total_non_cg: u32 = 0;
            for &c in xm {
                if c == b'H' || c == b'X' {
                    non_cpg_count += 1;
                    total_non_cg += 1;
                } else if c == b'h' || c == b'x' {
                    total_non_cg += 1;
                }
            }
            if u64::from(total_non_cg) >= minimum_count {
                // total_non_cg >= minimum_count >= 1, so the divisor is > 0
                // (Perl's "$total_nonCG is always > 0" comment, line 167).
                let perc = round_1dp(f64::from(non_cpg_count) / f64::from(total_non_cg) * 100.0);
                perc >= f64::from(cutoff)
            } else {
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T3: FilterMode = FilterMode::Threshold {
        threshold: 3,
        consecutive: false,
    };
    const T3C: FilterMode = FilterMode::Threshold {
        threshold: 3,
        consecutive: true,
    };

    // ── round_1dp parity with Perl sprintf("%.1f") ──────────────────────

    #[test]
    fn round_1dp_half_to_even_tie() {
        // 12.25 → 12.2 (round-half-to-even); both reviewers confirmed Perl
        // and Rust agree here.
        assert_eq!(round_1dp(12.25), 12.2);
        // 12.5 (1-dp tie at integer) → 12.5 (already 1 dp).
        assert_eq!(round_1dp(12.5), 12.5);
    }

    // ── Threshold mode: char semantics + boundary counts ────────────────

    #[test]
    fn empty_xm_never_fails() {
        // Absent/empty XM (SE-legal) → no non-CG calls → kept.
        assert!(!read_fails(b"", T3));
        assert!(!read_fails(
            b"",
            FilterMode::Percentage {
                cutoff: 0,
                minimum_count: 1
            }
        ));
    }

    #[test]
    fn threshold_boundary_n_minus_1_kept() {
        // Exactly 2 methylated non-CG (< 3) → kept.
        assert!(!read_fails(b"H.X.", T3));
    }

    #[test]
    fn threshold_boundary_exactly_n_removed() {
        // Exactly 3 methylated non-CG (>= 3) → removed.
        assert!(read_fails(b"HXH", T3));
    }

    #[test]
    fn threshold_boundary_n_plus_1_removed() {
        assert!(read_fails(b"HHHH", T3));
    }

    #[test]
    fn threshold_counts_only_upper_h_x() {
        // h/x/z/Z/u/U/. never increment the methylated-non-CG counter.
        assert!(!read_fails(b"hxzZuU....hhhxxx", T3));
    }

    #[test]
    fn threshold_cpg_methylated_z_does_not_count() {
        // Z (methylated CpG) is ignored — 100 Z's never fail.
        assert!(!read_fails(b"ZZZZZZZZZZ", T3));
    }

    #[test]
    fn threshold_early_exit_on_third_methylated() {
        // The decision fires at the 3rd H regardless of trailing content.
        assert!(read_fails(b"HHH<this is never scanned>", T3));
    }

    // ── Consecutive mode: reset on z/h/x; Z/u/./U transparent ───────────

    #[test]
    fn consecutive_run_of_three_fails() {
        assert!(read_fails(b"HHH", T3C));
    }

    #[test]
    fn consecutive_reset_by_lowercase_h_breaks_the_run() {
        // H H h H H — the lowercase h resets, so the max run is 2 → kept.
        assert!(!read_fails(b"HHhHH", T3C));
    }

    #[test]
    fn consecutive_reset_by_z_breaks_the_run() {
        assert!(!read_fails(b"HHzHH", T3C));
    }

    #[test]
    fn consecutive_reset_by_x_lower_breaks_the_run() {
        assert!(!read_fails(b"XXxXX", T3C));
    }

    #[test]
    fn consecutive_methylated_cpg_upper_z_is_transparent() {
        // Z does NOT reset → H H Z H is a run of 3 (the Z is skipped) → fails.
        assert!(read_fails(b"HHZH", T3C));
    }

    #[test]
    fn consecutive_dot_and_unknown_are_transparent() {
        // . and u/U do not reset → H H . u U H is a run of 3 → fails.
        assert!(read_fails(b"HH.uUH", T3C));
    }

    #[test]
    fn consecutive_vs_nonconsecutive_difference() {
        // Non-consecutive: total methylated = 3 → fails.
        assert!(read_fails(b"HhHhH", T3));
        // Consecutive: each h resets, max run 1 → kept.
        assert!(!read_fails(b"HhHhH", T3C));
    }

    // ── Percentage mode ─────────────────────────────────────────────────

    fn pct(cutoff: u32, minimum_count: u64) -> FilterMode {
        FilterMode::Percentage {
            cutoff,
            minimum_count,
        }
    }

    #[test]
    fn percentage_below_min_count_kept_even_at_100pct() {
        // 4 methylated non-CG, total 4 (< min 5) → min-count gate → kept,
        // even though it is 100% methylated. Proves the absolute threshold
        // never fires in percentage mode.
        assert!(!read_fails(b"HHHH", pct(20, 5)));
    }

    #[test]
    fn percentage_at_min_count_and_over_cutoff_removed() {
        // 5 methylated non-CG, total 5 = 100% >= 20, total >= min 5 → removed.
        assert!(read_fails(b"HHHHH", pct(20, 5)));
    }

    #[test]
    fn percentage_exactly_at_cutoff_removed() {
        // 1 H + 4 h = 1/5 = 20.0% >= cutoff 20, total 5 >= min 5 → removed.
        assert!(read_fails(b"Hhhhh", pct(20, 5)));
    }

    #[test]
    fn percentage_just_below_cutoff_kept() {
        // 3 H + 17 h = 3/20 = 15.0% < 20 → kept.
        assert!(!read_fails(b"HHHhhhhhhhhhhhhhhhhh", pct(20, 5)));
    }

    #[test]
    fn percentage_rounding_tips_over_cutoff() {
        // Construct a ratio that rounds UP to the cutoff: 1997/10000 *100? Too
        // fine. Use 1 H of 5 = 20.0 (already covered). For the tip-over,
        // 1 H + 1 h ... build 0.1996 -> not reachable with small ints. Instead
        // verify round_1dp tip-over directly via a value: 19.96 -> 20.0 >= 20.
        assert!(round_1dp(19.96) >= 20.0);
        // and 19.94 -> 19.9 < 20.
        assert!(round_1dp(19.94) < 20.0);
    }

    #[test]
    fn percentage_half_to_even_tie_at_cutoff() {
        // 5 of 40 = 12.5% exactly. cutoff 13 → 12.5 < 13 → kept (no tie ambiguity).
        // cutoff 12 → 12.5 >= 12 → removed. The exact .5 value is preserved by
        // round_1dp (already 1 dp), matching Perl.
        let xm_5_of_40 = {
            let mut v = vec![b'H'; 5];
            v.extend(std::iter::repeat_n(b'h', 35));
            v
        };
        assert!(!read_fails(&xm_5_of_40, pct(13, 5)));
        assert!(read_fails(&xm_5_of_40, pct(12, 5)));
    }

    #[test]
    fn percentage_zero_cutoff_with_any_non_cg_removed() {
        // cutoff 0: any read meeting the min-count is removed (0% >= 0).
        assert!(read_fails(b"hhhhh", pct(0, 5))); // 0% but 0 >= 0 → removed
    }

    #[test]
    fn percentage_zero_cutoff_below_min_count_kept() {
        // Below min count → kept regardless of cutoff 0.
        assert!(!read_fails(b"hhh", pct(0, 5)));
    }
}
