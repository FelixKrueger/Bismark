//! MAPQ computation — a port of Perl `calc_mapq` (3923–4186), both the **end-to-end** and
//! **`--local`** branches (the local ladder, `4082-4178`, ships for Bowtie 2 since #981 and
//! HISAT2 since the HISAT2-`--local` work).
//!
//! The returned MAPQ integers are **byte-identity-critical** — they land in the
//! BAM MAPQ column (Phase 5). Reviewer A verified Perl 5 and rustc 1.95 produce
//! **bit-identical `f64`** for this arithmetic, so the exact `==`/`>=` float
//! comparisons are intentional (an epsilon comparison would break parity).
//!
//! Two `--local` paths deviate from Perl deliberately (#1079): Bowtie 2-local's denominator
//! (Perl normalized by `abs(scMin)`, which assumes a perfect score of 0 — true only
//! end-to-end), and HISAT2-local's `scMin` form (Perl evaluated it logarithmically while
//! HISAT2 is emitted the linear `L` form). **End-to-end, for every aligner, remains
//! byte-identical to Perl.** See [`ScoreModel`] for the per-mode score model.

use crate::aligner::config::ScoreModel;

/// Bismark MAPQ. `read2_len` is `Some` only for paired-end; single-end passes
/// `None`. `model` carries the `--score_min` parameters, the `scMin` function
/// form and the ladder choice (see [`ScoreModel`]).
/// **The `ln()` form is bit-safe** — the Phase-0 spike (`plans/06132026_aligner-local-mode/
/// spikes/`) proved Perl `log` ≡ Rust `f64::ln()` bit-identical on the gate arch,
/// so the exact `==`/`>=` `f64` comparisons hold for both branches.
pub fn calc_mapq(
    read1_len: usize,
    read2_len: Option<usize>,
    as_best: i64,
    as_second: Option<i64>,
    model: ScoreModel,
) -> u8 {
    let (best_over, diff) = model.normalize(read1_len, read2_len, as_best);

    if model.local_ladder() {
        calc_mapq_local(best_over, diff, as_best, as_second)
    } else {
        calc_mapq_end_to_end(best_over, diff, as_best, as_second)
    }
}

/// End-to-end MAPQ ladder — a verbatim port of Perl `calc_mapq`'s default branch
/// (`bismark:3947-4076`). Split out to mirror [`calc_mapq_local`].
#[allow(clippy::float_cmp)] // exact f64 equality matches Perl `$bestOver == $diff` (verified bit-identical)
fn calc_mapq_end_to_end(best_over: f64, diff: f64, as_best: i64, as_second: Option<i64>) -> u8 {
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
/// come from [`ScoreModel::normalize`] — the `scMin` form and the denominator are
/// both aligner-dependent in local mode.
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

    /// `--score_min` (intercept, slope) cells for the frozen-path sweeps. `--score_min` is
    /// shape-validated only, so these are all legal, and each probes a way the historical
    /// `abs(scMin)` differs from `max(1, -scMin)`.
    const SCORE_MIN_CELLS: [(f64, f64); 6] = [
        (0.0, -0.2),  // default
        (0.0, -0.05), // shallow slope → the |scMin| >= 1 threshold moves to len >= 20
        (0.0, -0.6),
        (10.0, -0.2), // positive intercept → scMin > 0 for short reads
        (0.0, 0.0),   // scMin == 0 exactly
        (-1.0, -0.2),
    ];

    #[test]
    fn no_second_best_ladder() {
        // bestOver = as_best - scMin = as_best + 10.
        assert_eq!(
            calc_mapq(50, None, 0, None, ScoreModel::end_to_end(I, S)),
            42
        ); // bestOver 10 = diff (>=0.8)
        assert_eq!(
            calc_mapq(50, None, -3, None, ScoreModel::end_to_end(I, S)),
            40
        ); // 7 = 0.7·diff
        assert_eq!(
            calc_mapq(50, None, -4, None, ScoreModel::end_to_end(I, S)),
            24
        ); // 6 = 0.6·diff
        assert_eq!(
            calc_mapq(50, None, -5, None, ScoreModel::end_to_end(I, S)),
            23
        ); // 5 = 0.5·diff
        assert_eq!(
            calc_mapq(50, None, -6, None, ScoreModel::end_to_end(I, S)),
            8
        ); // 4 = 0.4·diff
        assert_eq!(
            calc_mapq(50, None, -7, None, ScoreModel::end_to_end(I, S)),
            3
        ); // 3 = 0.3·diff
        assert_eq!(
            calc_mapq(50, None, -10, None, ScoreModel::end_to_end(I, S)),
            0
        ); // 0
    }

    #[test]
    fn with_second_best_top_buckets() {
        // as_best 0 (bestOver 10 == diff), vary second-best.
        assert_eq!(
            calc_mapq(50, None, 0, Some(-10), ScoreModel::end_to_end(I, S)),
            39
        ); // bestDiff 10 (>=0.9), ==diff
        assert_eq!(
            calc_mapq(50, None, 0, Some(-8), ScoreModel::end_to_end(I, S)),
            38
        ); // bestDiff 8 (>=0.8), ==diff
        assert_eq!(
            calc_mapq(50, None, 0, Some(-5), ScoreModel::end_to_end(I, S)),
            35
        ); // bestDiff 5 (>=0.5), ==diff
    }

    #[test]
    fn with_second_best_not_at_diff() {
        // as_best -3 (bestOver 7, not == diff 10), second-best near.
        assert_eq!(
            calc_mapq(50, None, -3, Some(-3), ScoreModel::end_to_end(I, S)),
            1
        ); // bestDiff 0 → else; 7>=6.7
        assert_eq!(
            calc_mapq(50, None, -3, Some(-10), ScoreModel::end_to_end(I, S)),
            26
        ); // bestDiff 7 = 0.7·diff, not ==diff
        assert_eq!(
            calc_mapq(50, None, -3, Some(-13), ScoreModel::end_to_end(I, S)),
            33
        ); // bestDiff 10 ≥ 0.9·diff, not ==diff
    }

    #[test]
    fn non_integer_scmin() {
        // readLen 51 → scMin -10.2, diff 10.2; as_best 0 → bestOver 10.2 >= 8.16 → 42.
        assert_eq!(
            calc_mapq(51, None, 0, None, ScoreModel::end_to_end(I, S)),
            42
        );
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
            let got = calc_mapq(len, None, ab, Some(asb), ScoreModel::end_to_end(I, S));
            assert_eq!(got, want, "calc_mapq(len={len}, best={ab}, 2nd={asb})");
        }
    }

    #[test]
    fn user_score_min_slope() {
        // --score_min L,0,-0.4 on readLen 50 → scMin -20, diff 20.
        assert_eq!(
            calc_mapq(50, None, 0, None, ScoreModel::end_to_end(0.0, -0.4)),
            42
        ); // bestOver 20 = diff
        assert_eq!(
            calc_mapq(50, None, -6, None, ScoreModel::end_to_end(0.0, -0.4)),
            40
        ); // bestOver 14 = 0.7·20
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

    /// Independent reference for Bowtie 2-local normalization — the **f64 analogue** of
    /// `unique.h:206-222`. Deliberately NOT a faithful transcription: Bowtie 2 computes
    /// `scMin`/`diff`/`bestOver` in `int64_t` (`simple_func.h:106` truncates), Bismark in
    /// `f64`. Matching its truncation would change end-to-end MAPQ, so the int/float gap
    /// is an accepted divergence (#1079) and this reference keeps Bismark's `f64`. Upstream
    /// also multiplies by `(double)0.8f` (a widened float literal) where Bismark and Perl use
    /// the double `0.8` — a second, deliberate divergence, so do not try to make this exact.
    fn bowtie2_local_reference(
        read1_len: usize,
        read2_len: Option<usize>,
        as_best: i64,
        as_second: Option<i64>,
        intercept: f64,
        slope: f64,
    ) -> u8 {
        let term = |len: usize| intercept + slope * (len as f64).ln();
        let sc_min = term(read1_len) + read2_len.map_or(0.0, term);
        let perfect = 2.0 * (read1_len + read2_len.unwrap_or(0)) as f64;
        let diff = (perfect - sc_min).max(1.0);
        calc_mapq_local(as_best as f64 - sc_min, diff, as_best, as_second)
    }

    /// Bowtie 2-local must agree with the independent reference above across a grid.
    /// Replaces a test that asserted `calc_mapq == calc_mapq_local(.., sc.abs(), ..)` —
    /// i.e. the implementation against itself, which could not fail whatever `diff` was.
    #[test]
    fn local_bowtie2_matches_independent_reference() {
        for len in [25usize, 50, 100, 150, 250] {
            for as_best in [2 * len as i64, len as i64, len as i64 / 2, 0, -1] {
                for as_second in [None, Some(-1), Some(as_best - 1)] {
                    assert_eq!(
                        calc_mapq(
                            len,
                            None,
                            as_best,
                            as_second,
                            ScoreModel::bowtie2_local(20.0, 8.0)
                        ),
                        bowtie2_local_reference(len, None, as_best, as_second, 20.0, 8.0),
                        "SE len={len} as_best={as_best} as_second={as_second:?}"
                    );
                }
            }
        }
        // PE: the perfect score must sum BOTH mates (unique.h:207-209).
        for (l1, l2) in [(50usize, 50usize), (100, 100), (75, 125)] {
            assert_eq!(
                calc_mapq(
                    l1,
                    Some(l2),
                    (l1 + l2) as i64,
                    Some(-1),
                    ScoreModel::bowtie2_local(20.0, 8.0)
                ),
                bowtie2_local_reference(l1, Some(l2), (l1 + l2) as i64, Some(-1), 20.0, 8.0),
                "PE {l1}+{l2}"
            );
        }
        // The local branch genuinely diverges from end-to-end for the same args.
        assert_ne!(
            calc_mapq(50, None, 100, None, ScoreModel::bowtie2_local(20.0, 8.0)),
            calc_mapq(50, None, 100, None, ScoreModel::end_to_end(20.0, 8.0))
        );
    }

    /// The reporter's case from #1079: the old `diff = abs(scMin)` gave 42 here because it
    /// ignored the positive perfect local score. Bowtie 2's `perfectScore - scMin` gives 24.
    #[test]
    fn local_bowtie2_denominator_uses_perfect_score_issue_1079() {
        // len 100, G,20,8 → scMin = 20 + 8·ln(100) ≈ 56.8414; perfect = 200.
        // old: diff = 56.8414, bestOver/diff = 0.759 → 42
        // new: diff = 143.1586, bestOver/diff = 0.301 → 24
        assert_eq!(
            calc_mapq(100, None, 100, None, ScoreModel::bowtie2_local(20.0, 8.0)),
            24
        );
        // A perfect local alignment (AS == perfect) puts bestOver exactly on diff → top rung.
        assert_eq!(
            calc_mapq(100, None, 200, None, ScoreModel::bowtie2_local(20.0, 8.0)),
            44
        );
        // PE perfect score sums both mates: 2·(100+100) = 400, NOT 200.
        let sc = 2.0 * (20.0 + 8.0 * 100.0_f64.ln());
        let (bo, diff) = ScoreModel::bowtie2_local(20.0, 8.0).normalize(100, Some(100), 400);
        assert_eq!(diff, 400.0 - sc);
        assert_eq!(bo, 400.0 - sc);
        // …and differs from the SE-only denominator, so a single-mate bug cannot hide.
        let (_, se_diff) = ScoreModel::bowtie2_local(20.0, 8.0).normalize(100, None, 400);
        assert_ne!(diff, se_diff);
    }

    /// `diff` is clamped to ≥1 (`unique.h:218`). Reachable only when `perfect - scMin < 1`,
    /// which for `G,20,8` needs a read too short for Bowtie 2 to report at all (len ≲ 22) —
    /// but fake-aligner fixtures can produce it, so pin the arithmetic.
    #[test]
    fn local_diff_is_clamped_to_at_least_one() {
        // len 6: perfect = 12, scMin = 20 + 8·ln(6) ≈ 34.33 → perfect - scMin ≈ -22.33.
        let (_, diff) = ScoreModel::bowtie2_local(20.0, 8.0).normalize(6, None, 0);
        assert_eq!(diff, 1.0);
        // The rung matters more than the clamp: a degenerate `diff` fails by silently winning
        // the TOP rung (every `>= diff * k` passes), so pin the value. bestOver = 0 - 34.33 is
        // negative, so every rung fails and the no-second-best floor 22 results.
        assert_eq!(
            calc_mapq(6, None, 0, None, ScoreModel::bowtie2_local(20.0, 8.0)),
            22
        );
        // End-to-end is NOT clamped — `scMin == 0` still yields `diff == 0` (frozen).
        let (_, e2e) = ScoreModel::end_to_end(0.0, 0.0).normalize(50, None, 0);
        assert_eq!(e2e, 0.0);
    }

    /// HISAT2-`--local` default params `(0, -0.2)`. HISAT2 is emitted the **linear** `L` form,
    /// so `scMin = -0.2·readLen` — it was evaluated as `-0.2·ln(readLen)` before #1079 D2, a
    /// threshold HISAT2 never applied. Its perfect score stays 0 (Bismark's own docs: "for
    /// HISAT2, it is currently not exactly known how the best alignment is calculated"), so
    /// `diff = abs(scMin)` here — deliberately unchanged pending that follow-up.
    ///
    /// Every expectation below is the Perl local ladder (`bismark:4082-4178`) hand-applied to
    /// the linear `scMin` — NOT read back from the implementation.
    #[test]
    fn local_hisat2_uses_the_linear_form_it_was_emitted() {
        let (i, s) = (0.0, -0.2);
        // @50bp: scMin = -10, diff = 10, best_over = as_best + 10.
        // as_best 0 → best_over 10 == diff (≥0.8·diff) → 44.
        assert_eq!(
            calc_mapq(50, None, 0, None, ScoreModel::hisat2_local(i, s)),
            44
        );
        // as_best -1 → best_over 9 = 0.9·diff (≥0.8) → 44. (Was 22 under the ln() scMin, whose
        // sub-unity diff of 0.78 put best_over at -0.218.)
        assert_eq!(
            calc_mapq(50, None, -1, None, ScoreModel::hisat2_local(i, s)),
            44
        );
        // @150bp: scMin = -30, best_over 30 == diff → 44 (as_best 0 is readLen-invariant).
        assert_eq!(
            calc_mapq(150, None, 0, None, ScoreModel::hisat2_local(i, s)),
            44
        );
        // Second-best @50bp, as_best 0: best_diff = |0| - |-1| = 1, and 1 ≥ diff·0.1 = 1 exactly
        // → the 0.1 bucket, where best_over == diff → 31. (`10.0 * 0.1 == 1.0` is exact in
        // IEEE-754 and Perl computes the same double, so this is deterministic — but the margin
        // is zero, hence the off-boundary cells below.)
        assert_eq!(
            calc_mapq(50, None, 0, Some(-1), ScoreModel::hisat2_local(i, s)),
            31
        );
        // Same rungs with real margin, so the mode is not pinned only on that knife edge.
        // @100bp: scMin -20, diff 20, best_over 20 == diff.
        //   second -3 → best_diff 3 ≥ diff·0.1 = 2 (margin 1) → 0.1 bucket, ==diff → 31
        //   second -5 → best_diff 5 ≥ diff·0.2 = 4 (margin 1) → 0.2 bucket, ==diff → 32
        //   second -1 → best_diff 1 < diff·0.1 = 2 → terminal leaf, 20 ≥ 10 → 11
        assert_eq!(
            calc_mapq(100, None, 0, Some(-3), ScoreModel::hisat2_local(i, s)),
            31
        );
        assert_eq!(
            calc_mapq(100, None, 0, Some(-5), ScoreModel::hisat2_local(i, s)),
            32
        );
        assert_eq!(
            calc_mapq(100, None, 0, Some(-1), ScoreModel::hisat2_local(i, s)),
            11
        );
        // as_best -1, second -1: best_diff 0 → the terminal leaf; best_over 9 ≥ diff·0.5 = 5 → 1.
        assert_eq!(
            calc_mapq(50, None, -1, Some(-1), ScoreModel::hisat2_local(i, s)),
            1
        );
        // PE 150+150: scMin = -60, diff 60, best_over 60. best_diff 1 < diff·0.1 = 6 → terminal
        // leaf; 60 ≥ 30 → 11.
        assert_eq!(
            calc_mapq(150, Some(150), 0, Some(-1), ScoreModel::hisat2_local(i, s)),
            11
        );
        // The form is what changed: HISAT2-local must NOT evaluate the logarithmic scMin.
        let (_, linear_diff) = ScoreModel::hisat2_local(i, s).normalize(50, None, 0);
        assert_eq!(linear_diff, 10.0);
        assert_ne!(linear_diff, (i + s * 50.0_f64.ln()).abs());
    }

    /// Re-homes the `ln()`-derived-bucket-boundary coverage that HISAT2-local provided before
    /// D2 made it linear. Bowtie 2-local is now the only `ln()` consumer, so the guard belongs
    /// here: the selected rung depends on an `ln()`-derived threshold, not an integer one.
    #[test]
    fn local_bowtie2_ln_derived_bucket_boundary() {
        // len 25, G,20,8 → scMin = 20 + 8·ln(25) = 45.7510066, perfect = 50,
        // diff = 4.2489934.  With as_best 50 (perfect): best_over == diff → top rung.
        let sc = 20.0 + 8.0 * 25.0_f64.ln();
        let (bo, diff) = ScoreModel::bowtie2_local(20.0, 8.0).normalize(25, None, 50);
        assert_eq!(diff, 50.0 - sc);
        assert_eq!(bo, diff);
        assert_eq!(
            calc_mapq(25, None, 50, None, ScoreModel::bowtie2_local(20.0, 8.0)),
            44
        );
        // as_best 48 → best_over 2.2489934, ratio 0.52930 → the 0.5 rung (36). The 0.5/0.4
        // boundary sits at 2.1244967, an ln()-derived value ~0.1245 away — robust, but
        // genuinely non-integer, which is the property this cell exists to pin.
        assert_eq!(
            calc_mapq(25, None, 48, None, ScoreModel::bowtie2_local(20.0, 8.0)),
            36
        );
    }

    /// `(local, aligner)` → resolved score model. The one piece of new logic, and the
    /// failure mode it guards is silent: a mis-wired aligner leaves `match_bonus` at 0,
    /// which turns the #1079 fix into a no-op that every value test still passes.
    #[test]
    fn score_model_construction_matrix() {
        use crate::aligner::config::{Aligner, ScoreMinForm};
        // Bowtie 2 --local: logarithmic G form + positive perfect score.
        let m = ScoreModel::from_emitted(20.0, 8.0, ScoreMinForm::Log, true, Aligner::Bowtie2);
        assert!(m.local_ladder());
        let (_, diff) = m.normalize(100, None, 0);
        assert_eq!(diff, 200.0 - (20.0 + 8.0 * 100.0_f64.ln())); // perfect - scMin
        assert_eq!(m, ScoreModel::bowtie2_local(20.0, 8.0));

        // HISAT2 --local: linear L form, perfect score 0 → abs(scMin), local ladder.
        let m = ScoreModel::from_emitted(0.0, -0.2, ScoreMinForm::Linear, true, Aligner::Hisat2);
        assert!(m.local_ladder());
        let (_, diff) = m.normalize(100, None, 0);
        assert_eq!(diff, 20.0); // abs(-0.2·100), NOT max(1, 0 - scMin)
        assert_eq!(m, ScoreModel::hisat2_local(0.0, -0.2));

        // End-to-end, every aligner: linear, perfect 0, end-to-end ladder — byte-frozen.
        for aligner in [
            Aligner::Bowtie2,
            Aligner::Hisat2,
            Aligner::Minimap2,
            Aligner::Rammap,
        ] {
            let m = ScoreModel::from_emitted(0.0, -0.2, ScoreMinForm::Linear, false, aligner);
            assert!(
                !m.local_ladder(),
                "{aligner:?} must use the end-to-end ladder"
            );
            assert_eq!(m, ScoreModel::end_to_end(0.0, -0.2), "{aligner:?}");
            let (_, diff) = m.normalize(100, None, 0);
            assert_eq!(diff, 20.0, "{aligner:?}");
        }
    }

    /// **End-to-end** MAPQ is byte-frozen against the pre-#1079 formula. Structural (the
    /// `match_bonus == 0` branch IS `abs(scMin)`), but swept anyway — and crucially over a
    /// `--score_min` axis, because `abs(scMin) == -scMin` only for `scMin <= -1`. A
    /// non-default `--score_min` is exactly where applying the new denominator universally
    /// would have silently changed default-path MAPQ (verified: injecting a universal clamp
    /// fails this test at `len = 1`).
    ///
    /// HISAT2-local is covered separately below — only its denominator *shape* is frozen,
    /// not its values (D2 changed its `scMin` form).
    #[test]
    fn end_to_end_matches_the_pre_fix_formula() {
        // The pre-#1079 end-to-end formula, verbatim: linear scMin, diff = abs(scMin).
        fn frozen(l1: usize, l2: Option<usize>, best: i64, sec: Option<i64>, i: f64, s: f64) -> u8 {
            let term = |len: usize| i + s * len as f64;
            let sc_min = term(l1) + l2.map_or(0.0, term);
            calc_mapq_end_to_end(best as f64 - sc_min, sc_min.abs(), best, sec)
        }
        for (i, s) in SCORE_MIN_CELLS {
            for len in 1..=500usize {
                for best in [0i64, -1, -5, -20, 5] {
                    for sec in [None, Some(-1), Some(-7)] {
                        assert_eq!(
                            calc_mapq(len, None, best, sec, ScoreModel::end_to_end(i, s)),
                            frozen(len, None, best, sec, i, s),
                            "e2e SE i={i} s={s} len={len} best={best} sec={sec:?}"
                        );
                        assert_eq!(
                            calc_mapq(len, Some(len), best, sec, ScoreModel::end_to_end(i, s)),
                            frozen(len, Some(len), best, sec, i, s),
                            "e2e PE i={i} s={s} len={len} best={best} sec={sec:?}"
                        );
                    }
                }
            }
        }
    }

    /// HISAT2-local's denominator stays `abs(scMin)` (its perfect score is deliberately 0),
    /// now over the **linear** `scMin` it is actually emitted. Only the shape is frozen — the
    /// values moved with D2, which is what `local_hisat2_uses_the_linear_form_it_was_emitted`
    /// pins. The `--score_min` axis matters here too: `abs()` must not become `max(1, -scMin)`.
    #[test]
    fn hisat2_local_denominator_is_abs_of_the_linear_scmin() {
        for (i, s) in SCORE_MIN_CELLS {
            for len in 1..=500usize {
                let (_, se) = ScoreModel::hisat2_local(i, s).normalize(len, None, 0);
                assert_eq!(se, (i + s * len as f64).abs(), "SE i={i} s={s} len={len}");
                let (_, pe) = ScoreModel::hisat2_local(i, s).normalize(len, Some(len), 0);
                assert_eq!(
                    pe,
                    (2.0 * (i + s * len as f64)).abs(),
                    "PE i={i} s={s} len={len}"
                );
            }
        }
    }
}
