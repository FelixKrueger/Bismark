# Plan Review B — `plans/08072026_1095-validation-gates/PLAN.md` (rev 0)

**Reviewer:** B (fresh context) · **Date:** 2026-08-07
**Verdict: SOUND IN INTENT, NOT LANDABLE AS WRITTEN.** The two gates are the right ones and I
independently re-verified every fixture constant and both round trips — the tests themselves will
pass. But the plan's CI premise is false: **dev's last Rust CI run is already red because the
existing #1095 gates panic in two jobs that don't install samtools**, this plan adds three more
such gates, and §5.2 ("no changes outside this one file") forbids the only fix. One clause of
Gate 1's specification is also mathematically wrong on the very fixture it targets.

Everything below was checked against the repo, the fixtures (via samtools), the converter source,
the parent artifacts, and live GitHub Actions results — not taken from the plan's prose.

---

## 1. Logic review

### 1.1 CRITICAL — the plan's CI claim is wrong, and the branch cannot go green under the plan's own constraints

Plan §2: *"`rust_ci.yml` `test` job installs samtools (added by #1095), so the new gates run
there for real"* — true but fatally incomplete. §7: *"CI: the gates join the existing #1095 gate
set in the `test` job; samtools present."*

What the workflow actually does (`.github/workflows/rust_ci.yml`):

- `test` job (line 35): installs **minimap2 + samtools** — fine.
- `rammap-inprocess` job (line ~96) and `binseq-input` job (line ~134): install **minimap2
  only**, then run `cargo test -p bismark --features …` — which compiles and runs the *same*
  `tests/aligner_five_base_bisulfite.rs` integration suite. `samtools_available()` (test file
  line 42–58) **panics whenever `$CI` is set and samtools is absent**.

This is not hypothetical. The most recent Rust CI run on `dev` — run **31204223600**, for
`7be9dde` (the very commit that added the panic guard) — concluded **failure**:

| Job | Conclusion |
|---|---|
| cargo test | success |
| cargo clippy / fmt / perl-oracle | success |
| **cargo build/test (rammap-inprocess feature)** | **failure** |
| **cargo build/test (binseq-input feature)** | **failure** |

The failed-job logs show exactly the predicted panic, three times per job:
`panicked at bismark/tests/aligner_five_base_bisulfite.rs:52: samtools not found but $CI is set …
Refusing to no-op.` (failing tests: `bisulfite_input_round_trips_to_identical_sam_text`,
`xm_is_left_untouched`, `appends_a_pg_with_a_distinct_id`). The two subsequent `dev` commits are
docs-only and did not trigger the workflow (it is path-filtered to `rust/**` +
`rust_ci.yml`), so red is the current baseline — and the handoff's "dev is clean, in sync"
framing masked it. This plan's branch touches `rust/**`, so pushing it **will** trigger all jobs,
and the three new samtools-gated tests raise the per-job failure count from 3 to 6.

Consequences for the plan as written:

- §9 row 1 "Gates pass first time … 10/10 green" is only true locally and in the `test` job.
- §5.2 "No changes outside this one file" makes green CI unachievable.
- The fix is two one-line additions (`samtools` in each feature job's `apt-get install`), or
  adopting the noodles comparison (see §1.5 / Open-1). Either way the plan must own this —
  ideally as a separate first commit, since the red CI is a pre-existing #1095 defect this
  "close the validation gaps" plan is the natural home for. Note this is the dual-driver
  back-port trap in CI-job form: the G8 fix was applied to one job while two sibling jobs run
  the identical tests.

### 1.2 CRITICAL (one-line fix) — Gate 1 step 4's bit-test gloss is false on this fixture

§3 Gate 1 step 4 opens: *"Assert the biconditional per record: `XG == "CT"` ⇔
`FLAG & 0x10 == 0` for this fixture's FLAG set"*. That equivalence is **wrong for 10 of the 20
records**:

- FLAG **147** = 0x1|0x2|**0x10**|0x80 — reverse bit **set**, yet `XG:Z:CT` (4 records);
- FLAG **163** = 0x1|0x2|0x20|0x80 — reverse bit **clear**, yet `XG:Z:GA` (6 records).

R2 of an OT pair is reverse-complemented and R2 of an OB pair is forward — XG tracks the *pair's*
originating strand, not the record's orientation bit. (The record-level bit truth on this fixture
is `XG == "CT" ⇔ (FLAG & 0x10 != 0) == (FLAG & 0x80 != 0)`, but do not put bit arithmetic in the
test at all.) The step's own continuation — "concretely `FLAG ∈ {99,147}` for CT and
`FLAG ∈ {83,163}` for GA … assert via the §9.8 formulation (`FLAG ∉ {16,83,163}`)" — **is
correct** and matches my census, so the operative instruction survives. But a plan whose step 4
states a false equivalence as the property being pinned invites the implementer to transcribe it
into an assertion (immediate loud failure, then a risky "repair" that could weaken the gate —
e.g. asserting R1 records only). Delete the `FLAG & 0x10` clause; keep only the set formulation.
This is the same FLAG-bit-reasoning-on-PE error class #1030 documented; it should not appear in a
plan whose fixture exists *because* of #1030.

### 1.3 The census constants are right — verified independently

`samtools view` census of `rust/bismark/tests/data/dedup/nondir_pe_1030.bam` (my run,
2026-08-07): **(CT,99)×4, (CT,147)×4, (GA,83)×6, (GA,163)×6 — 20 records**, every record carries
`XG:Z:` ∈ {CT, GA}, and the biconditional holds on all 20. Matches Assumption 1 exactly.
`synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam`: **12,974 records** (FLAG census
99×3217 / 147×3217 / 83×3270 / 163×3270), **42** records with `I`/`D` in CIGAR. Matches
Assumption 2 and the plan's §2 description.

### 1.4 "The only committed fixture with all four `XG`/FLAG combinations" is false

Inherited verbatim from parent §9.8 and CODE_REVIEW_A/B — and disproven by the census above:
**`synth_barcode…_pe.bam` also contains all four (XG, FLAG) pairs** (any directional PE Bismark
BAM does, because R2's orientation is flipped relative to R1). What is genuinely unique about
`nondir_pe_1030.bam` is **pair-level**: CTOT/CTOB pairs whose R1/R2 FLAG bits are #1030-swapped —
a structure the proposed **record-level** scan never inspects. The gate is still the right gate
on the right fixture (smallest, and the one carrying the swap pairs), but the plan should correct
the claim rather than propagate it, and should not imply the test distinguishes four *strand
indices* — at record level it distinguishes four (XG, FLAG) pairs that a directional fixture also
has. (The test name `nondir_pe_all_four_strand_indices_round_trip_identically` is fine — the
*fixture* does contain all four strand indices; the *Gate 1 assertion* just doesn't see them.)

### 1.5 What Gate 1 actually tests — data, not code

Gate 1 as specified (§3 step 2) scans `sam_body(dedup/nondir_pe_1030.bam)` — the committed,
Perl-v0.25.1-produced fixture. **No #1095 (or any Rust) code executes.** Parent §9.8's stated
purpose — "a future change to the flag table must break a test rather than the science" — is not
achieved by scanning a static fixture: the Rust aligner's index→(FLAG, XG) table is never
consulted. What the gate *does* buy, and the plan should say so plainly:

1. It pins the cross-tool contract that makes the converter's XG-only design safe: the converter
   derives the (meth, unmeth) pair from `XG` alone (`XgStrand::from_tag`,
   `aligner/mod.rs:785`; FLAG is only read for pass-through routing at `mod.rs:740`), while
   downstream `patter` infers strand from **FLAG** (handoff G15: OT selector {99,147}, OB
   {83,163} — exactly the fixture's partition). If XG⟺FLAG broke, the converter would still
   "round-trip" happily and the science would be silently wrong downstream — which is precisely
   why the invariant deserves its own gate.
2. It fail-louds fixture regeneration/truncation (the census).

Combined with Gate 2 over the same fixture (output text == input text), the property transfers to
converter output — the plan's §1 "invisible to the idempotence gate" framing is correct (verified
against `five_base_bisulfite.rs:15-17`: `methylation_call` branches on `XR`, not `XG`; the
XG→base map is emergent). Fine — but one honest sentence in the plan about "this gate asserts a
property of committed Bismark output, it runs no product code" prevents a future reader from
believing the Rust aligner's flag table is under test. Binding *that* table would belong in the
groundtruth suite (aligner-produced BAMs) — a legitimate follow-up, not this plan.

### 1.6 `FLAG ∉ {16, 83, 163}`: correct, non-vacuous — but the `16` member is dead weight here

Directly answering the formulation question:

- **Correct** on this fixture: CT ↔ {99,147} (both ∉ set), GA ↔ {83,163} (both ∈ set). Holds on
  all 20 records — verified.
- **Non-vacuous**: both XG values occur and both directions of the biconditional are
  instantiated; the four-pair census plus `== 20` closes the G19 hole. Good design.
- **But `16` is never exercised**: this PE-only fixture has no SE record, so a test that
  (wrongly) used `{83,163}` would pass identically. The SE half of the §9.8 table is one
  parameterisation away: `softclip_indel_se.bam` has FLAGs **0×4 (XG:CT) and 16×4 (XG:GA)**
  (verified). Running the same scan over it (census 8, pairs (CT,0)/(GA,16)) instantiates the
  `16` member and covers the full spec set. Cheap and worth doing; otherwise the plan should
  admit the `16` is asserted-but-unwitnessed.

### 1.7 Gate 2 — refactor and new tests are sound; round trips re-verified

I ran the shipped converter over both PE fixtures myself (debug binary, no extra flags):
**both byte-identical** on decompressed bodies (`cmp` clean), 20/20 and 12,974/12,974 re-encoded,
flip rate `0.000000`, `masked 0`. So Assumptions 2–4 hold and the two new tests will pass first
time; output naming `<stem>.bisulfite.bam` confirmed in `aligner/mod.rs:708` and on disk.

Two wrinkles in the refactor description:

- **"Pure extraction" is overstated.** The helper adds a non-vacuity census
  (`expected_records`) that the current SE test does not have, so the SE test *gains* an
  assertion. That is a strengthening, not a risk (the SE fixture has exactly 8 records — report
  and `xm` test agree), but §3's "must not change the SE test's assertions (pure extraction)"
  and §9 row 4's "same assertions; git diff shows extraction only" contradict the plan's own
  §3 step 1. Reword to "extraction plus one added census; no assertion removed or weakened".
- The helper must derive the produced filename from `fixture.file_stem()` (the current test
  hardcodes it); the plan implies but never states this. One clarifying clause avoids a
  hardcoded-name helper that only works for one fixture.

Test arithmetic checks out: 7 existing + 3 new = 10, matching §5.3.

## 2. Assumptions

| # | Plan assumption | Check | Verdict |
|---|---|---|---|
| A1 | nondir census (20; 4/4/6/6; biconditional holds) | Recounted via samtools | ✅ exact |
| A2 | synth = 12,974 records, round-trips byte-identically | Recounted + re-ran converter + `cmp` | ✅ |
| A3 | Output name `<stem>.bisulfite.bam` | `mod.rs:708` + observed on disk | ✅ |
| A4 | PE BAM accepted with no further flags | Ran it | ✅ |
| A5 | §9.8 equivalence is a fixed Bismark property, pinned on this fixture only | Consistent with G15's patter selectors | ✅ reasonable |

**Unstated assumptions the plan missed:**

- **"CI is green at baseline" — FALSE** (§1.1). The load-bearing one.
- "The gates run in CI only via the `test` job" — false; two feature jobs run them too.
- "`nondir_pe_1030.bam` is the only fixture with all four XG/FLAG combinations" — false at the
  record level (§1.4).
- MD/NM present on every record of both PE fixtures (the converter hard-requires them,
  `mod.rs:777-783`) — true, but it is an input precondition the round-trip silently depends on;
  my successful runs confirm it.

## 3. Efficiency

No concerns. My timed runs: nondir < 0.1 s, synth ≈ 1 s (debug build) plus two `samtools view`
per test; the whole suite stays in single-digit seconds. No fixture added; repo size unchanged.
One micro-note: Gate 1 and the nondir round-trip test each invoke `samtools view` on the same
fixture — merging them would save one subprocess but blur two properties into one test; the
plan's separation is the better trade.

## 4. Validation sufficiency

**Good:** the falsifiability rows (§9 rows 2–3 — observe each gate fail once) and the census/
non-vacuity design are exactly the right G19 medicine; the refactor-purity row and hygiene rows
are appropriate.

**Gaps, in order of risk:**

1. **No "all CI jobs green" validation row** — the highest-risk failure mode for this change is
   not a wrong assertion but the branch turning (staying) red in `rammap-inprocess`/
   `binseq-input`. Add: push, then assert **all six** workflow jobs pass. As written the plan
   would be "validated" locally while deepening a red CI.
2. **Gate 1's falsifiability row under-specifies**: inverting `==`→`!=` proves the biconditional
   direction fires; also falsify the **census** (e.g. require a fifth pair or count 21) once, so
   both halves of the gate are observed failing. Row 3 does this for Gate 2 only.
3. **The `16` member of the spec set is unwitnessed** (§1.6) — either extend Gate 1 over the SE
   fixture or record the limitation.
4. Minor: no row confirms the two new PE tests' output files land in the tempdir with the
   derived names (implicitly covered by the helper's exists-assert — fine once the helper's
   stem-derivation is stated).

## 5. Alternatives

1. **noodles `RecordBuf` comparison (Open-1) deserves a real re-decision, not a default.** The
   plan dismisses it as "the CI panic guard already prevents silent skips" — but the panic guard
   is exactly what has two CI jobs red today, and **both** parent reviewers suggested in-process
   comparison (CODE_REVIEW_B §"noodles"; CODE_REVIEW_A: "shows the comparison can be done
   in-process with noodles and the external dependency dropped entirely" — the plan attributes
   it to B alone). A noodles gate needs no samtools, no skip logic, no workflow edit, and runs
   identically in all six jobs. Keeping samtools text for file-wide consistency is defensible —
   but then the two-line workflow fix is mandatory, and the Open-1 row should cite the red run
   as the cost of the chosen path.
2. **Parameterise Gate 1 over the SE fixture too** — closes the {0,16} half of the §9.8 table
   for the cost of one function call (§1.6).
3. **Assert Gate 1's scan over converter output as well as input** — nearly free inside the
   round-trip helper, and makes Gate 1 exercise product code; largely redundant given Gate 2's
   equality, so optional.
4. **Open-3's unmapped-pass-through rationale is overstated**: "needs a fixture change that
   invalidates existing counts" is only true if the *existing* fixture is edited. A separate
   1–2-record fixture (unmapped + full tags) invalidates nothing — CODE_REVIEW_B's H-list even
   proposed synthesising it. Deferral is still legitimate (the handoff scoped this plan to two
   gaps), but the recorded reason should be scope, not impossibility.

## 6. Action items

### Critical

1. **Resolve the red-CI conflict before or with this change** (§1.1). Either add `samtools` to
   the `rammap-inprocess` and `binseq-input` install steps in `.github/workflows/rust_ci.yml`
   (two one-line edits; drop §5.2's "no changes outside this one file"), or adopt the noodles
   comparison so the gates need no samtools anywhere. Add an "all CI jobs green" row to §9.
   Evidence: run 31204223600 (`dev` @ 7be9dde) — both feature jobs failed with the
   `aligner_five_base_bisulfite.rs:52` panic.
2. **Delete the false `FLAG & 0x10 == 0` gloss from §3 Gate 1 step 4** (§1.2). FLAG 147 (XG:CT)
   and FLAG 163 (XG:GA) violate it — 10/20 records. Keep only the set formulation
   `FLAG ∉ {16,83,163}` ⟺ `XG == "CT"`, which I verified holds on all 20.

### Important

3. Correct "the only committed fixture with all four XG/FLAG combinations" (§1.4) — synth has
   all four too; nondir's uniqueness is the pair-level #1030 FLAG swap, which the record-level
   gate does not inspect.
4. State in the plan (and ideally the test doc comment) that Gate 1 asserts a property of
   committed Bismark-Perl data and runs no product code; its value is pinning the
   converter↔patter FLAG/XG contract and fail-louding fixture drift — not gating the Rust
   aligner's flag table (§1.5).
5. Either extend Gate 1 over `softclip_indel_se.bam` (FLAGs 0/16, verified) so the `16` in the
   spec set is witnessed, or record that it is not (§1.6).
6. Fix the "pure extraction / same assertions" wording (§1.7): the helper adds the
   `expected_records` census to the SE test — say so, and make §9 row 4 check "no assertion
   removed or weakened" instead of "extraction only". Also state that the helper derives the
   output name from the fixture stem.

### Optional

7. Falsify Gate 1's census once (not just the biconditional) before trusting it (§4.2).
8. Re-open Open-1 as a genuine decision with the CI evidence on the table; correct the
   attribution (both reviewers proposed noodles).
9. Reword Open-3's unmapped-pass-through rationale from "invalidates existing counts" to a
   scope decision; note the separate-tiny-fixture route for whoever closes it later.

---

### What I verified independently (so the next reader doesn't have to)

- Fixture censuses (both PE fixtures + SE fixture) via `samtools view` — all constants in §8
  A1/A2 are exact.
- Both PE round trips: ran `./rust/target/debug/bismark --illumina_5base
  --five_base_bisulfite_bam <fixture> --output_dir <tmp>`; `cmp` of decompressed bodies clean;
  reports: 20/20 and 12974/12974 re-encoded, flip rate 0.000000, masked 0.
- Converter branches on XG only; FLAG used solely for pass-through routing
  (`rust/bismark/src/aligner/mod.rs:740, 785`; `five_base_bisulfite.rs:15-17, 32-42`).
- Output naming `<stem>.bisulfite.bam` (`mod.rs:708`).
- CI: workflow env `RUSTFLAGS: -D warnings`; samtools installed in `test` job only; feature jobs
  run the same integration tests; run 31204223600 red with the samtools panic in both feature
  jobs; workflow path-filtered so the two later docs commits never re-ran it.
- Parent artifacts: §9.8 wording, G9 list (SESSION_HANDOFF.md:109-115), CODE_REVIEW_A
  recommendations (lines 62, 77, 96), CODE_REVIEW_B fixture table rows 38-39 + L10 + noodles
  suggestion, COVERAGE.md rows 40 and 71.
