//! MAPQ computation — a verbatim port of Perl `calc_mapq` (3923–4186),
//! **end-to-end branch only** (`--local` is rejected in v1).
//!
//! The returned MAPQ integers are **byte-identity-critical** — they land in the
//! BAM MAPQ column (Phase 5). Reviewer A verified Perl 5 and rustc 1.95 produce
//! **bit-identical `f64`** for this arithmetic, so the exact `==`/`>=` float
//! comparisons are intentional (an epsilon comparison would break parity).

/// Bismark MAPQ. `read2_len` is `Some` only for paired-end; single-end passes
/// `None`. `intercept`/`slope` are the `--score_min` parameters (end-to-end
/// default `0.0`/`-0.2`; local default `20.0`/`8.0`). `local` selects the
/// `--local` branch: `scMin = intercept + slope·ln(readLen)` (vs the linear
/// end-to-end form) and the separate local MAPQ ladder (Perl `4082-4178`).
/// **`local` is bit-safe** — the Phase-0 spike (`plans/06132026_aligner-local-mode/
/// spikes/`) proved Perl `log` ≡ Rust `f64::ln()` bit-identical on the gate arch,
/// so the exact `==`/`>=` `f64` comparisons hold for both branches.
#[allow(clippy::float_cmp)] // exact f64 equality matches Perl `$bestOver == $diff` (verified bit-identical)
pub fn calc_mapq(
    read1_len: usize,
    read2_len: Option<usize>,
    as_best: i64,
    as_second: Option<i64>,
    intercept: f64,
    slope: f64,
    local: bool,
) -> u8 {
    // scMin (Perl 3932-36): local uses ln(readLen), end-to-end uses readLen; add
    // read 2 for PE. The `else` arithmetic is byte-identical to the pre-`--local`
    // code (when `local == false`, `local` adds nothing) — end-to-end frozen.
    let mut sc_min = if local {
        intercept + slope * (read1_len as f64).ln()
    } else {
        intercept + slope * read1_len as f64
    };
    if let Some(l2) = read2_len {
        sc_min += if local {
            intercept + slope * (l2 as f64).ln()
        } else {
            intercept + slope * l2 as f64
        };
    }
    let diff = sc_min.abs(); // scores vary by up to this much (max AS = 0)
    let best_over = as_best as f64 - sc_min;

    if local {
        return calc_mapq_local(best_over, diff, as_best, as_second);
    }

    let Some(sec) = as_second else {
        // No second-best hit (3947–54).
        return if best_over >= diff * 0.8 {
            42
        } else if best_over >= diff * 0.7 {
            40
        } else if best_over >= diff * 0.6 {
            24
        } else if best_over >= diff * 0.5 {
            23
        } else if best_over >= diff * 0.4 {
            8
        } else if best_over >= diff * 0.3 {
            3
        } else {
            0
        };
    };

    // With a second-best hit (3957–4076).
    let best_diff = (as_best.abs() - sec.abs()).abs() as f64;
    if best_diff >= diff * 0.9 {
        if best_over == diff { 39 } else { 33 }
    } else if best_diff >= diff * 0.8 {
        if best_over == diff { 38 } else { 27 }
    } else if best_diff >= diff * 0.7 {
        if best_over == diff { 37 } else { 26 }
    } else if best_diff >= diff * 0.6 {
        if best_over == diff { 36 } else { 22 }
    } else if best_diff >= diff * 0.5 {
        if best_over == diff {
            35
        } else if best_over >= diff * 0.84 {
            25
        } else if best_over >= diff * 0.68 {
            16
        } else {
            5
        }
    } else if best_diff >= diff * 0.4 {
        if best_over == diff {
            34
        } else if best_over >= diff * 0.84 {
            21
        } else if best_over >= diff * 0.68 {
            14
        } else {
            4
        }
    } else if best_diff >= diff * 0.3 {
        if best_over == diff {
            32
        } else if best_over >= diff * 0.88 {
            18
        } else if best_over >= diff * 0.67 {
            15
        } else {
            3
        }
    } else if best_diff >= diff * 0.2 {
        if best_over == diff {
            31
        } else if best_over >= diff * 0.88 {
            17
        } else if best_over >= diff * 0.67 {
            11
        } else {
            0
        }
    } else if best_diff >= diff * 0.1 {
        if best_over == diff {
            30
        } else if best_over >= diff * 0.88 {
            12
        } else if best_over >= diff * 0.67 {
            7
        } else {
            0
        }
    } else if best_diff > 0.0 {
        if best_over >= diff * 0.67 { 6 } else { 2 }
    } else if best_over >= diff * 0.67 {
        1
    } else {
        0
    }
}

/// Local-mode MAPQ ladder — a verbatim port of Perl `calc_mapq`'s `--local`
/// branch (`bismark:4082-4178`). Distinct return values AND a uniform `diff*0.5`
/// sub-threshold (NOT the end-to-end `0.84/0.68/0.88/0.67`). `best_over`/`diff`
/// are computed with the `ln()` `scMin` by the caller.
#[allow(clippy::float_cmp)] // exact f64 equality matches Perl `$bestOver == $diff` (ln() bit-safe per spike)
fn calc_mapq_local(best_over: f64, diff: f64, as_best: i64, as_second: Option<i64>) -> u8 {
    let Some(sec) = as_second else {
        // No second-best hit (4082-90).
        return if best_over >= diff * 0.8 {
            44
        } else if best_over >= diff * 0.7 {
            42
        } else if best_over >= diff * 0.6 {
            41
        } else if best_over >= diff * 0.5 {
            36
        } else if best_over >= diff * 0.4 {
            28
        } else if best_over >= diff * 0.3 {
            24
        } else {
            22
        };
    };

    // With a second-best hit (4091-4177). bestDiff = |abs(best) - abs(second)|.
    let best_diff = (as_best.abs() - sec.abs()).abs() as f64;
    if best_diff >= diff * 0.9 {
        40
    } else if best_diff >= diff * 0.8 {
        39
    } else if best_diff >= diff * 0.7 {
        38
    } else if best_diff >= diff * 0.6 {
        37
    } else if best_diff >= diff * 0.5 {
        if best_over == diff {
            35
        } else if best_over >= diff * 0.5 {
            25
        } else {
            20
        }
    } else if best_diff >= diff * 0.4 {
        if best_over == diff {
            34
        } else if best_over >= diff * 0.5 {
            21
        } else {
            19
        }
    } else if best_diff >= diff * 0.3 {
        if best_over == diff {
            33
        } else if best_over >= diff * 0.5 {
            18
        } else {
            16
        }
    } else if best_diff >= diff * 0.2 {
        if best_over == diff {
            32
        } else if best_over >= diff * 0.5 {
            17
        } else {
            12
        }
    } else if best_diff >= diff * 0.1 {
        if best_over == diff {
            31
        } else if best_over >= diff * 0.5 {
            14
        } else {
            9
        }
    } else if best_diff > 0.0 {
        if best_over >= diff * 0.5 { 11 } else { 2 }
    } else if best_over >= diff * 0.5 {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // default --score_min: intercept 0, slope -0.2; readLen 50 → scMin -10, diff 10.
    const I: f64 = 0.0;
    const S: f64 = -0.2;

    #[test]
    fn no_second_best_ladder() {
        // bestOver = as_best - scMin = as_best + 10.
        assert_eq!(calc_mapq(50, None, 0, None, I, S, false), 42); // bestOver 10 = diff (>=0.8)
        assert_eq!(calc_mapq(50, None, -3, None, I, S, false), 40); // 7 = 0.7·diff
        assert_eq!(calc_mapq(50, None, -4, None, I, S, false), 24); // 6 = 0.6·diff
        assert_eq!(calc_mapq(50, None, -5, None, I, S, false), 23); // 5 = 0.5·diff
        assert_eq!(calc_mapq(50, None, -6, None, I, S, false), 8); // 4 = 0.4·diff
        assert_eq!(calc_mapq(50, None, -7, None, I, S, false), 3); // 3 = 0.3·diff
        assert_eq!(calc_mapq(50, None, -10, None, I, S, false), 0); // 0
    }

    #[test]
    fn with_second_best_top_buckets() {
        // as_best 0 (bestOver 10 == diff), vary second-best.
        assert_eq!(calc_mapq(50, None, 0, Some(-10), I, S, false), 39); // bestDiff 10 (>=0.9), ==diff
        assert_eq!(calc_mapq(50, None, 0, Some(-8), I, S, false), 38); // bestDiff 8 (>=0.8), ==diff
        assert_eq!(calc_mapq(50, None, 0, Some(-5), I, S, false), 35); // bestDiff 5 (>=0.5), ==diff
    }

    #[test]
    fn with_second_best_not_at_diff() {
        // as_best -3 (bestOver 7, not == diff 10), second-best near.
        assert_eq!(calc_mapq(50, None, -3, Some(-3), I, S, false), 1); // bestDiff 0 → else; 7>=6.7
        assert_eq!(calc_mapq(50, None, -3, Some(-10), I, S, false), 26); // bestDiff 7 = 0.7·diff, not ==diff
        assert_eq!(calc_mapq(50, None, -3, Some(-13), I, S, false), 33); // bestDiff 10 ≥ 0.9·diff, not ==diff
    }

    #[test]
    fn non_integer_scmin() {
        // readLen 51 → scMin -10.2, diff 10.2; as_best 0 → bestOver 10.2 >= 8.16 → 42.
        assert_eq!(calc_mapq(51, None, 0, None, I, S, false), 42);
    }

    #[test]
    fn inner_threshold_leaves_pinned() {
        // Every leaf of the with-second-best ladder, so a future 0.88↔0.84 /
        // 0.68↔0.67 typo can't pass green. (readLen 50 → scMin -10, diff 10;
        // bestOver = as_best + 10; bestDiff = |as_best| - |as_second| (abs).)
        // The `bestDiff > 0 && < 0.1·diff` (6/2) leaves need diff 20 (readLen 100).
        let cases: &[(usize, i64, i64, u8)] = &[
            // bestOver == diff (as_best 0) across the bestDiff buckets:
            (50, 0, -10, 39),
            (50, 0, -8, 38),
            (50, 0, -7, 37),
            (50, 0, -6, 36),
            (50, 0, -5, 35),
            (50, 0, -4, 34),
            (50, 0, -3, 32),
            (50, 0, -2, 31),
            (50, 0, -1, 30),
            // 0.9 / 0.8 / 0.7 / 0.6 buckets, NOT == diff (bestOver 7):
            (50, -3, -13, 33),
            (50, -3, -11, 27),
            (50, -3, -10, 26),
            (50, -3, -9, 22),
            // 0.5 bucket (0.84 / 0.68 sub-thresholds): 25 / 16 / 5
            (50, -1, -6, 25),
            (50, -3, -8, 16),
            (50, -4, -9, 5),
            // 0.4 bucket (0.84 / 0.68): 21 / 14 / 4
            (50, -1, -5, 21),
            (50, -3, -7, 14),
            (50, -4, -8, 4),
            // 0.3 bucket (0.88 / 0.67): 18 / 15 / 3
            (50, -1, -4, 18),
            (50, -3, -6, 15),
            (50, -4, -7, 3),
            // 0.2 bucket (0.88 / 0.67): 17 / 11 / 0
            (50, -1, -3, 17),
            (50, -3, -5, 11),
            (50, -4, -6, 0),
            // 0.1 bucket (0.88 / 0.67): 12 / 7 / 0
            (50, -1, -2, 12),
            (50, -3, -4, 7),
            (50, -4, -5, 0),
            // bestDiff in (0, 0.1·diff): 6 / 2 (needs diff 20)
            (100, -3, -4, 6),
            (100, -9, -8, 2),
            // bestDiff == 0: 1 / 0
            (50, -3, -3, 1),
            (50, -4, -4, 0),
        ];
        for &(len, ab, asb, want) in cases {
            let got = calc_mapq(len, None, ab, Some(asb), I, S, false);
            assert_eq!(got, want, "calc_mapq(len={len}, best={ab}, 2nd={asb})");
        }
    }

    #[test]
    fn user_score_min_slope() {
        // --score_min L,0,-0.4 on readLen 50 → scMin -20, diff 20.
        assert_eq!(calc_mapq(50, None, 0, None, 0.0, -0.4, false), 42); // bestOver 20 = diff
        assert_eq!(calc_mapq(50, None, -6, None, 0.0, -0.4, false), 40); // bestOver 14 = 0.7·20
    }

    // ── --local ladder (Perl 4082-4178) ── values cross-checked against the
    // Phase-0 spike's Perl computation. `calc_mapq_local` takes (best_over, diff,
    // as_best, as_second) with best_diff = |abs(best) - abs(second)|.

    #[test]
    fn local_no_second_best_ladder() {
        let d = 10.0;
        assert_eq!(calc_mapq_local(8.0, d, 0, None), 44); // 0.8·diff
        assert_eq!(calc_mapq_local(7.0, d, 0, None), 42); // 0.7
        assert_eq!(calc_mapq_local(6.0, d, 0, None), 41); // 0.6
        assert_eq!(calc_mapq_local(5.0, d, 0, None), 36); // 0.5
        assert_eq!(calc_mapq_local(4.0, d, 0, None), 28); // 0.4
        assert_eq!(calc_mapq_local(3.0, d, 0, None), 24); // 0.3
        assert_eq!(calc_mapq_local(2.0, d, 0, None), 22); // <0.3
    }

    #[test]
    fn local_second_best_ladder() {
        let d = 10.0;
        // Flat top buckets (NO bestOver sub-case in local): 0.9/0.8/0.7/0.6.
        assert_eq!(calc_mapq_local(5.0, d, 0, Some(-9)), 40); // bestDiff 9
        assert_eq!(calc_mapq_local(5.0, d, 0, Some(-8)), 39); // 8
        assert_eq!(calc_mapq_local(5.0, d, 0, Some(-7)), 38); // 7
        assert_eq!(calc_mapq_local(5.0, d, 0, Some(-6)), 37); // 6
        // 0.5 / 0.4 / 0.3 / 0.2 / 0.1 buckets: {==diff, >=diff*0.5, else}.
        for (bd_second, b_eq, b_hi, b_lo) in [
            (-5_i64, 35_u8, 25_u8, 20_u8), // bestDiff 5 (0.5)
            (-4, 34, 21, 19),              // 4 (0.4)
            (-3, 33, 18, 16),              // 3 (0.3)
            (-2, 32, 17, 12),              // 2 (0.2)
            (-1, 31, 14, 9),               // 1 (0.1)
        ] {
            assert_eq!(calc_mapq_local(10.0, d, 0, Some(bd_second)), b_eq); // ==diff
            assert_eq!(calc_mapq_local(5.0, d, 0, Some(bd_second)), b_hi); // >=diff*0.5
            assert_eq!(calc_mapq_local(4.0, d, 0, Some(bd_second)), b_lo); // else
        }
        // bestDiff > 0 but < diff*0.1 needs diff 20 (integer bestDiff can't be in (0,1)).
        assert_eq!(calc_mapq_local(10.0, 20.0, 0, Some(-1)), 11); // bestDiff 1 < 2.0; bestOver>=0.5·20
        assert_eq!(calc_mapq_local(9.0, 20.0, 0, Some(-1)), 2); // bestOver < 0.5·20
        // bestDiff == 0: 1 / 0.
        assert_eq!(calc_mapq_local(5.0, d, -3, Some(-3)), 1); // bestOver>=0.5·diff
        assert_eq!(calc_mapq_local(4.0, d, -3, Some(-3)), 0);
    }

    #[test]
    fn local_calc_mapq_uses_ln_scmin_and_local_ladder() {
        // local default (20, 8); readLen 50 → scMin = 20 + 8·ln(50) (≈ 51.296).
        let sc = 20.0_f64 + 8.0 * 50.0_f64.ln();
        for as_best in [120_i64, 100, 80, 70, 60, 55] {
            // calc_mapq(local=true) must equal the local ladder fed the ln scMin.
            let expect = calc_mapq_local(as_best as f64 - sc, sc.abs(), as_best, None);
            assert_eq!(calc_mapq(50, None, as_best, None, 20.0, 8.0, true), expect);
        }
        // The local branch genuinely diverges from end-to-end for the same args.
        assert_ne!(
            calc_mapq(50, None, 100, None, 20.0, 8.0, true),
            calc_mapq(50, None, 100, None, 20.0, 8.0, false)
        );
    }
}
