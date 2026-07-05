# Code Review A — Phase 4 (N-way lockstep merge + scoring + MAPQ)

**Reviewer:** A (independent, fresh context) · **Date:** 2026-06-01
**Scope:** `mapq.rs`, `merge.rs`, `align.rs` (`SamStream`), `lib.rs`
(`pipeline`/`run_se_directional`/`drive_merge`), `config.rs`/`options.rs`
(`score_min_intercept`/`slope` + `score_min_params`), `tests/cli.rs`.
**Gate:** byte-identical decompressed SAM vs Perl Bismark v0.25.1.
**Audit only — no code modified** (dual-reviewer mode).

---

## Summary

Phase 4 is a **faithful, high-quality port**. The two areas the brief flagged as
the highest-risk — the `calc_mapq` threshold ladder (esp. the 0.84/0.68 vs
0.88/0.67 shift) and the merge's overwrite / second-best / dedup / tie logic —
are both **transcribed correctly, leaf for leaf, against the Perl**. I verified
`calc_mapq` two ways: (1) line-by-line against Perl 3945–4076, and (2)
empirically — a side-by-side Perl-vs-Rust harness over **14 inner-bucket inputs**
(every 0.84/0.68/0.88/0.67 leaf in the with-second-best path) produced **identical
MAPQ** in both, and the `bestOver == diff` exact-`f64` comparison holds
bit-identically in Perl 5 and rustc across integer and non-integer scMin
(len 50/51/75/101/151). The `#[allow(clippy::float_cmp)]` exact `==` is correct
and intentional; an epsilon would break parity.

`cargo test -p bismark-aligner` → **67 unit + 15 CLI pass**.
`cargo clippy -p bismark-aligner --all-targets -- -D warnings` → **clean**.

No Critical or High **correctness** defects. The only material gap is **test
coverage** (High): the exact threshold-shift leaves the brief warned about are
never asserted, so a future one-character edit (e.g. 0.88→0.84) would ship
undetected. Everything else is Low / informational and concerns malformed-input
edge behaviour, not the real-data spine.

---

## Issues by area

### 1. `calc_mapq` transcription — CORRECT (no defect)

Verified every leaf of `mapq.rs` 29–114 against Perl 3945–4076:

- No-second-best ladder (mapq.rs 29–46) = Perl 3948–3954 (42/40/24/23/8/3/0). ✓
- With-second-best `bestDiff` buckets 0.9→0.1 + `>0` + `else`: the `==diff`
  high values (39/38/37/36/35/34/32/31/30) and the no-`==diff` values
  (33/27/26/22) all match. ✓
- **The threshold shift is correct:** the 0.5 and 0.4 buckets use **0.84 / 0.68**
  (→ 25/16/5 and 21/14/4); the 0.3 / 0.2 / 0.1 buckets use **0.88 / 0.67**
  (→ 18/15/3, 17/11/0, 12/7/0). mapq.rs 61/63/71/73 use 0.84/0.68; 81/83/91/93/
  101/103 use 0.88/0.67. Exactly Perl. ✓
- `bestDiff = (as_best.abs() − sec.abs()).abs()` (mapq.rs 49) = Perl 3957
  `abs(abs(AS_best)-abs(AS_secBest))`. ✓
- The no-second-best vs with-second-best split via `let Some(sec) = as_second
  else` (mapq.rs 29) = Perl `if (!defined $AS_secBest)`. ✓
- `--local` correctly omitted (rejected upstream in `options.rs`); only the
  `!$local` branch is ported, as the module doc states. ✓

**Empirical confirmation:** Perl and Rust agreed on all 14 probed inner-bucket
cases (AS_best/AS_sec pairs hitting 25,16,5,21,14,4,18,15,3,17,11,12,7,1).

### 2. Merge faithfulness (`merge.rs` vs Perl 2702–3151) — CORRECT

- **overwrite / best_AS / amb_same_thread** (merge.rs 144–159) = Perl 2802–2834:
  `>=` keeps equally-good alignments; only strictly-better (`>`) resets
  `amb_same_thread`; `best_as_so_far` updated after the reset check. ✓
- **second-best handling** (merge.rs 162–189) = Perl 2840–2953: `as == sb` →
  set `amb_same_thread` only if it is the current best, store nothing; else store
  iff `overwrite`. The unconditional discard-until-qname-changes loop (192–194)
  correctly hoists Perl's three per-branch `until` loops into one (all three Perl
  branches run it). ✓
- **chr:pos dedup** (`insert_alignment`, merge.rs 276) = Perl 2877/2917
  `join(":",chromosome,position)` — same-location alignments collapse to one
  entry, keeping the first-seen (lower) index. Test `same_location_in_both_
  instances_dedups` covers it. ✓
- **3075 second-best conditional** (merge.rs 224–227): best's own second-best is
  used for MAPQ only if **strictly greater** than the runner-up's AS, else the
  runner-up's AS. Exactly Perl 3075–3080. ✓
- **unique-best sort + tie boot** (merge.rs 209–234) = Perl 3033–3088:
  `len==1` → accept; `len<=4` → sort desc, `entries[0].as == entries[1].as` →
  ambiguous (`unsuitable_sequence_count++`), else pick `entries[0]`; `>4` → die
  with the verbatim message. ✓ HashMap-iteration nondeterminism is **harmless**:
  a tie at the top is booted before any pick, and the runner-up's *AS value*
  (the only thing consumed) is identical regardless of which tied entry sorts
  to index 1. ✓
- **directional index-2/3 rejection** (merge.rs 237–240) = Perl 3112–3118
  (`alignments_rejected_count++`, return). Inert on the SE-directional 2-instance
  spine but tested via a 4-instance double. ✓
- **flag==4 advance + die** (merge.rs 104–112) = Perl 2739–2758: advance once,
  die if the next record is still this identifier, else continue; EOF after the
  unmapped marker → `current()` is `None` → continue (Perl undef + next). ✓
- **de-conversion + AS/MD die** (merge.rs 115–139) = Perl 2763–2768 / 2838:
  strip `_CT_converted`/`_GA_converted` or die; AS and MD mandatory on a mapped
  record. ✓ (Note: Rust de-converts only the **trailing** suffix via
  `strip_suffix`, matching Perl's anchored `s/_(CT|GA)_converted$//`.)
- **Strand counters NOT incremented here** — confirmed absent. `Counters`
  (merge.rs 54–66) holds only the five Phase-4 tallies; the per-strand
  CT_CT/CT_GA/… counters (Perl 7113–7120) are correctly deferred to Phase 5. ✓

### 3. Lockstep key (`drive_merge`, lib.rs 188–193) — CORRECT

`identifier = fix_id(chomp(header), icpc)` then strip a single leading `@`
(lib.rs 190–192) = Perl 2420–2442 (`chomp` → `fix_IDs` → `.= "\n"` → `chomp` →
`s/^\@//`). `fix_id` is applied to the header **with `@` still present**, exactly
as Perl runs `fix_IDs` before `s/^\@//`. The converted temp FastQ keeps the `@`
as the FastQ marker, so Bowtie 2's QNAME equals the `@`-stripped identifier — the
key matches the SAM `qname`. Sequence is `chomp` → `to_ascii_uppercase` (lib.rs
193) = Perl `uc$sequence` (passed to `check_results_single_end` at 2444).
`skip`/`upto` use the same pre-skip `count`/post-skip `sequences_count` ordering
as Perl 2424–2433 **and** as `convert.rs` 194–213, so the driver and both streams
stay in lockstep (skipped reads are absent from the temp file the streams read).
✓

### 4. Driver / child-process (`align.rs`, `lib.rs`) — CORRECT

- 2 streams spawned on the **C→T** temp file: `--norc` on the CT index,
  `--nofw` on the GA index (lib.rs 112–127) = Perl 6873–6882 strand/flag table.
  ✓
- `finish()` (align.rs 241–252) drains stdout **before** `wait()` — the
  `early_stop_does_not_deadlock_or_zombie` test emits 5000 records (> 64K pipe
  buffer) and confirms no hang. `Drop` (255–263) kills-then-waits if not finished,
  so an error path that bypasses `finish()` (lib.rs 131–133 short-circuits on
  `?`) leaves no zombie. ✓
- The "streams in lockstep" assumption is handled: the merge skips any instance
  whose `current()` qname ≠ identifier (merge.rs 97) = Perl 2735 `if last_seq_id
  eq identifier`. An instance that has run ahead is simply not consulted for this
  read. ✓
- No borrow/move issue: `drive_merge` borrows `&mut [AlignerStream]`; the owning
  `Vec` is consumed afterwards by `for s in streams { s.finish()? }`. Compiles
  and tests pass. ✓

### 5. Tests

`merge`/`mapq`/`align` units assert real behaviour (de-conversion, cross-instance
best/tie, same-thread ambiguity, dedup, no-alignment, missing-suffix die, flag4+
same-id die, directional rejection; stream header-skip/EOF/early-stop/nonzero-
exit). The one notable **gap** is below (M-1).

---

## Recommendations (by priority)

### High

**H-1 — MAPQ inner-bucket (threshold-shift) leaves are untested.**
The brief's headline risk ("a single wrong integer is a silent MAPQ divergence")
is precisely the set of leaves that *no* test exercises. `mapq.rs` tests cover the
no-second-best ladder, the three top `==diff` buckets (39/38/35), three
not-`==diff` cases (1/26/33), one non-integer scMin, and the user-slope path —
but **none** of the 0.84/0.68/0.88/0.67 sub-buckets (values 25/16/5, 21/14/4,
18/15/3, 17/11, 12/7, 6/2). A future edit swapping 0.88↔0.84 (the exact failure
mode named) would pass all current tests. The transcription *is* correct today
(I confirmed all 14 against Perl), so this is a regression-guard gap, not a bug.
**Fix:** add ~14 `assert_eq!` cases pinning each inner leaf, e.g. at readLen 50
(scMin −10, diff 10): `calc_mapq(50,None,-1,Some(-6),I,S)==25`,
`(-3,Some(-8))==16`, `(-4,Some(-9))==5`, `(-1,Some(-5))==21`, `(-3,Some(-7))==14`,
`(-4,Some(-8))==4`, `(-1,Some(-4))==18`, `(-3,Some(-6))==15`, `(-4,Some(-7))==3`,
`(-1,Some(-3))==17`, `(-3,Some(-5))==11`, `(-1,Some(-2))==12`, `(-3,Some(-4))==7`,
plus a `bestDiff>0` `==6`/`==2` pair. (These exact inputs/expectations are the
ones I verified Perl-vs-Rust-identical.)

### Medium

**M-1 — Multi-instance unique-best (3+ entries) and the 3075 second-best path
have no merge unit test.** `merge.rs` tests cover 2-instance scenarios only.
The `entries.len() <= 4` branch's runner-up tie boot, the `len()>4` die, and
especially the 3075 conditional (best's own second-best vs runner-up's AS) are
unexercised at the merge level. **Fix:** add a 4-instance `VecStream` test with
three distinct-location mapped records (e.g. AS 0 / −3 / −6) asserting
`UniqueBest` with `alignment_score_second_best == Some(-3)` (runner-up), plus one
where the best carries `XS:i:` strictly greater than the runner-up to assert
best's own second-best is kept. Also a 5-entry case asserting the "too many
potential hits" error.

### Low

**L-1 — `score_min_params` errors on a non-numeric / 3+-value intercept where
Perl numifies leniently.** Perl 7917–7921 captures `^L,(.+),(.+)$` (greedy) and
**numifies** the captures — `L,1,2,3` → intercept `"1,2"`→1.0, slope 3; `L,abc,
def` → 0,0. `options.rs::score_min_params` (197–219) splits on the *last* comma
(`rsplit_once` — which **is** the faithful equivalent of Perl's greedy capture,
confirmed) but then `f64::parse` **errors** on a non-numeric half. So on
malformed `--score_min` the Rust fails at resolve-time where Perl would proceed
(Bowtie 2 would then reject the pushed string anyway). For all well-formed
`L,<float>,<float>` inputs there is **zero** divergence, and the pushed Bowtie 2
option string is byte-identical (Rust passes verbatim; Perl reconstructs to the
same). Acceptable; document that malformed score_min is a fail-fast deviation,
or mirror Perl's leading-numeric coercion if strict parity on garbage input is
ever required.

**L-2 — MAPQ read length uses `String::len()` (bytes) on a `from_utf8_lossy`
sequence.** lib.rs 193 builds `sequence` via `from_utf8_lossy(...).
to_ascii_uppercase()`; `calc_mapq` then uses `sequence.len()` (byte length) for
scMin. Perl's `length($sequence)` is character length. For legitimate ASCII read
data these are identical, and `to_ascii_uppercase` matches Perl `uc` on bases.
Only invalid-UTF-8 sequence bytes (which never occur in real FastQ sequence
lines) would diverge (replacement-char inflation). Negligible; note it so it is a
conscious assumption rather than a latent surprise.

**L-3 — Documentation nit.** `merge.rs` `BestAlignment.index` doc (line 24) says
"2/3 = PE/non-dir" but index 2/3 are the **non-directional / CTOT-CTOB SE
instances** (PE uses the same 0–3 indexing); minor wording. No code impact.

---

## Verdict

**APPROVE for the SE-directional spine**, contingent on adding H-1 (and ideally
M-1) before merge — these are regression guards for the exact byte-identity risk
the phase is gated on, not blockers on current correctness. The merge, MAPQ
ladder, lockstep key, child-process lifecycle, and option/param plumbing are all
faithful to Perl v0.25.1. L-1/L-2/L-3 are malformed-input / documentation
edge notes with no real-data impact.

Tests: 67 unit + 15 CLI green; clippy `-D warnings` clean.
