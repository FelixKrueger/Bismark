# Plan Review A — #1095 validation gates (PLAN.md rev 0)

**Reviewer:** A (fresh context) · **Date:** 2026-08-07
**Plan:** `plans/08072026_1095-validation-gates/PLAN.md`
**Verdict:** Sound in intent and mostly well-verified, but **not implementable as written**. Two critical problems: (1) the plan's CI claims are wrong — the branch base's `rust_ci` is **already red** in the two feature jobs the new gates will also run in, and the plan's "no changes outside this one file" scope cannot deliver its own "gates green in CI" validation; (2) Gate 1's step 4 states a bit-level formulation (`FLAG & 0x10 == 0`) that is **provably false on 10 of the fixture's 20 records** — the plan contradicts itself within one paragraph. Both are cheap to fix at plan level.

Everything below was checked against the actual fixtures (`samtools view`), the parent artifacts, `rust_ci.yml`, and the live CI run history — not taken from the plan's prose.

---

## 1. Logic review

### 1.1 Gate 1, step 4 — the bit-level biconditional is wrong for PE (Critical)

The plan writes: *"Assert the biconditional per record: `XG == "CT"` ⇔ `FLAG & 0x10 == 0` for this fixture's FLAG set — concretely `FLAG ∈ {99, 147}` for `CT` …"*

These two clauses contradict each other. Verified census of `rust/bismark/tests/data/dedup/nondir_pe_1030.bam`:

```
(CT, 99)×4   99  = 0x63 → 0x10 CLEAR
(CT, 147)×4  147 = 0x93 → 0x10 SET      ← violates "CT ⇔ 0x10 clear"
(GA, 83)×6   83  = 0x53 → 0x10 SET
(GA, 163)×6  163 = 0xA3 → 0x10 CLEAR    ← violates it from the other side
```

If an implementer codes the `FLAG & 0x10 == 0` clause, the test fails on 10/20 records. The parent plan states the bit form **for SE only** (§3.8: "SE … `FLAG & 0x10 ⟺ XG == "GA"`"); for PE the invariant is `patter`'s `is_bottom` — equivalently *reverse bit == second-in-pair bit ⟺ XG == "CT"* — and §9.8's enumerated-set form `XG == "CT" ⟺ FLAG ∉ {16, 83, 163}` is the correct cross-mode statement (verified to hold on all 20 records, and on all 12,974 records of the synth fixture with zero violations).

The failure mode is not silent (the test fails loudly), but a self-contradictory spec paragraph is exactly what sends an implementer to "fix" the wrong side — weaken the assertion or blame the fixture. Delete the `& 0x10` clause; assert the §9.8 set form, with a comment giving the correct PE bit-level reading.

### 1.2 "The only committed fixture with all four XG/FLAG combinations" is false, and the census does not pin what makes this fixture special (Important)

Verified: `dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` **also** has all four (XG, FLAG) pairs — (CT,99)×3217, (CT,147)×3217, (GA,83)×3270, (GA,163)×3270. This is not an accident: *any* directional PE Bismark BAM has all four, because R2 of an OT pair is (CT,147) and R2 of an OB pair is (GA,163). The claim is inherited from parent §9.8/CODE_REVIEW_A verbatim; both are imprecise at the (XG,FLAG)-pair level.

What actually makes `nondir_pe_1030.bam` unique — verified in file order — is that **all ten of its pairs are #1030 flag-swapped CTOT/CTOB pairs**: the first record of every QNAME pair carries the 0x80 second-in-pair bit — 4 pairs with file-order FLAGs (147,99) (index 2, CTOT) and 6 with (163,83) (index 1, CTOB). It contains **no OT or OB pairs at all**. Per parent §3.8, the loaded half of the equivalence is exactly these indices ("they agree *because of* the index-1/2 R1↔R2 swap").

Consequence for step 5's census: a fixture regenerated as a **directional** OT/OB file would produce the same four (XG,FLAG) pairs and pass the census unchanged — silently losing the swap-dependent witness, which is the one §9.8 exists for. The plan's self-review claims the census makes regeneration "fail loudly rather than weaken the gate"; that is true for truncation/subsetting but false for this — the most on-topic — drift. Fix: assert the pair structure (adjacent QNAME pairs; every pair's (first, second) FLAGs ∈ {(147,99), (163,83)}), or at minimum the full multiset 4/4/6/6 **plus** "first-of-pair always has 0x80 set".

### 1.3 Gate 1 never runs the converter (observation)

As specified, Gate 1 is `sam_body(fixture)` only — a lint on a static committed file. It exercises zero #1095 code. Its end-to-end meaning comes solely from composition with Gate 2's round-trip over the *same* fixture (identity transfers the property to the output). That composition is sound and matches §9.8's own wording, but the plan never states the dependency. Also note the gate breaks on an aligner flag-table change **only when fixtures are regenerated** — worth one line in the test's doc comment so nobody over-trusts it.

### 1.4 "Pure extraction" is contradicted by the plan's own helper (Important, wording)

§3 Gate 2 step 1 adds a **new** assertion to the shared helper (input body has `expected_records` lines), then closes with "The helper must not change the SE test's assertions (pure extraction)", and §9 row 4 requires "`git diff` shows extraction only". Both statements are false as written: the SE test gains an assertion (8 records — true, consistent with the report test's `records read\t8`, so it passes). Harmless behaviorally, but the coverage audit (`/plan-manager`) will flag the mismatch, and an implementer honoring "pure extraction" literally would drop the census. Reword to "pure extraction **plus** the shared non-vacuity census (SE: 8)".

### 1.5 What checks out (verified independently)

- Census constants: `nondir_pe_1030.bam` = 20 records, (CT,99)×4, (CT,147)×4, (GA,83)×6, (GA,163)×6 — matches assumption 1 exactly. Synth fixture = 12,974 records, 42 with `I`/`D` in CIGAR, FLAGs {99,147,83,163} — matches assumption 2 and CODE_REVIEW_B row 38.
- The §9.8 equivalence holds on **both** PE fixtures (0 violations out of 12,994 records total) — Gate 1 and Gate 2 should indeed pass first time.
- 7 existing `#[test]`s in `rust/bismark/tests/aligner_five_base_bisulfite.rs`; +3 = the plan's 10.
- Helpers exist as described: `samtools_available()` panics under `$CI` (line 51–57), `run_converter` writes to the passed out-dir, output naming `<stem>.bisulfite.bam` (line 99).
- Parent citations accurate: §9.8 at PLAN.md:498–500; §10b "Not done" lists §9.8 as "Worth adding"; CODE_REVIEW_A:77 ("zero risk — I have run it") and HIGH-2; CODE_REVIEW_B rows 38–39, L10, and the noodles suggestion (line 144, action 2); COVERAGE.md row 71 = MISSING/admitted.
- `rust_ci.yml`: workflow-scope `RUSTFLAGS: -D warnings` (line 15); `test` job installs minimap2 + samtools (lines 35–45); `rammap-inprocess` feature exists (lines 92–95). **But see §4.1.**

---

## 2. Assumptions

| # | Plan assumption | Check |
|---|---|---|
| 1 | nondir fixture counts/pairs | **Confirmed** by direct census (above) |
| 2 | synth fixture 12,974 records, round-trips | **Confirmed** count + 42 indel records; round-trip re-proven by the gate itself, as the plan says |
| 3 | Output naming `<stem>.bisulfite.bam` | **Confirmed** (existing SE test) |
| 4 | PE accepted with no further flags | Consistent with both reviewers' runs; converter is per-record, no PE-specific flags exist |
| 5 | "§9.8 equivalence is a property of Bismark-produced BAMs; test pins it on this fixture only" | Sound — parent §3.8 derives it from `output.rs` for SE and all four PE indices |
| — | **Implicit, wrong:** "the new gates run in CI only via the `test` job, where samtools is present" | **False** — the same test binary runs in two more jobs without samtools; see §4.1 |
| — | **Implicit, wrong:** "this fixture is the only four-combination fixture, so the census pins its identity" | **False** — see §1.2 |
| — | **Implicit, false as stated:** synth fixture has "CB:Z:/barcode aux tags" (§2 context) | **0 `CB:Z:` tags** in the file; the barcode is a QNAME suffix (`…:TTAGTTGT`). `CB:Z:` is what dedup's `--add_barcode` writes into the *deduplicated* sibling. The fixture's real distinctive value — scale, RNEXT/PNEXT/TLEN mate fields, indels, both strands — is plenty; describe it truthfully. The proposed name `real_pe_with_mate_fields_and_aux_tags_…` also oversells ("real" for a file literally named `synth_barcode…`; NM/MD/XM/XR/XG are ordinary Bismark tags) |

---

## 3. Efficiency

No issues. Two extra debug-build converter runs over ≤12,974 records plus `samtools view` — seconds; `sam_body` holds ~4–5 MB strings. No fixture added. The refactor-to-helper is the right call (three call sites). Run commands in §5.3 are correct and match CI (clippy `--features rammap-inprocess` variant included).

---

## 4. Validation sufficiency

### 4.1 The base CI is already red, and the plan's CI story misses the jobs that matter (Critical)

Verified against live CI: run **31204223600** on `dev` @ `7be9dde` (the #1095 review-fix commit, an ancestor of this branch's base `26eb28a`) — `cargo test` / clippy / fmt / perl-oracle **pass**, but **`rammap-inprocess` and `binseq-input` both FAIL**. Log excerpt:

```
thread 'bisulfite_input_round_trips_to_identical_sam_text' panicked at
  bismark/tests/aligner_five_base_bisulfite.rs:52:9
thread 'xm_is_left_untouched' panicked at ...:52:9
thread 'appends_a_pg_with_a_distinct_id' panicked at ...:52:9
```

Cause: both feature jobs run `cargo test -p bismark --features …` — which includes `aligner_five_base_bisulfite.rs` — but their install steps add **only minimap2** (`rust_ci.yml:97–105`, `:135–143`); #1095 added samtools **only to the `test` job**. The `$CI` panic guard then fires, exactly as designed. (This is the dual-driver back-port trap: the guard was copied from the minimap2 gates, whose dependency *is* installed in the feature jobs; samtools was not.)

Consequences for this plan:

- §2 ("`rust_ci.yml` `test` job installs samtools … so the new gates run there for real") and §7 ("samtools present") are **incomplete to the point of being wrong**: the gates also run in two jobs where samtools is absent and the guard panics.
- §9 row 1's "gates pass first time" and the implicit "CI green" cannot be met: the PR will inherit two red jobs, and the three new gates raise the panicking-test count in those jobs from 3 to 6.
- §5.2 "No changes outside this one file" is therefore untenable. The fix is two lines — add `samtools` to both feature jobs' `apt-get install` lines — and it is squarely in this plan's remit (its entire subject is "#1095 gates run for real"). Alternatively, state it as an explicit prerequisite fix landed first; what the plan may not do is stay silent.
- Add a §9 row: "feature jobs green — `rammap-inprocess` and `binseq-input` pass on the PR run" (they never have since `7be9dde`).

### 4.2 Otherwise the validation section is good

The falsifiability rows (observe each gate failing once) are exactly right, and non-vacuity via `expected_records` + the four-pair census kills the empty-stream and truncation failure modes. Remaining gaps, in risk order:

1. Census does not pin the CTOT/CTOB pair structure (§1.2) — the highest-value drift goes undetected.
2. Nothing constrains FLAGs outside the biconditional's enumerated set: a drifted record (CT, 2048-supplementary) satisfies `FLAG ∉ {16,83,163}` and the census can still find its four pairs among the other 19 records. Asserting per-record `FLAG ∈ {99,147,83,163}` (or the full 4/4/6/6 multiset) closes this for free — the plan already verified the exact counts.
3. §9 row 4's "extraction only" criterion will fail its own check (§1.4) — fix the criterion, not the code.

---

## 5. Alternatives

- **Open-1 (samtools text vs noodles):** the plan's choice (samtools text, consistent with the file) is defensible, but note §4.1 shifts the trade-off: Reviewer B's noodles `RecordBuf` comparison would make the gates dependency-free and dissolve the feature-job problem for the *new* tests (not the three existing ones). The 2-line CI fix is still cheaper and fixes all six; either resolves it — the plan must pick one explicitly rather than assume CI is green.
- **Open-2 (report assertions for PE):** agree with the plan, with a stronger argument than it gives — byte-identity of the output *implies* flip rate 0, so a report assertion adds only report-wording coverage, which the SE test already pins.
- **Open-3 (defer the other three G9 gaps):** the decision matches the handoff scope and stands, but the stated rationale for the unmapped pass-through — "needs a fixture change that invalidates existing counts" — is only true for editing the existing fixture in place; a **new** small mixed mapped+unmapped fixture invalidates nothing. Don't let that sentence survive into the next planning round; the module NOTE (test file lines 289–295) records the honest state.
- **Gate 1 over the output instead of the input:** asserting the biconditional over the *converter output* of the fixture would make Gate 1 exercise #1095 code directly at identical cost; with Gate 2's identity it is mathematically the same assertion, so this is a taste call — at minimum document the Gate-1/Gate-2 pairing (§1.3).
- **Run Gate 1's biconditional over the synth fixture too:** ~3 extra lines with a shared per-line checker; extends the equivalence witness to directional-style PE at scale (verified holding, 0/12,974 violations). Purely additive.

---

## 6. Action items

### Critical

1. **Fix the CI story (§4.1).** Add `samtools` to the `rammap-inprocess` and `binseq-input` install steps in `.github/workflows/rust_ci.yml` (2 lines) — inside this plan, or as an explicitly-sequenced prerequisite. Update §2/§7 to name all three jobs that run these tests, drop §5.2's "no changes outside this one file" if the fix rides along, and add a §9 row requiring both feature jobs green. Without this the plan's own validation criteria are unmeetable and the new gates add 3 more panicking tests to already-red jobs.
2. **Fix Gate 1 step 4 (§1.1).** Delete the `XG == "CT" ⇔ FLAG & 0x10 == 0` clause — it is false for (CT,147) and (GA,163), i.e. 10 of 20 records. Assert the §9.8 set form `XG == "CT" ⟺ FLAG ∉ {16,83,163}`; if a bit-level comment is wanted, the correct PE statement is *reverse bit == second-in-pair bit ⟺ XG == "CT"* (parent §3.8's `is_bottom`).

### Important

3. **Pin the pair structure in Gate 1's census (§1.2).** Assert adjacent-QNAME pairing with per-pair (first,second) FLAGs ∈ {(147,99), (163,83)} (4 and 6 pairs respectively), or at least the full 4/4/6/6 multiset plus "first-of-pair carries 0x80". Correct the "only committed fixture with all four XG/FLAG combinations" claim (§1, §2, and the parent-inherited wording): the synth fixture also has all four; this fixture's uniqueness is that every pair is a #1030-swapped CTOT/CTOB pair.
4. **Fix the synth-fixture description and test name (§2 Assumptions table).** Remove "CB:Z:/barcode aux tags" (the file has none — barcode is in the QNAME); rename `real_pe_with_mate_fields_and_aux_tags_round_trips_identically` to something truthful, e.g. `pe_at_scale_with_mate_fields_and_indels_round_trips_identically`.
5. **Reword the "pure extraction" claims (§1.4).** §3's closing line and §9 row 4 must acknowledge the helper's added `expected_records` assertion (SE: 8), or the coverage audit will flag a false mismatch.

### Optional

6. Per-record `FLAG ∈ {99,147,83,163}` guard in Gate 1 (subsumed by item 3's multiset).
7. Document the Gate-1 ↔ Gate-2 composition in the test doc comment, and note that Gate 1 detects flag-table changes only via fixture regeneration (§1.3).
8. Extend the Gate-1 biconditional over the synth fixture (verified holding; ~3 lines).
9. Rephrase Open-3's unmapped-pass-through rationale ("new fixture invalidates nothing"; deferral itself stands).
