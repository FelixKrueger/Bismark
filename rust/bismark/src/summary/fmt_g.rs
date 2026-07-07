//! Faithful reimplementation of C `printf("%.15g", x)` — the format Perl
//! uses for default scalar (NV) stringification.
//!
//! In `bismark2summary` this powers the **unmethylated** percentage arrays
//! (SPEC §2.9a): `$p_*_unmeth = 100 - $p_*_meth`, where `$p_*_meth` is an
//! already-rounded `%.2f` string. Perl numifies that string, subtracts from
//! integer `100`, and stringifies the result with default `%.15g` —
//! **dropping trailing zeros** (`100 - "50.00"` → `"50"`, `100 - "100.00"`
//! → `"0"`, `100 - "12.30"` → `"87.7"`). The methylated arrays keep their
//! `%.2f` form verbatim, so the two are asymmetric.
//!
//! Copied verbatim (duplicate-not-couple, SPEC §3 / O2) from
//! `bismark-bedgraph/src/fmt_g.rs`, where it was validated against C
//! `printf("%.15g")` across 2,003,000 fractions + scientific-notation
//! boundary cases (0 mismatches), and re-confirmed bit-exact against Perl on
//! the `100 - "99.99"` → `"0.0100000000000051"` artifact by both
//! `bismark2summary` plan-reviewers.
//!
//! C `%g` with precision `P`:
//! - Let `E` be the decimal exponent of the value (the exponent `%e` would
//!   print, **after** rounding to `P` significant figures).
//! - If `E < -4` or `E >= P`: use `%e` style with `P-1` fractional digits.
//! - Otherwise: use `%f` style with `P-1-E` fractional digits.
//! - Then strip trailing zeros and a trailing `.`.
//! - The `%e` exponent is printed with a sign and at least two digits.

/// Format `x` exactly as C `printf("%.15g", x)` would.
#[must_use]
pub fn format_g15(x: f64) -> String {
    format_g(x, 15)
}

/// General `%.*g` for precision `precision` (≥ 1). Public for testing.
#[must_use]
pub fn format_g(x: f64, precision: usize) -> String {
    // C: a precision of 0 is treated as 1.
    let p = precision.max(1);

    // C prints exactly "0" for +0.0.
    if x == 0.0 {
        return "0".to_string();
    }
    if !x.is_finite() {
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

    /// The `100 - %.2f` complement (the unmethylated-percentage contract,
    /// §2.9a): round to `%.2f`, re-parse, subtract from 100, `%.15g`.
    #[test]
    fn unmeth_complement_drops_trailing_zeros() {
        let complement = |meth_pct_2dp: &str| {
            let m: f64 = meth_pct_2dp.parse().unwrap();
            format_g15(100.0 - m)
        };
        assert_eq!(complement("50.00"), "50");
        assert_eq!(complement("100.00"), "0");
        assert_eq!(complement("0.00"), "100");
        assert_eq!(complement("12.34"), "87.66");
        assert_eq!(complement("12.30"), "87.7");
        // The FP-artifact case both reviewers verified bit-exact vs Perl.
        assert_eq!(complement("99.99"), "0.0100000000000051");
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
        assert_eq!(trim_trailing_zeros("87.7000000000000"), "87.7");
        assert_eq!(trim_trailing_zeros("100"), "100");
    }
}
