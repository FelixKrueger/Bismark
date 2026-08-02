# CODE REVIEW A — #1081 minimap2/rammap MAPQ denominator

**Reviewer:** A (independent; no coordination with Reviewer B)
**Target:** branch `plan/mapq-minimap2-denominator` (uncommitted) on `dev` `172da96`
**Plan:** `plans/08012026_minimap2-mapq-denominator/PLAN.md` rev 2 (+ `SPIKE.md`, both plan reviews)
**Date:** 2026-08-01

---

## Verdict

**Ship after fixing one High finding, which is documentation-only.** The code is correct: every
asserted MAPQ value re-derives against an independently extracted Perl oracle, Bowtie 2 and
HISAT2 are untouched *structurally* (not just by a green suite), and the gates have real teeth —
I reproduced three of the plan's six fault injections plus two of my own design, and each one
failed in exactly the place the plan predicted.

The one substantive defect is that **the shipped user-facing text says the new MAPQ floor is 22
and that a `-q 20` filter therefore stays a no-op. That is false**, and false in the
data-loss direction: reads with a cross-instance runner-up now land at 2–21. This is #1080's
HIGH-1 defect class, in a default path, and the plan itself carries the same error, so it needs
Felix's decision rather than a wording tweak.

A second issue — the rammap BAM gate breaking `cargo test -p bismark --features
rammap-inprocess`, a real CI job — I found and empirically confirmed; Reviewer B applied the
identical fix while I was reviewing, and I verified it. Recorded below as independent
corroboration.

---

## 1. What I verified independently

### 1.1 Every asserted MAPQ value (re-derived, not read back)

I built a Perl oracle by extracting `sub calc_mapq` **verbatim** from `bismark:3920-4180` and
patching exactly four lines: the two `scMin` lines (to take `(intercept, slope, log_form)` as
parameters instead of the globals), the `$diff` line (to `match_bonus > 0 ? max(1, perfect −
scMin) : abs(scMin)`), and the ladder selector (`if (!$local)` → `if (!($MB > 0))`). Both
ladders are untouched Perl text, so a transcription error on my side cannot produce agreement.

| Assertion site | Asserted | Oracle | ✓ |
|---|---|---|---|
| `minimap_like_denominator_uses_perfect_score_issue_1081` — len 50/100/1000 | 44/44/41/28/22 | 44/44/41/28/22 at all three lengths | ✓ |
| …its pre-fix control | 42 for all 15 cells | 42 | ✓ |
| `minimap_like_second_best_branch_issue_1081` | new 11/34/39/11/18 | 11/34/39/11/18 | ✓ |
| …its pre-fix control | old 25/33/33/25/33 | 25/33/33/25/33 | ✓ |
| `minimap_like_floor_is_twentytwo_not_zero` | 22, e2e-ladder counterpart 0 | 22 / 0 | ✓ |
| `minimap_like_non_default_score_min_compresses_upward` slopes −0.2/−1/−5/−20 | 44/44/41/28/22 · 44/44/42/41/28 · 44/44/44/44/42 · 44/44/44/44/44 | identical | ✓ |
| …the `L,10,−0.2` clamp cells, len 1–4 | `diff == 1.0`, `bestOver < 0`, MAPQ 22 | 22; first non-clamped length is 5 (`perfect − scMin == 1.0` exactly), so stopping the loop at 4 is right | ✓ |
| BAM cells (a)(b)(c), 6 bp, `L,0,−0.2` | 44 / 41 / 22 | 44 / 41 / 22 | ✓ |
| BAM cell (d), CT 11 @1 + GA 12 @3 | new 11, old 27 | 11 / 27 | ✓ |
| cell (b)'s failure-message fault values | Log form → 36, bonus 0.0 → 42, bonus 1.0 → 44 | 36 / 42 / 44 | ✓ |
| rev-2's claim that `AS:i:5` is form-blind | 28 under both forms | 28 / 28 | ✓ |

**No wrong number and no wrong reasoning in any new test.** The "why" comments (which rung,
which leaf, which sub-threshold) are also correct in every case I checked.

### 1.2 Bowtie 2 / HISAT2 are untouched *structurally*

Not inferred from the green suite. `from_emitted`'s new `match`:

```rust
Aligner::Bowtie2 if local => BOWTIE2_LOCAL_MATCH_BONUS,
Aligner::Minimap2 | Aligner::Rammap => MINIMAP2_MATCH_BONUS,
_ => 0.0,
```

Bowtie 2 with `local == false` does not match the guarded arm and does not match the
`Minimap2 | Rammap` pattern, so it falls to `_ => 0.0`; HISAT2 falls to `_` for both `local`
values. That is byte-for-byte the old `if local && aligner == Aligner::Bowtie2 { 2.0 } else
{ 0.0 }`. `normalize()`, `perfect()`, `local_ladder()` and both ladder functions have
**doc-comment-only** diffs — no executable line changed. `Aligner` still has exactly four
variants (`config.rs:21-37`), so there is no in-process rammap variant the arm could miss.

Single production construction site, `config.rs:840-846`, passing the resolved `aligner` —
so the arm is live in production, and `minimap_like()`'s hardcoded `local = false` is safe
because `--local` is genuinely rejected for both aligners (`config.rs:717-728`).

### 1.3 Fault injections — I ran five, in an isolated copy of the tree

I did **not** mutate the working tree (Reviewer B was editing the same files concurrently);
I rsynced `rust/` to a scratch dir with its own `CARGO_TARGET_DIR` and injected there.

| Fault | Result | Note |
|---|---|---|
| (iii) `MINIMAP2_MATCH_BONUS = 1.0` | **6 unit tests fail**, incl. `minimap_like_matches_independent_reference` | Confirms the literal `2.0` in `positive_bonus_reference` keeps V4 non-vacuous — Reviewer B's I6 concern is genuinely closed |
| (v) drop `\| Aligner::Rammap` | `score_model_construction_matrix` **and** the rammap BAM cell fail (`left: 42, right: 44`); the minimap2 BAM cell stays **green** | ⚠️ §12's table says "rammap BAM **only**" — the construction matrix catches it too (it loops `[Minimap2, Rammap]`). The rammap BAM cell is the only *end-to-end* detector, which is still the point |
| (vi) production score-min form → `Log` for minimap-like | **all 21 mapq unit tests stay green**; only minimap2 BAM cell (b) fails (`left: 36, right: 41`) | The sharpest fault in the set, and cell (b) is its **sole** detector. Confirms A-I2 / rev 2's `AS:i:5`→`AS:i:7` change was load-bearing, and that resolving the A-vs-B contradiction in A's favour was correct |
| **mine:** gate the arm on `!local` (`Minimap2 \| Rammap if !local`) | construction matrix fails | The `for local in [false, true]` loop has teeth — production could never reach `local = true`, so without the loop this would ship green |
| **mine:** remove `normalize`'s `.max(1.0)` clamp | V14's clamp cells + the pre-existing `local_diff_is_clamped_to_at_least_one` fail | The rev-2 clamp finding is properly pinned |

### 1.4 V15 is a real gate with margin

Reproduced the new test's experiment directly against minimap2 2.31-r1302 (same reference
generator, same reads, same option set):

```
-x map-ont   primary_hits=16  perfect_cells=4  violations=[]
-x map-pb    primary_hits=16  perfect_cells=4  violations=[]
-x sr        primary_hits=16  perfect_cells=4  violations=[]
```

12 perfect cells against the `>= 6` floor (2× margin), 0 `AS > 2·len`, and `AS == 2·len`
**exactly** for every perfect read under every preset — including `sr`, where the `end_bonus`
leak would have shown. The `!hits.is_empty()` per-preset guard plus the `perfect_cells >= 6`
guard mean it cannot pass vacuously, and the panic-if-`$CI` gate is correct.

I also confirmed **all** CI jobs that run the bismark suite install minimap2 (`rust_ci.yml:41`,
`:99`, `:137`), and that `perl-oracle` uses `-- --exact <13 names>`, so V15 never runs there and
cannot trip its `EXPECTED=13` / `^skipping:` checks.

### 1.5 Other gates

- V13's grep returns **0** surviving hits. The remaining `Bowtie 2-only` strings in `src/` are
  all about unrelated features (`--multicore`, `--combined_index`, `--reorder`) — correct.
- `cargo fmt -p bismark -- --check` clean.
- Fixture facts checked against source: `make_genome_mmi`'s `chr1` is `ACGTACGT` (8 bp) ✓;
  `methylation.rs:154-159`'s guard is `pos < 2` on a 0-based pos, so SAM POS 3 passes and the
  window is `chr[0..2] + chr[2..8]` = 8 = `read_len + 2` ✓; the merge's running-maxima gate
  (`merge.rs:257-279` `>=`, insertion at `:283-310`) and `second_for_mapq` (`:345-350`) behave
  as the fixture comment says (modulo LOW-4) ✓. The GA record's SEQ `ACATAC` is the correct G→A
  conversion of `ACGTAC` — a nice touch. The HashMap key is `format!("{chromosome}:{}", rec.pos)`
  (`merge.rs:399`), so distinct POS is genuinely required, as the comment states.

---

## 2. Findings

### HIGH-1 — The shipped docs, CHANGELOG and README all state a MAPQ floor of 22 and that `-q 20` stays a no-op. Both are false, in the direction that costs users reads.

The local ladder's floor is 22 **only in the no-second-best branch**. Once a cross-instance
runner-up exists, the same ladder returns values far below 22. Verified against the Perl oracle
at len 100 (`perfect = 200`, `diff = 220`):

| AS | AS₂ | old MAPQ | **new MAPQ** |
|---|---|---|---|
| 80 | 70 | 25 | **2** |
| 60 | 50 | 25 | **2** |
| 22 | 0 | 33 | **9** |
| 90 | 69 | 33 | **11** |
| 44 | 0 | 33 | **12** |
| 200 | 190 | 25 | **11** |

Full set of reachable values with a runner-up: 2, 9, 11, 12, 14, 16, 17, 18, 19, 21, 25, and
31–40. Ten of them are below 22 and every one of those is below a conventional `-q 20`.

Three surfaces assert otherwise:

1. **`docs/src/content/docs/options/alignment.md:314`** — *"MAPQ therefore **ranges from 22**
   (a read scoring well below its best possible score) **to 44**."* Flatly false, and this is
   the page a user consults before deciding whether to re-check their filters.
2. **`CHANGELOG.md:14`** — *"**the floor is 22, not 0**, so a `-q 20`-style filter is still
   close to a no-op — this fixes MAPQ's *discrimination*, not its usefulness as a hard
   filter"*. Self-contradictory: two sentences earlier the same bullet says a near-tie
   *"drops from 25 to **11**"*, which a `-q 20` filter drops.
3. **`rust/README.md` Milestones (2026-08-01 line)** — *"the floor is **22**, so this restores
   MAPQ's discrimination but not its usefulness as a hard filter"*. Same claim.

**Why this matters more than a wording slip.** The reassurance is doing real work in the release
notes: it is the sentence that tells a minimap2 user they do not need to revisit downstream
`samtools view -q` / methylseq filtering. The exact boundary at len 100 (verified against the
oracle): a read whose runner-up is within `0.1 × diff` (< 22 points) scores **11** if
`bestOver >= 0.5 × diff`, i.e. `AS >= 90` (45 % of perfect), and **2** below that —
`AS 89 / AS₂ 88 → 2`, `AS 90 / AS₂ 89 → 11`. Those reads were 25 or 33 before and passed
`-q 20`; they will now be discarded. That is exactly the hazard #1080's code review raised as
its HIGH-1 — here in a **default** path, and currently documented as impossible.

This is not a new *ladder* bug: pre-fix, an extreme near-tie could also fall off (`200/199` →
**6**). What changes is the **population** — pre-fix a read needed `bestDiff < 2` at len 100 to
reach that leaf, post-fix it needs `bestDiff < 22`, an order of magnitude wider.

**Also note the claim is inherited from the plan** (§3.4 *"The local ladder's floor is 22, above
a conventional `-q 20`, so a MAPQ filter remains close to a no-op"*; V3 *"The floor"*), and it is
load-bearing in §3.4's argument for D-LADDER ("the end-to-end ladder's floor is 0 ⇒ it would
newly drop uniquely-aligned reads from `-q 1`"). The local ladder *also* newly drops reads, just
in the other branch. So this is a decision for Felix, not a copy-edit — which is why I have not
changed the prose.

**Recommendation.** Scope the claim in all three places, e.g.:

> A uniquely-aligned read with no cross-instance runner-up now scores between 22 and 44, so for
> those reads a `-q 20` filter remains close to a no-op. Reads whose best alignment has a
> runner-up from another strand instance are scored on the second-best branch of the ladder and
> **can drop as low as 2**, so `-q`-filtered output downstream of `--minimap2`/`--rammap` will
> lose reads it previously kept. This fixes MAPQ's discrimination; it does not leave filtering
> unchanged.

and drop "ranges from 22 to 44" from the docs page. If the ≤21 population turns out to be large
in real data it is worth a sentence quantifying it, but the categorical claim has to go either
way.

---

### HIGH-2 (found independently; **already fixed** by Reviewer B during this review) — the rammap BAM gate broke `cargo test -p bismark --features rammap-inprocess`, a live CI job.

As written, `rammap_mapq_uses_the_perfect_score_denominator_end_to_end` passed plain `--rammap`
with a fake subprocess binary. On a `--features rammap-inprocess` build `--rammap` **defaults to
the in-process backend** (shipped 3.1.0 behaviour), so the fake is bypassed. I confirmed the
failure empirically before the fix landed:

```
error: failed to load rammap index …/BS_CT.mmi: Empty index file
test result: FAILED. 0 passed; 1 failed
```

`rust_ci.yml:101-102` runs exactly that command as its own job, so this would have gone red in
CI while the author's local `cargo test -p bismark` (the gate §12 reports) stayed green at 2120.
The neighbouring `rammap_se_mapped_names_report_and_notice` already documents the fix and the
reason (`aligner_cli.rs:3061-3067`), which is what pointed me at it.

Reviewer B applied `.arg("--rammap_subprocess")` with an equivalent comment while I was
verifying; the fix is correct and does not weaken V7 (the `ScoreModel` is resolved from
`Aligner::Rammap` in `config::resolve`, independent of backend). I verified `--rammap_subprocess`
is accepted on a **default**-feature build too, so the test is backend-stable on both.

**Two reviewers reaching this independently is the signal**: the release/conda/Docker
configuration is a build the local gate does not cover. Worth adding
`cargo test -p bismark --features rammap-inprocess` to the pre-push checklist for any PR that
touches a `--rammap` test.

---

### MEDIUM-1 — "a fifth backend cannot inherit `_ => 0.0` without failing here" is not enforced.

`mapq.rs:916-918`:
> **Every aligner is classified explicitly**, so a fifth backend cannot inherit
> `from_emitted`'s `_ => 0.0` arm without failing here.

and `config.rs:207-208`:
> `score_model_construction_matrix` classifies every aligner explicitly, so a fifth backend
> cannot inherit the `_ => 0.0` arm without failing a test.

Both are false. The test iterates two **hardcoded arrays** (`[Bowtie2, Hisat2]`,
`[Minimap2, Rammap]`); adding `Aligner::Foo` compiles and this test passes untouched, with
`Foo` silently taking `_ => 0.0` and the end-to-end ladder. (The pre-#1081 version had the same
weakness, but it did not claim enforcement — and PLAN.md §11 risk 6 names this test as
*"now the only tripwire"* for exactly this case, so the claim was supposed to become true here.)

This is the plan's own "dominant defect class" (§11 risk 7): an authoritative-looking comment
that a diff-only review would accept.

**Recommendation** — make it true rather than soften it; the `match` without a `_` arm is what
does the work:

```rust
for aligner in [Aligner::Bowtie2, Aligner::Hisat2, Aligner::Minimap2, Aligner::Rammap] {
    // EXHAUSTIVE on purpose: a fifth backend fails to COMPILE here until it is classified,
    // rather than silently inheriting `from_emitted`'s `_ => 0.0` arm (#1081).
    let minimap_like = match aligner {
        Aligner::Bowtie2 | Aligner::Hisat2 => false,
        Aligner::Minimap2 | Aligner::Rammap => true,
    };
    …
}
```

---

### MEDIUM-2 — the CHANGELOG under-specifies the second-best condition, and omits the strand-order artefact entirely.

PLAN.md §7 step 9 is explicit: the second-best population must be *"phrased as that condition"* —
a **later**-slot instance that **strictly** out-scores every earlier one — *"**not** as 'reads
that map to both strands'"*, because getting that wrong was rev 1's root-cause error.

Shipped text (`CHANGELOG.md:14`): *"Reads whose runner-up score comes from a **different strand
instance** move in both directions"*. That is the framing the plan rejected: it omits the
ordering condition. A user whose read maps to both strands with **CT** winning sees `42 → 44`,
not `25 → 11`, and will conclude the note is wrong.

Two related gaps in the same bullet:

- *"**MAPQ was 42 for every uniquely-aligned minimap2 read**, at every read length and every
  alignment score"* — not true of unique reads that had a recorded cross-instance runner-up
  (they were 25/27/33). The bullet corrects itself a sentence later, but the headline claim as
  written is the one people quote.
- The **strand-order artefact** appears nowhere user-facing. PLAN.md §3.5 says it *"Must be
  stated, not discovered by a user diffing two BAMs"*: two reads with identical `(AS, AS₂)` get
  44 or 11 depending only on which strand won, and this change widens that spread from 17 points
  to 33.

**Recommendation.** Replace "from a different strand instance" with "from a later strand
instance that scored strictly higher than every earlier one (roughly half the reads that align
to both)", qualify the "42 for every" headline with "with no such runner-up", and add one
sentence on the strand-order spread.

---

### LOW-1 — the rammap BAM cell has no form-sensitive value.

Measured under fault (vi): the rammap gate stays **green** when the production score-min form is
wrong, because `AS:i:12` on a 6 bp read returns 44 under both a linear and a logarithmic `scMin`.
Only the minimap2 `AS:i:7` cell catches it. That is fine as designed (one shared code path), but
it means the rammap gate proves *arm coverage* and nothing else. Optional: add an `AS:i:7` rammap
cell, or state in the rammap test's doc that cell (b) carries the form for both aligners.

### LOW-2 — `aligner_minimap2_as_bound.rs` option order is not Bismark's.

The module doc says *"Bismark's minimap2 option string, verbatim, minus the varied `-x
<preset>`"*, but the test appends `-x <preset>` **last**, after `-K 250K`, whereas Bismark emits
`-a --MD --secondary=no -t 2 -x map-ont -K 250K`. minimap2 applies a preset at the point it
parses `-x`, so anything the preset sets overrides an earlier flag rather than the other way
round — i.e. the *effective* `-K` in the test is whatever the preset chooses, not `250K`.

The gate is still valid: in my replication (identical ordering) all three presets emitted SAM
with `AS:i:`/`MD:Z:` and exactly one primary per read, so `-a`, `--MD` and `--secondary=no`
demonstrably survived, and the only option that can differ is `-K`, which is I/O batching and
cannot move a score. It is the word "verbatim" that overstates it. Cheapest fix: move
`-x <preset>` before `-K 250K`, which makes the string genuinely verbatim and removes the
question.

### LOW-3 — `positive_bonus_reference` did not get §7 step 5's safeguard, and the deviation is unrecorded.

The plan asked for either an internal `assert!(match_bonus > 0.0)` or in-function ladder
derivation. The implementation added a comment instead. This is **benign and arguably better** —
the function takes no `match_bonus` parameter at all, so the trap cannot arise — but §12's
deviations list does not mention it. Worth one line there so the next reader does not re-derive
the concern.

### LOW-4 — cell (d)'s fixture comment gets the *equal-score* case slightly wrong.

`make_fake_minimap2_two_instance`'s comment: *"the **later** slot must score **strictly higher**
or its record is never stored and no second best exists."* The practical instruction is right,
but "never stored" only describes a strictly-**lower** later slot. An **equal** score does set
`overwrite` (`merge.rs:267` is `>=`, as the comment itself quotes) and *is* stored — the read then
dies at the cross-instance tie check (`merge.rs:338`) as `Decision::Ambiguous`, so it is never
written at all. PLAN.md §7 step 7(d) constraint 1 stated this correctly ("equal ⇒
`Decision::Ambiguous`, `merge.rs:338-342`"); the fixture comment is the place the plan asked to
carry all three constraints, so it is worth one word. Either failure mode is caught loudly
(`unique best alignments:   1`), so this is documentation only.

### LOW-5 — the `--rammap` docs section says nothing about MAPQ.

The MAPQ block landed under `--minimap2` (`alignment.md:314`) and ends with *"The same applies to
`--rammap`."* The `--rammap` section itself (`:341`) has no note and no cross-reference, so a
rammap-only reader will not find it. PLAN.md §7 step 8 asked for the two sections
cross-referenced; only one direction shipped.

### LOW-6 — CI's minimap2-install step comments are now incomplete.

All three read *"Install minimap2 (5-Base #787 ground-truth gates)"*. They now also serve
#1081's V15 gate. Cosmetic only — V15 panics rather than skipping when `$CI` is set, so it
cannot be silently disarmed — but the comment is the thing a future 5-Base cleanup would read.

---

## 3. Corrections to the record (no code change needed)

- **§12's fault table, row (v)** says *"Detected by: **rammap BAM only**"*. Measured:
  `score_model_construction_matrix` also fails (it loops `[Minimap2, Rammap]`). The rammap BAM
  cell is the only **end-to-end** detector, which is the claim that actually matters.
- **§1's `--score_min` sensitivity box** gives `L,0,−0.6` → `44/44/**42**/36/24`. The oracle
  gives `44/44/**41**/36/24`: at len 100, `bestOver/diff = 180/260 = 0.6923 < 0.7`, so it is the
  0.6 rung (41), not the 0.7 rung (42). Documentation only — `−0.6` is not one of the slopes
  asserted in code, and the row's qualitative point ("still discriminates") is unaffected.
- **§12's test delta is off by one.** The **total, 2120, is confirmed** (I measured
  `passed=2120 failed=0 ignored=20` across 78 binaries), but the diff adds **10** tests, not 11:
  5 in `mapq.rs::tests`, 1 in `config.rs::tests`, 3 in `aligner_cli.rs`, 1 in the new
  `aligner_minimap2_as_bound.rs` (`git diff | grep -c '^+.*#\[test\]'` = 9, plus the new file's
  1). So the stated prior total must be 2110, not 2109. Immaterial, but the count is quoted as
  evidence.
- `end_to_end()`'s doc says *"~47 call sites"*; the actual count is 50 `ScoreModel::end_to_end(`
  occurrences (`combined.rs` 16, `mapq.rs` 30, `merge.rs` 9, `config.rs` 1, `convert.rs` 1).
  The "~" makes it fine.
- `end_to_end()`'s doc says the `merge.rs`/`combined.rs` selection tests *"none of them asserts
  a MAPQ value"*. `combined.rs:1140` (`select_unique_best_mapq_equals_calc_mapq`) does assert
  one, but derived from the same model, so it is aligner-agnostic as claimed. Harmless.

---

## 4. Things I looked for and did **not** find

- **A seventh silent fault in the code.** I tried: mis-scoping the arm to include Bowtie 2 or
  HISAT2 (caught by the matrix + `end_to_end_matches_the_pre_fix_formula`); gating it on `local`
  (caught by the matrix's `local` loop); removing the clamp (caught by V14); loosening
  `local_ladder()` to `>= 0.0` (caught by the matrix); building `minimap_like` with
  `Aligner::Rammap` instead of `Minimap2` (no behavioural difference — not a fault); using
  `BOWTIE2_LOCAL_MATCH_BONUS` in the minimap2 arm (indistinguishable while both constants are
  2.0, which is exactly why the plan wanted them separate — no test can close this and none
  should try). The two real "ships green through the author's suite" faults are HIGH-2 (the
  feature build) and HIGH-1 (prose, which nothing gates).
- **Any Bowtie 2 / HISAT2 behaviour change.** Structurally impossible per §1.2, and
  `local_bowtie2_matches_independent_reference`'s coverage is unchanged by the `log_form`
  parameterization: `bowtie2_local_reference` is a thin `log_form = true` wrapper, still passing
  literal `20.0, 8.0`, still hitting `calc_mapq_local`, still exercising the PE mate-sum cells.
- **A weakened V4.** Fault (iii) fails it, so the literal `2.0` works as intended.
- **A vacuous cell (d).** `minimap2_bam_mapq` asserts `unique best alignments:   1` on stderr, so
  a fixture that collapsed to one entry (44) or went `Ambiguous` fails loudly, and the failure
  message names the slot-order cause.
- **Any missing `calc_mapq` reachability.** The four call sites are as §11 states; `combined.rs`
  is rejected for these aligners (`config.rs:1027`, `:1073`) and PE at `:577`, both re-checked.

---

## 5. Fixes applied by me

**None.** The one unambiguous code fix I intended to make (HIGH-2's `--rammap_subprocess`) was
applied by Reviewer B while I was verifying it, and I confirmed their version is correct. I made
no further edits: a second reviewer was writing to the same files, and the remaining findings are
either prose decisions for Felix (HIGH-1, MEDIUM-2) or small structural test changes better made
once, with the author's intent (MEDIUM-1).

## 6. Gate status as I measured it

All measured by me on the current working tree (i.e. *including* Reviewer B's
`--rammap_subprocess` fix), not taken from §12.

| Gate | Result |
|---|---|
| `cargo fmt -p bismark -- --check` | **clean** |
| `cargo clippy -p bismark --all-targets` | **0 warnings, 0 errors** |
| `cargo test -p bismark` | **exit 0 — 2120 passed / 0 failed / 20 ignored**, 78 binaries (matches §12's total) |
| `cargo test -p bismark --features rammap-inprocess` | **exit 0 — 2127 passed / 0 failed / 20 ignored** (the CI job that HIGH-2 would have broken; it failed 1 before the fix) |
| V13 doc grep (§7 step 2) | **0 hits** |
| V15 against real minimap2 2.31-r1302 | **passes** — 12 perfect cells vs a floor of 6, 0 `AS > 2·len`, `AS == 2·len` exactly for every perfect read under all 3 presets |
| Perl-oracle re-derivation of every new expectation | **100 % agreement**, no expectation edited |
| 5 fault injections (3 of the plan's + 2 of mine) | **all 5 detected**, each in the predicted place and at the predicted value |

**Bottom line:** the code is right and the gates are the strongest of the three issues in this
lineage. What needs to happen before merge is HIGH-1 (three prose surfaces state a MAPQ floor
that the ladder does not have, in the direction that costs users reads) and, ideally, MEDIUM-1
and MEDIUM-2 in the same pass. Nothing I found requires touching `from_emitted`, `normalize`,
`perfect`, `local_ladder`, or either ladder.
