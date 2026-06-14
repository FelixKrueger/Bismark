# IMPL_REVIEW_B — HISAT2 `--local` (Reviewer B: test-quality / regression / gate angle)

**Target:** `plans/06142026_aligner-hisat2-local/IMPL.md` (TDD task list).
**Scope plan:** `PLAN.md` rev 2 (dual-reviewed). Locked context (do/SE+PE/minimap2-rejected,
MAPQ fix = `score_min_params` not `calc_mapq`) treated as settled.
**Verdict: APPROVE-WITH-CHANGES.** The plan is correct, the seams are accurately located, and the
core architectural claim (only `score_min_params` needs the aligner branch; `calc_mapq` is
sign-agnostic) is source-verified true. But the headline non-vacuity guards have **two real
test-quality holes** (Task 4 MAPQ regime + the e2e/gate soft-clip "toggle does something" check)
that, if implemented literally as written, can pass green while proving little. Fix those two and a
stale-base note before implementing.

All file:line citations verified against the source in `/Users/fkrueger/Github/Bismark-hisat2mc/`.

---

## 1. Logic / seam accuracy

Verified every seam in the IMPL "Key seams" table against source — all accurate:

- `config.rs:295` reject (`aligner != Aligner::Bowtie2`), msg "only supported with Bowtie 2" — confirmed.
- `options.rs:347` `score_min_params(cli: &Cli)` — confirmed single-arg, keys on `cli.local` alone,
  hardcodes G-form `(20.0, 8.0)` (lines 348-352). This IS the load-bearing bug. ✅
- Call site `config.rs:360` `options::score_min_params(cli)?` — confirmed; `aligner` is in scope
  (used at `config.rs:355` for `build_aligner_options`), so threading it in is a clean local change. ✅
- Local block `options.rs:82` (`debug_assert_eq!` at `:83`), HISAT2 tail `options.rs:328`
  (`tail.push("--no-softclip --omit-sec-seq")`) — confirmed. ✅
- Reject test `config.rs:1060` `resolve_rejects_local_with_non_bowtie2` — confirmed (asserts
  HISAT2 Err + minimap2 Err today). ✅
- Docs: `rust/README.md:61-62`, `cli.rs:169` (`--local` help), `config.rs:178` (`score_min_local`
  doc) — confirmed all present and stale. ✅

**Control-flow correctness (no double-emission):** the local `--score-min` push lives inside
`if cli.local { … }` (options.rs:82-103) and the end-to-end push in the `else` (`:104-120`). The
HISAT2-local arm correctly belongs INSIDE the `if cli.local` block (emitting the L-form there), so
there is no risk of a stray end-to-end `--score-min` being appended. The IMPL is precise on this. ✅

**MAPQ wiring is genuinely aligner-agnostic (verified, satisfies prompt Q4):**
- `calc_mapq` (`mapq.rs:18-135`) + `calc_mapq_local` (`:142-219`): the local ladder uses
  `diff = sc_min.abs()` (`mapq.rs:42`) and `best_over = as_best - sc_min` (`:43`) — **sign-agnostic**,
  so the negative-slope `(0,−0.2)` HISAT2 case needs no code change. ✅
- PE is wired for the local branch, not just end-to-end: `merge.rs:736-744` passes
  `Some(sequence_2.len())` + `score_min_local` into `calc_mapq`, and `calc_mapq` adds the second
  `ln(l2)` term under `if local` (`mapq.rs:35-41`). SE path `merge.rs:359-367` passes `None`. ✅
- `config.rs:361` `score_min_local = cli.local` is just a bool and needs no aligner awareness — only
  the resolved `(intercept, slope)` from `score_min_params` must become `(0,−0.2)` for `hisat2 && local`.
  The IMPL's claim "the MAPQ fix is `score_min_params`, not `calc_mapq`" is **correct**. ✅

---

## 2. Test-quality (the central concern)

### 2a. 🟠 Task 4 MAPQ test — the prescribed regime is **largely vacuous** as written

The IMPL guidance ("read lengths 40/50/75/100/150 × representative `AS_best`, transcribe expected
buckets") is directionally right (avoids self-consistency) but **under-specifies the regime**, and a
literal implementation will likely prove only the two extreme buckets. I computed the actual ladder
behaviour for `(0,−0.2)`:

- `diff = 0.2·ln(readLen)` is **sub-unity**: 0.738 (L40) / 0.782 (L50) / 0.863 (L75) / 0.921 (L100)
  / 1.002 (L150). The bucket widths are `~0.1·diff ≈ 0.07–0.10`.
- **SE, no second-best, integer `AS_best`:** `best_over` jumps by 1.0 per unit of AS while buckets are
  ~0.1·diff wide → **only buckets 44 (AS≥0) and 22 (AS<0) are reachable.** The intermediate
  42/41/36/28/24 buckets are unreachable with integer AS. A test sweeping AS∈{0,−1,−2} no-2nd proves
  *nothing* about the `ln()`-sensitive interior.
- **SE, with second-best:** `best_diff = ||AS_best|−|AS_second|| ` is an integer ≥1, and `diff≈1.0`, so
  `best_diff ≥ 1 ≥ diff·0.9` → **always bucket 40** unless `best_diff==0`. The interior best_diff
  buckets (39/38/37/35/34/…) are unreachable with integer AS.
- **PE** adds at most one intermediate bucket (e.g. `sumAS=−1` → 24 at L50/L50, 28 at L75, 36 at L150).

**The genuinely `ln()`-ULP-sensitive points** are the **exact-equality leaves** `best_over == diff`
(local ladder leaves 35/34/33/32/31 at `mapq.rs:173,181,189,197,205`). These fire precisely when
`AS_best == 0` (so `best_over == sc_min.abs() == diff`) WITH a second-best present. If Rust's `ln()`
differed from Perl's by 1 ULP, `best_over == diff` would flip the leaf. That is the assertion that
actually exercises the transcendental.

**Action (Important):** Task 4 must mandate, at minimum:
  (i) the `best_over == diff` exact-equality leaf — `calc_mapq(L, None_or_Some(L2), AS_best=0,
      AS_second=Some(s), 0.0, −0.2, local=true)` across the read lengths, asserting the `==diff`
      bucket (35/34/33/32/31 depending on `best_diff`), since this is the only `ln()`-ULP-sensitive
      path with integer inputs;
  (ii) at least one PE case where the summed-`ln()` `sc_min` lands an *intermediate* no-2nd bucket
      (e.g. L50/L50 sumAS=−1 → 24, L150/L150 sumAS=−1 → 36) — proving the two-`ln()`-term sum, not
      just one;
  (iii) optionally a non-integer `AS_best` (or a user L-form with a steeper slope, e.g. `L,0,-0.6`)
      to land an interior bucket like 42/41 deliberately.
  The IMPL should state *which buckets must be non-trivially covered*, not just "representative
  AS_best", or the implementer will write the vacuous AS∈{0,−1} sweep. Transcribing the expected
  bucket integers (a small literal table, like the existing `inner_threshold_leaves_pinned`
  `mapq.rs:269-311`) is the right shape — that table form is exactly what's needed here.

(Note: `mapq.rs:370` already has `local_calc_mapq_uses_ln_scmin_and_local_ladder`, but it uses the
Bowtie 2-local `(20,8)` constants and proves `calc_mapq`==`calc_mapq_local`-fed-ln by *self*
construction — it does NOT cross-check Perl and does NOT touch the sub-unity `(0,−0.2)` regime. So
Task 4 is genuinely new coverage, good — just make it non-vacuous per above.)

### 2b. 🟢 Task 5 e2e soft-clip round-trip — mechanically sound; one strengthening

The mechanism is proven: the harness already reads the output BAM via
`bismark_io::BamReader::from_path` and inspects records incl. CIGAR/MAPQ/tags
(`cli.rs:402-416`), and asserts the report `aligner_options` line verbatim
(`hisat2_se_mapped_names_and_report`, `cli.rs:2030`). So both Task 5 assertions are feasible. The
fake-HISAT2 soft-clip variant (an awk that emits `2S4M` against the 8 bp `ACGTACGT` genome, with a
length-consistent SEQ + adjusted `MD:Z:4`) is a faithful exerciser of `methylation.rs:174` (`b'I' |
b'S' =>` treats S like I: pads X, no pos advance). ✅

**Two corrections / strengthening (Important + Optional):**
- 🟠 **`2S62M` is infeasible against the test fixture.** Task 5 (IMPL:84) says "e.g. `2S62M`", but the
  genome fixture `make_genome_ht2` is 8 bp (`cli.rs:1967`, `ACGTACGT`) and reads are ~6 bp. A 64 bp
  CIGAR cannot align. The test-infra line (IMPL:43) correctly says `2S4M`. **Use `2S4M`** (consumes
  4 ref bases, fits the 8 bp genome) and fix the `2S62M` mention to avoid a confused implementer.
  Also: the soft-clipped SEQ must remain length-consistent (`2S4M` ⟹ a 6 bp SEQ, of which 4 align)
  and `AS`/`MD` adjusted so the methylation walk doesn't run off `chr.len()` (the walk has an
  edge-guard at `methylation.rs:203`, but the CIGAR/SEQ/genome must be internally consistent or the
  call result is meaningless).
- 🟢 (Optional) Assert the **e2e MAPQ comes from the LOCAL ladder**, not just that an `S` exists. The
  end-to-end fake yields MAPQ 42 (`cli.rs:408`); a `--hisat2 --local` no-2nd-best read yields the
  local top leaf 44 (per §2a). Asserting `mapping_quality == 44` (vs 42) in the e2e proves the
  `score_min_local=true` path actually fired end-to-end — a cheap, high-value non-vacuity bonus that
  ties Task 4 (the ladder) to Task 5 (the wiring).

---

## 3. Regression-safety (prompt #2 — the big risk)

I checked the byte-frozen surfaces the IMPL edits (`options.rs:82` local block + `:328` HISAT2 tail)
and the guard tests. **Conclusion: the existing guard tests are adequate AND the IMPL keeps them** —
but the IMPL should *name* the must-stay-green regression tests explicitly rather than rely on
"full module green".

Guard tests that pin the byte-frozen strings (all present, must NOT be deleted/relaxed):
- Bowtie 2-local (#981): `accepts_local_for_bowtie2_emits_local_and_g_score_min` (`options.rs:478`),
  `local_custom_g_score_min_accepted_l_rejected` (`:497`) — pin `--local --score-min G,20,8` +
  G/L validation. The Task 3 GREEN ("Bowtie 2: push `--local` + G-form (current, byte-frozen)")
  keeps these green by construction. ✅
- HISAT2 end-to-end: `hisat2_se_option_string` (`:555` — `…--no-softclip --omit-sec-seq`),
  `hisat2_pe_option_string_has_no_dovetail` (`:607`), `hisat2_multicore_remap_emits_p_reorder`
  (`:570`), `bowtie2_hisat2_strings_byte_frozen_alongside_minimap2` (`:776`). These pin the
  **end-to-end** tail; since the IMPL only changes the tail under `if cli.local` (and these tests have
  no `--local`), they stay green. ✅ The key risk — an implementer making the tail `--omit-sec-seq`
  unconditionally — IS caught by `hisat2_se_option_string` (it asserts the literal
  `--no-softclip --omit-sec-seq`). Good safety net.
- `score_min_params_local_defaults_and_parses_g` (`options.rs:516`): the IMPL correctly flags this as
  the one that won't compile after the signature change (Task 2 RED) and must be updated, not deleted.
  The update must KEEP the Bowtie 2-local `(20,8)` + end-to-end `(0,−0.2)` cases (regression) and ADD
  the HISAT2-local `(0,−0.2)` case. ✅

**Action (Important):** the IMPL's Task 3 "Verify" says "regression — Bowtie 2-local + HISAT2
end-to-end strings byte-unchanged" but doesn't name the tests. Add the explicit list above to the
Task 3 verify step so the implementer (and the later plan-manager) can confirm each named test still
asserts the same literal.

**methylseq conformance suite — verified NOT broken, but the IMPL omits it.** There is a
`tests/methylseq_conformance.rs` (the README:188 "flip-detecting KnownUnsupported rows"). I read it:
- `methylseq_align_local_now_accepted` (`:188`) tests **Bowtie 2** `--local` only (byte-frozen). ✅
- `methylseq_align_hisat2_multicore_now_accepted_via_p_threading` (`:214`) — HISAT2-multicore,
  already lifted by #986. ✅
- **Nothing asserts `--hisat2 --local` is rejected** — so this IMPL does NOT break the conformance
  suite (the PLAN:185 claim "no conformance flip needed" is correct). **But the IMPL never mentions
  this suite at all.** It should be listed as a regression-guard that must stay green (a future
  reviewer/plan-manager shouldn't have to rediscover it). Optional but worth one line.

---

## 4. Validation-sufficiency (prompt #3 — the oxy gate non-vacuity)

The IMPL's blocking `samtools view | awk '$6 ~ /S/' | wc -l > 0` (IMPL:101) is **necessary but not
sufficient** to prove the toggle does something:

- 🟠 **"`S`-count > 0" alone can pass without the toggle mattering.** HISAT2 end-to-end with
  `--no-softclip` can still occasionally soft-clip in some edge configurations, and more importantly
  the gate compares Rust-local vs **Perl-local** (same oracle) — so if BOTH produce the same
  soft-clips, you've proven byte-identity to Perl-local but NOT that dropping `--no-softclip` changed
  anything vs end-to-end. The headline ("drop `--no-softclip`") is only proven if local actually
  *differs* from end-to-end on this dataset. **Add a cross-check:** run the SAME dataset through
  `--hisat2` (end-to-end) and `--hisat2 --local`, and assert the soft-clipped-read set is
  **non-empty AND differs** (the end-to-end run should have ~0 `S` CIGARs since `--no-softclip` is
  passed; the local run should have >0). That delta is what proves the toggle is load-bearing — not
  the bare existence of `S` in the local BAM. This is the single most important gate strengthening.
- 🟡 **"Use a soft-clip-prone dataset … if the clean WGBS subset yields ~0" is not concrete enough to
  execute.** A directional WGBS subset of clean trimmed reads frequently yields *zero* soft-clips even
  in local mode (the reads align end-to-end fine), which would make the gate vacuous and force an
  ad-hoc scramble for data on oxy. The IMPL should name a concrete soft-clip-inducing approach BEFORE
  the gate run, e.g. (a) reads with deliberate untrimmed adapter tails, or (b) reads with a few
  appended non-genomic bases, or (c) a longer-read subset — and state the fallback order. Otherwise
  the "blocking" assertion risks blocking the gate itself with no ready remedy. Pair this with the Q4
  determinism prereq (already correctly blocking, IMPL:102).
- 🟢 The `--multicore N` cell (IMPL:100) is well-placed — it proves local + the #986 `-p N` remap
  compose, and the option-assembly order is already pinned by `hisat2_multicore_remap_emits_p_reorder`
  so the wall-clock-filtered report compare will catch a mis-order. ✅
- 🟢 The matrix (SE+PE × dir/non-dir/pbat) mirrors the #981/#986 gates and the PE second-mate `ln`
  term is verified wired (§1). ✅

---

## 5. Effort / sequencing / commit hygiene (prompt #5)

- 🟠 **Stale base — the worktree is NOT a fresh branch off iron-chancellor.** The worktree
  `/Users/fkrueger/Github/Bismark-hisat2mc` is checked out on `rust/aligner-hisat2-multicore` whose
  HEAD is `6260a1c` (a local `beta.8` bump) on top of the **unmerged** local multicore commit
  `69222f7` + a local merge `3969404`. Meanwhile `origin/rust/iron-chancellor` is already at **beta.9**
  (`478c974`), and #986 (HISAT2-multicore) is **merged upstream** as the squash `4b93f5b`
  (`git merge-base --is-ancestor 69222f7 origin/rust/iron-chancellor` → NOT an ancestor; the local
  commit is the pre-squash version). **If the implementer branches from this worktree's HEAD they will
  carry a now-redundant duplicate multicore commit + a stale local beta.8 bump that conflicts with the
  merged beta.8→beta.9 path** — exactly the duplicate-prior-phase-commit trap the project has hit
  before. The IMPL's instruction ("fresh worktree off latest `rust/iron-chancellor`") is **correct and
  load-bearing** — but it says "beta.8" when upstream is now **beta.9**. **Action:** update the base
  note to "beta.9 / `478c974`" and make explicit: create a NEW branch off `origin/rust/iron-chancellor`
  (do NOT branch from the current `rust/aligner-hisat2-multicore` worktree HEAD). Confirm `4b93f5b`
  (#986) is the base's multicore, so the HISAT2-multicore tests/route are present.
- 🟢 **No stray test-output files** currently (`git status --porcelain rust/bismark-aligner/` clean; no
  `reads_bismark_bt2.*` strays). The prior multicore-work trap is not present here, but the IMPL's
  commit-plan should still add a `git status` check before commit (cheap insurance, since the e2e
  tests write to `TempDir` and shouldn't leak, but the project has been bitten before).
- 🟢 **One-commit plan is realistic.** The change set is small (3 src files materially, 1 test file, 3
  doc surfaces) and self-contained; one commit is appropriate. The "Milestones line at merge" +
  "beta cut only on explicit go" hygiene matches project convention. ✅
- 🟢 TDD task ordering (reject → score_min_params → option delta → MAPQ test → e2e → docs) is sound;
  each RED is a genuine compile/assert failure (Task 2 RED is a true won't-compile on the signature
  change; Task 1/3 RED are genuine assert failures). ✅

---

## Action items

### Critical
*(none — no correctness-breaking flaw; the architecture is sound and source-verified)*

### Important
1. **Task 4 — make the MAPQ test non-vacuous.** Mandate the `best_over == diff` exact-equality leaf
   (AS_best=0 + second-best) as the `ln()`-ULP-sensitive assertion, plus a PE summed-`ln()`
   intermediate-bucket case; state *which buckets must be covered* (a literal expected-bucket table),
   not just "representative AS_best". As written, an integer-AS no-2nd sweep proves only buckets 22/44
   in the sub-unity-`diff` regime. (§2a)
2. **Gate non-vacuity — assert local *differs from* end-to-end, not just `S`>0.** Add a same-dataset
   `--hisat2` vs `--hisat2 --local` cross-check: end-to-end ~0 `S` CIGARs, local >0; the *delta* proves
   the `--no-softclip` drop is load-bearing. Bare `S`>0 in the local BAM can pass without proving the
   toggle. (§4)
3. **Name a concrete soft-clip-inducing dataset/approach up front** (adapter-tailed / appended-base /
   longer-read subset + fallback order) so the blocking `S`>0 assertion has a ready remedy instead of
   blocking the gate run itself. (§4)
4. **Fix the base note: beta.9, fresh branch off `origin/rust/iron-chancellor` (`478c974`), NOT from
   the current `rust/aligner-hisat2-multicore` worktree HEAD** (which carries an unmerged duplicate
   multicore commit + stale beta.8 bump). #986 is already merged upstream as `4b93f5b`. (§5)
5. **Task 3 verify — name the byte-frozen regression tests** that must stay green:
   `hisat2_se_option_string`, `hisat2_pe_option_string_has_no_dovetail`,
   `accepts_local_for_bowtie2_emits_local_and_g_score_min`,
   `local_custom_g_score_min_accepted_l_rejected`,
   `bowtie2_hisat2_strings_byte_frozen_alongside_minimap2`, and the
   updated-not-deleted `score_min_params_local_defaults_and_parses_g`. (§3)

### Optional
6. **Fix `2S62M` → `2S4M` in Task 5** (IMPL:84) — `2S62M` can't align against the 8 bp test fixture;
   the test-infra line already says `2S4M`. Keep the soft-clipped SEQ/AS/MD internally consistent. (§2b)
7. **Task 5 — also assert e2e MAPQ == 44 (local top leaf)**, not just the `S` CIGAR — proves the
   `score_min_local` path fired end-to-end (end-to-end gives 42). Cheap, ties Task 4 to Task 5. (§2b)
8. **Mention `tests/methylseq_conformance.rs`** as a verified-unaffected regression-guard (it tests
   Bowtie 2-local + HISAT2-multicore, NOT `--hisat2 --local`, so the PLAN's "no conformance flip"
   holds) — so a later reviewer doesn't have to rediscover it. (§3)

---

**Bottom line:** APPROVE-WITH-CHANGES. The plan correctly locates every seam and the MAPQ-is-in-
`score_min_params` reframing is verified true; PE local wiring is already in place. The two things that
would let a vacuous implementation slip through green are the **Task 4 MAPQ regime** (sub-unity `diff`
makes the naive integer-AS test prove only buckets 22/44 — force the `==diff` exact-equality leaf) and
the **gate's bare `S`>0 check** (assert local *differs* from end-to-end). Fix items 1–5 before
implementing; 6–8 are quick wins.
