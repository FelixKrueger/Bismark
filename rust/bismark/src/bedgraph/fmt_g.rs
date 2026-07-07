//! Faithful reimplementation of C `printf("%.15g", x)` — the format Perl
//! uses for the methylation percentage (`bismark2bedGraph:399`/`:601`,
//! default NV stringification; the 2-dp rounding line `:602` is commented
//! out). This is THE byte-identity linchpin (SPEC §2.3).
//!
//! Approach (validated by plan-review Reviewer B against C `printf("%.15g")`
//! across 2,003,000 fractions + all scientific-notation boundary cases,
//! 0 mismatches): format with Rust's `{:.14e}` (15 significant figures),
//! parse the rounded exponent, then apply C `%g`'s style-selection and
//! trailing-zero rules.
//!
//! C `%g` with precision `P`:
//! - Let `E` be the decimal exponent of the value (the exponent `%e` would
//!   print, **after** rounding to `P` significant figures).
//! - If `E < -4` or `E >= P`: use `%e` style with `P-1` fractional digits.
//! - Otherwise: use `%f` style with `P-1-E` fractional digits.
//! - Then strip trailing zeros and a trailing `.`.
//! - The `%e` exponent is printed with a sign and at least two digits.

/// Format `x` exactly as C `printf("%.15g", x)` would. Used for the
/// methylation percentage written to bedGraph and coverage (the same
/// string for both, matching Perl's single `$meth_percentage` variable).
#[must_use]
pub fn format_g15(x: f64) -> String {
    format_g(x, 15)
}

/// General `%.*g` for precision `precision` (≥ 1). Public for testing.
#[must_use]
pub fn format_g(x: f64, precision: usize) -> String {
    // C: a precision of 0 is treated as 1.
    let p = precision.max(1);

    // C prints exactly "0" for +0.0. Methylation percentages are never
    // negative or non-finite (total ≥ 1), but handle the general shape.
    if x == 0.0 {
        return "0".to_string();
    }
    if !x.is_finite() {
        // Unreachable for meth% (total ≥ 1); defensive only.
        return format!("{x}");
    }

    // Round to P significant figures and read back the (post-rounding)
    // exponent. Rust's `{:.*e}` always prints one digit before the point.
    let sci = format!("{:.*e}", p - 1, x);
    let (mantissa, exp_str) = sci
        .split_once('e')
        .expect("Rust {:e} always contains an 'e'");
    let e: i32 = exp_str.parse().expect("Rust {:e} exponent is an integer");

    if e < -4 || e >= p as i32 {
        // Scientific style. Mantissa already carries P-1 fractional digits.
        let mant = trim_trailing_zeros(mantissa);
        let sign = if e < 0 { '-' } else { '+' };
        let eabs = e.unsigned_abs();
        format!("{mant}e{sign}{eabs:02}")
    } else {
        // Fixed style with P-1-E fractional digits.
        let frac = (p as i32 - 1 - e).max(0) as usize;
        let fixed = format!("{x:.frac$}");
        trim_trailing_zeros(&fixed).to_string()
    }
}

/// Strip trailing zeros from a decimal string, then a trailing `.`.
/// Leaves integer-looking strings untouched.
fn trim_trailing_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Empirically-verified Perl `($m/$t)*100` outputs (see SPEC §2.3 table,
    /// produced by `perl -e 'print $m/$t*100'`). These are the contract.
    #[test]
    fn matches_perl_meth_percentage_table() {
        let cases: &[(u64, u64, &str)] = &[
            (1, 2, "50"),
            (1, 1, "100"),
            (1, 3, "33.3333333333333"),
            (2, 3, "66.6666666666667"),
            (1, 7, "14.2857142857143"),
            (5, 6, "83.3333333333333"),
            (33, 99, "33.3333333333333"),
            (1, 1_000_000, "0.0001"), // decimal boundary (E = -4)
            (1, 10_000_000, "1e-05"), // scientific boundary (E = -5)
            (2, 300_000, "0.000666666666666667"),
            (0, 5, "0"), // 0% → "0"
            (5, 5, "100"),
        ];
        for &(m, t, expected) in cases {
            let pct = (m as f64 / t as f64) * 100.0;
            assert_eq!(format_g15(pct), expected, "format_g15({m}/{t}*100 = {pct})");
        }
    }

    #[test]
    fn scientific_boundary_at_exp_minus_five() {
        // 1 in 2,000,000 → 0.00005 → 5e-05 (E = -5, scientific).
        assert_eq!(format_g15(1.0 / 2_000_000.0 * 100.0), "5e-05");
        // 1 in 1,000,000 → 0.0001 → decimal (E = -4).
        assert_eq!(format_g15(1.0 / 1_000_000.0 * 100.0), "0.0001");
    }

    #[test]
    fn whole_numbers_have_no_decimal_point() {
        assert_eq!(format_g15(25.0), "25");
        assert_eq!(format_g15(10.0), "10");
        assert_eq!(format_g15(0.0), "0");
    }

    #[test]
    fn trim_helper() {
        assert_eq!(trim_trailing_zeros("5.00000000000000"), "5");
        assert_eq!(trim_trailing_zeros("33.3333333333333"), "33.3333333333333");
        assert_eq!(trim_trailing_zeros("100.000000000000"), "100");
        assert_eq!(
            trim_trailing_zeros("0.000666666666666667"),
            "0.000666666666666667"
        );
        assert_eq!(trim_trailing_zeros("100"), "100");
    }

    /// Round-trip: the formatted value parses back to within 15 sig-figs of
    /// the input. Guards against gross formatting bugs across the meth%
    /// domain (0..=100 at many coverage levels).
    #[test]
    fn round_trip_within_15_sig_figs() {
        for total in [1u64, 2, 3, 7, 13, 97, 1000, 999_983, 5_000_000] {
            for meth in 0..=total.min(50) {
                let pct = (meth as f64 / total as f64) * 100.0;
                let s = format_g15(pct);
                let back: f64 = s.parse().unwrap();
                let tol = pct.abs() * 1e-14 + 1e-300;
                assert!(
                    (back - pct).abs() <= tol,
                    "round-trip drift: {meth}/{total}*100={pct} → {s} → {back}"
                );
            }
        }
    }
}
