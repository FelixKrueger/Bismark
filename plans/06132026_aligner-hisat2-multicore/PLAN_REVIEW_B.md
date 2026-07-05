# PLAN_REVIEW_B — HISAT2 multi-core (Approach B) scoping plan

**Reviewer:** B (independent, fresh context) · **Date:** 2026-06-13
**Target:** `plans/06132026_aligner-hisat2-multicore/PLAN.md` (rev 1, scoping / spike-first)
**Verdict:** **APPROVE WITH CHANGES.** Approach B is sound, correctly scoped, and the spike-first stance
is the right call. All load-bearing source/Perl claims verified true against this worktree (`f1bcf42`).
But three things must be tightened before the spike runs: (1) the `-p 1` baseline is **not a literal Perl
`-p 1` run** (Perl *dies* on `-p 1`), so the spike's central comparison needs re-specifying; (2) the
"whole read set splice discovery" mechanism stated as fact is **imprecise** and should be demoted to a
spike question; (3) the validation matrix is under-specified for an aligner whose faithful single-core
gate covered SE+PE × {dir,non-dir,pbat} × {FastQ,FastA} + `--ambig_bam`/`--unmapped`/`--ambiguous`.

I did **not** re-litigate Approach A (locked by Felix). This review takes B as the target and critiques
whether it is correctly scoped, the spike correctly aimed, the gates well-defined, and the validation
sufficient.

---

## 1. Claim verification (all load-bearing claims checked against source)

| Plan claim | File:line | Verdict |
|---|---|---|
| Reject lives at `config.rs:254` (rev-0 `:251` = comment-block start) | `config.rs:254` | ✅ TRUE — `if aligner == Aligner::Hisat2 && cli.multicore.unwrap_or(1) > 1` returns `Unsupported`. Comment block opens at 244, fires at 254. The conformance test's doc-comment still cites `:251` (`methylseq_conformance.rs:208`) — stale but harmless. |
| `-p`/`--reorder` plumbing exists and is NOT Bowtie 2-gated | `options.rs:149-158` | ✅ TRUE — `if let Some(p) = cli.bowtie_threads { … opts.push("-p {p}"); opts.push("--reorder") }`. No `aligner ==` guard. Emitted for whichever aligner resolves. The *code comment* says "Bowtie 2 intra-instance threads" (misleading label) but the *behaviour* matches the plan. |
| Perl ships faithful `--hisat2 -p N --reorder` regardless of backend | `bismark:7998-7999` | ✅ TRUE — both pushed unconditionally inside `if ($parallel)`. |
| `'p=i' => $parallel` | `bismark:7348` | ✅ TRUE |
| `'parallel\|multicore=i' => $multicore` (so `--parallel` aliases `--multicore`, NOT `-p`) | `bismark:7361` | ✅ TRUE — confirms the naming-trap framing. |
| HISAT2-specific `-p` warning | `bismark:8004` | ✅ TRUE — "Each HISAT2 instance is going to be run with $parallel threads." |
| `--ambig_bam` multicore temp-name builder is Bowtie 2-only | `bismark:676-684` | ✅ plausible (cited in both the reject comment and the plan); under B there is one instance so it is moot. |
| README cpus-cap stop-gap | `rust/README.md:64-72` | ✅ TRUE, accurately summarised (incl. the "don't override `ext.args`" trap). |
| Conformance flip-detector exists | `methylseq_conformance.rs:211` `methylseq_align_hisat2_multicore_known_unsupported` | ✅ TRUE — asserts the error message `contains("not supported with --hisat2")`. |

**The plan's factual spine is solid.** The reuse-not-rebuild premise (B routes `--multicore N` into the
existing `-p`/`--reorder` machinery) is genuinely backed by the code.

---

## 2. Logic review

### 2.1 CRITICAL — the `-p 1` baseline does not exist in Perl; the spike's pivot comparison is mis-specified

The spike's go/no-go probe (Phase 0 step 1) is "HISAT2 `-p 1` vs `-p N`". But:

- **Perl literally cannot run `-p 1`.** `bismark:7994`: `die "Please select a value for -p of 2 or more!\n" unless ($parallel > 1)`. The Rust mirror enforces the same floor (`options.rs:151`, `if p < 2 → Validation error`).
- HISAT2/Bismark's own help text (`bismark:9953`) says `--reorder` "Has no effect if -p is set to 1". So `--reorder` is *only ever present* alongside `-p N≥2`.
- The existing byte-identical HISAT2 single-core gates ran with **no `-p` flag at all** — confirmed by the prior determinism spike (`plans/06052026_bismark-aligner-v1x/phase1-hisat2-determinism-spike/spikes/SPIKE_hisat2_determinism.md` §2 Q2: *"No `--seed`/`-p`/`--reorder` in the assembled options"*).

So the spike's "`-p 1`" must be defined precisely as **the no-`-p`-flag default single-core invocation
(= the shipped B-strong oracle)**, NOT a literal `-p 1` run. As written, a spike author could try to run
Perl `--hisat2 -p 1`, hit the `die`, and either (a) waste a cycle or (b) silently substitute `-p 2` as the
"baseline", which would make B-strong unfalsifiable.

There is a second-order subtlety the plan must call out: the **no-`-p` default emits no `--reorder`**, while
`-p N` emits `--reorder`. So B-strong is actually "`-p N --reorder` content == no-`-p`/no-`--reorder`
content". `--reorder` only governs output *order*, but the spike must confirm it does not change record
*content* (it shouldn't — but that is exactly the kind of thing a byte-identity port can't assume). **Action:
restate the pivot as `-p N --reorder` (N≥2) vs the bare single-core default, and name the latter explicitly.**

### 2.2 IMPORTANT — the "whole read set splice discovery" mechanism is stated as fact but is imprecise

The plan (lines 72-75) and the existing reject comment (`config.rs:245-249`) assert: *"a single `-p N`
instance sees the **whole read set**, so splice discovery is identical to single-core — the chunking that
breaks faithfulness never happens."* The Self-Review (line 188) calls this the load-bearing premise of B.

Checking the Perl HISAT2 plumbing (`bismark:8290-8307`): Bismark does **not** auto-build or feed a
cross-read splice-site table between reads or between instances. `--known-splicesite-infile` is **opt-in
and user-supplied** (`bismark:7373, 8302`); without it HISAT2 performs **per-read** dynamic spliced-alignment
search against the genome's static splice index. There is **no global "learn splices from the whole read
set then re-align" pass** in the Bismark HISAT2 path.

This means the mechanism that makes chunked multicore produce 1310-vs-1219 spliced reads is **not** a global
splice table that single-instance preserves. The more likely real cause is per-read spliced-search being
sensitive to thread scheduling / per-read RNG / chunk boundaries — which is *exactly* the kind of
`-p N`-threading non-determinism the spike is supposed to rule out. If the true cause is thread-scheduling
sensitivity rather than chunk-boundary read-set partitioning, then **single-instance `-p N` could ALSO
perturb the spliced set** (it is still multi-threaded), and B-strong would be **unreachable for the same
underlying reason** the fork model failed.

The plan partially hedges this ("The only question is whether HISAT2's threading itself perturbs the
alignments — that is exactly what the spike resolves", line 74-75) — which is correct. But the surrounding
prose presents "single instance ⟹ identical splice discovery" as established, which **over-sells B-strong's
reachability**. **Action: demote the splice-discovery claim from fact to a spike question, and explicitly
tie it to §2 below — make the spike inspect the N-CIGAR (spliced) record set specifically across `-p N` vs
single-core, not just total record content.** The prior spike found 12/8360 spliced records at 10k SE
(`SPIKE_hisat2_determinism.md` §2 Q4) — small enough that a 1-record splice drift could hide inside a "looks
mostly identical" eyeball and only surface as a byte-diff. The spike harness must diff *all* records (it
plans to — "modulo nothing") and additionally **call out the spliced-subset count** so a near-miss is legible.

### 2.3 IMPORTANT — HISAT2 `-p N` per-read RNG / co-optimal tie-breaks (the combined-index parallel)

The combined-index epic established that read-NAME-seeded RNG perturbs co-optimal alignment selection
(`--combined_index_single_pass`: 98/1M benign divergence from a qname tag perturbing Bowtie 2's read-name
RNG). HISAT2, like Bowtie 2, uses a per-read pseudo-random generator for tie-breaking among co-optimal
alignments. **Multi-threading does not change the per-read seed** (it is read-derived, not thread-derived) —
so in principle `-p N` should pick the same co-optimal alignment per read as `-p 1`. **This is the strongest
a-priori argument FOR B-strong and the plan should state it** (it currently does not invoke the per-read-seed
property at all, even though it is the most relevant precedent in the codebase). If the seed is genuinely
per-read and thread-independent, B-strong is *likely* reachable and the splice concern in §2.2 reduces to "do
spliced searches consult any thread-shared mutable state?". **Action: add the per-read-RNG-seed reasoning as
the explicit hypothesis the spike tests, citing the combined-index finding — it sharpens the prediction
(B-strong should hold) and tells the spike author exactly what a failure would mean (thread-shared state).**

### 2.4 — `--reorder` perf penalty is real and unmentioned (informational)

`bismark:9951-9953` documents that `--reorder` makes HISAT2 run *"somewhat slower and use somewhat more
memory than if --reorder were not specified"*. The spike's speedup measurement (step 3) will therefore
measure `-p N --reorder` (the shipped config), which is correct — but the plan should note the speedup is
the *reorder-penalised* speedup, not the raw `-p N` speedup, so the perf number isn't later mis-quoted.

### 2.5 — dispatch-seam logic is correct but the plan under-describes the actual change

The plan says (Phase 1) "Replace the `config.rs:254` reject with a route … set the single-instance path and
inject `-p multicore`". Tracing the real seams (see §4): the route is **two** edits in **two** files, not one
— (a) `lib.rs:144/180` must stop sending HISAT2-multicore into `parallel::run_*_multicore`, and (b) the
`-p N` value must reach `aligner_options`, which is built **once** at `config.rs:326-327` from
`cli.bowtie_threads` (an `Option<u32>`), **not** from `cli.multicore`. The plan's "route, not rebuild" is
*directionally* true but the one-line framing ("Replace the reject with a route") understates that
`build_aligner_options` currently has no visibility into `cli.multicore`. This is fine for a scoping plan but
the implementation plan must own it (see §4 action items).

---

## 3. Assumptions

- **Assumption 1 (the pivot, to confirm in spike):** correctly flagged as the gate-decider. **Strengthen
  per §2.1** (define the `-p 1` baseline as the no-`-p` default) and **§2.3** (add the per-read-seed
  hypothesis).
- **Assumption 2 (Bowtie 2 untouched):** sound — B is a HISAT2-only branch and the dispatch already
  separates them (`lib.rs:144/180`). Verified the Bowtie 2 multicore fan-out (`parallel.rs`) is independent.
- **Assumption 3 (`--ambig_bam` under single instance):** reasonable. Under B there is one instance, so the
  Bowtie-2-only multicore temp-name builder (`bismark:676-684`) is genuinely irrelevant. **But the plan
  should note that the *current* single-instance `--ambig_bam` HISAT2 path has only been gated at `-p 1`
  (no-`-p`)** — adding `-p N` could in principle reorder or change the ambiguous-BAM contents the same way
  it might the primary BAM. Fold `--ambig_bam` into the spike's content diff, not just a "it just works"
  confirmation (the plan does say "confirm in the spike, don't assume" at line 122 — good; make the diff
  explicit).
- **Assumption 4 (`-p`/`--reorder` plumbed, reuse not rebuild):** **TRUE** (`options.rs:149-158`, verified
  not Bowtie 2-gated). This is the assumption I most wanted to falsify and it holds.
- **MISSING assumption — HISAT2 honours `--reorder` identically to Bowtie 2.** The Rust port has only ever
  exercised `--reorder` under Bowtie 2 (`-p` was a Bowtie-2-era flag; all HISAT2 gates were no-`-p`). The
  plan assumes HISAT2 2.2.2's `--reorder` produces input-order output as Bowtie 2's does. Documented in
  Bismark's own help (`bismark:9949`, which names "the Bowtie 2 **or** HISAT2 output"), so low-risk — but it
  is an unstated assumption that the spike's content-diff will incidentally cover. State it.

---

## 4. Efficiency / "is B actually cheap?" pressure-test

The plan's "B is cheap because the plumbing exists" is **mostly true but not a one-liner.** Concrete seams an
implementation plan must touch:

1. **`config.rs:254`** — remove/replace the HISAT2-multicore reject. (1 edit)
2. **`build_aligner_options` (`options.rs:149`) OR `resolve` (`config.rs:326`)** — inject `-p N` for the
   HISAT2-multicore case. `-p` today is driven by `cli.bowtie_threads`; the multicore value lives in
   `cli.multicore`. The function signature has no `multicore` param. Either thread `multicore` in, or
   post-patch `aligner_options` in `resolve`. **Watch the `p < 2` floor** (`options.rs:151`): since the route
   only fires for `multicore > 1`, N≥2 is guaranteed — but if you reuse the `bowtie_threads` path verbatim,
   make sure a HISAT2 `--multicore 1` (the no-op default) does NOT inject `-p 1` (which would hit the floor's
   error). The reject's `> 1` guard already covers this; just don't regress it.
3. **`lib.rs:144` and `lib.rs:180`** — the `else if n > 1` arms route to `parallel::run_*_multicore`. For
   `aligner == Hisat2`, this must instead fall through to `run_se`/`run_pe` (the single-instance path) with
   `multicore` reduced to 1 *for dispatch purposes* while the `-p N` lives in `aligner_options`. This is the
   real structural edit and the plan's prose hides it. **Risk:** if `config.multicore` is left > 1, the
   single-instance path may still try chunk arithmetic elsewhere — grep for every `config.multicore` /
   `read_processing.skip`/`upto` consumer (the parallel path clears skip/upto via a `RunConfig` clone,
   `parallel.rs:23-24`; the single path must not double-apply).
4. **Report** (`config.rs:880-891` prints `aligner_options`) — `-p N --reorder` will now appear in the
   `_SE_report.txt`/`_PE_report.txt` "specified options" line. **This is a byte-identity surface:** if the
   B-faithful gate compares against Perl `--hisat2 -p N`, Perl's report will also carry `-p N --reorder`, so
   it matches. But if the B-strong gate compares against single-core `--hisat2`, the single-core report has
   **no `-p N --reorder`** in its options line → the reports will NOT be byte-identical even if the BAM is.
   **The plan's gate definitions (lines 140-144) only mention "decompressed BAM, @PG-filtered" — they are
   silent on the report.** Under B-strong the report line legitimately differs (it documents the actual
   invocation). **Action: the gate must state explicitly that the report's options line is expected to differ
   under B-strong (and is therefore excluded from / normalised in the byte gate), or the conformance flip
   will fail on the report even when the BAM is perfect.** This is the single most likely "passes the BAM
   diff, fails the gate" trap.
5. **stderr notice** — the never-silent semantic-remap message. New code, but trivial.
6. **Conformance flip** (`methylseq_conformance.rs:211`) — the test asserts the *error message*. When B
   ships, this test must change from "expect_err contains 'not supported with --hisat2'" to a success/accept
   assertion. **The plan says "flips → move to an accept row" (line 145) but does not specify the new
   assertion.** Since the test is fixture-free (no `bowtie2 --version` subprocess — the reject fired before
   I/O), the *new* accept path will now reach `resolve()`'s aligner-detection subprocess
   (`config.rs:325 detect_aligner`), which needs a real HISAT2 on PATH or a fixture. **This is a non-obvious
   seam: the flipped test may no longer be runnable as a pure unit test** — it might have to move to an
   oxy/integration gate or stub the detector. Flag for the implementation plan.

**Net:** B is *moderate*, not trivial: ~2 substantive edits (lib.rs dispatch fall-through + options `-p`
injection) plus the report/gate-definition subtlety (item 4) and the conformance-flip detector-becomes-live
subtlety (item 6). "Route, not rebuild" is fair, but the plan should not let "cheap" imply "one line".

---

## 5. Validation sufficiency

The plan's validation section (lines 136-151) is the weakest part for a byte-identity port.

### 5.1 IMPORTANT — the implementation gate matrix is under-specified

The faithful single-core HISAT2 port (v1.x Phase 2a/2b) was gated across **SE + PE**, and the Bowtie 2
multicore (Phase 9b) gate covered **SE/PE × {dir,non-dir,pbat}** plus `--unmapped`/`--ambiguous`/`--ambig_bam`
merge correctness. This plan's gate (lines 140-144) says only "for several N" and "decompressed BAM,
@PG-filtered" — it does **not** enumerate the layout/library/format/side-channel matrix. For a worker-count
change the relevant axes are:

- **SE + PE** — both (PE has the extra mate-pairing + `--reorder`-lockstep concern; the prior spike only
  covered SE, deferring PE determinism to "aligner-level not layout-dependent" — that hand-wave was fine for
  *run-to-run* determinism but `-p N`-vs-`-p 1` is a new axis and PE should be gated explicitly).
- **dir / non-dir / pbat** — non-dir runs 4 HISAT2 instances; under B each becomes a single `-p N` instance.
  At minimum gate dir + non-dir (pbat shares the non-dir machinery).
- **FastQ + FastA** — Phase 9a added FastA; a worker change shouldn't interact, so FastA can be a justified
  *subset* (one cell) rather than the full cross.
- **`--ambig_bam` + `--unmapped` + `--ambiguous`** — these are the side-channels most likely to silently
  drift under threading. At least one cell each.
- **several N** (for B-strong: prove N-invariance — e.g. N ∈ {2, 4, 8}; the methylseq-derived N is `cpus/3`
  so realistic values are small).

**Action:** the plan should state the multicore gate matrix or an explicit *justified subset* (with the
justification — e.g. "FastA orthogonal to threading, one cell"). "Several N" alone is insufficient. The good
news: under **B-strong** the oracle is the *already-shipped single-core Rust output*, so the matrix is cheap
(no Perl re-run needed per cell — just `--multicore N` vs `--multicore 1` Rust-vs-Rust). Under B-faithful
each cell needs a matching Perl `--hisat2 -p N` run.

### 5.2 IMPORTANT — N-stability / run-to-run determinism at fixed N is asserted, not gated

B-faithful's whole premise is that `-p N` is **deterministic run-to-run at a fixed N** (Assumption 1). The
spike (step 1) checks this once. But a single passing run does not establish determinism — the prior spike
explicitly noted it passed "single iteration … first try" (`SPIKE_hisat2_determinism.md` §2). For a
*threading* change specifically, **run-to-run flakiness is the canonical failure mode** (thread scheduling
varies between runs). **The spike should run `-p N` at least twice (same N) and confirm byte-identity
run-to-run BEFORE comparing to `-p 1`** — otherwise a "deterministic" verdict from one run could be luck. The
plan's step 1 says "non-deterministic run-to-run even at fixed N → escalate" but does not say the spike
*runs it twice* to detect that. **Action: make "≥2 runs at fixed N, assert byte-identical" an explicit spike
step.**

### 5.3 — the spike correctly captures all three Perl oracles (good)

Phase 0 step 2 (capture Perl `-p 1`, `-p N`, `--multicore N` in one run) is well-designed — it disambiguates
B-strong vs B-faithful AND re-confirms the rev-0 worker-variance in a single pass. **Caveat from §2.1:** the
"Perl `-p 1`" oracle is the no-`-p` default (Perl dies on literal `-p 1`); label it correctly in the harness.

### 5.4 — regression coverage is named but not gated

Line 148-150 lists the regressions to preserve (Bowtie 2 multicore, single-core HISAT2, existing `-p N`) but
these are assertions, not gate steps. Since B is a HISAT2-multicore-only branch, the risk to Bowtie 2 is
low — but the existing test suite (240 tests per the aligner memory) must stay green, and the
`methylseq_align_hisat2_multicore_known_unsupported` flip is the one *intended* break. State "full `cargo
test` green except the deliberately-flipped conformance test."

---

## 6. Alternatives / trade-offs (within B — not re-litigating A)

- **B-strong vs B-faithful is the right pivot** and the spike picks it correctly. One refinement: the plan
  treats them as mutually exclusive outcomes, but the spike could find **B-strong holds for SE/dir but not
  for spliced-heavy or non-dir cells**. The gate should be picked *per the spike's evidence across the
  matrix*, not from a single SE/dir run. (Ties to §2.2 — spliced records are where B-strong is most at risk.)
- **Escalation path on non-determinism is clean** (defer to the documented cpus-cap stop-gap, which already
  unblocks methylseq). Good — B is correctly framed as a *quality* improvement, not an announcement blocker
  (Q4). No objection.
- **The semantic remap (`--multicore N` → `-p N` for HISAT2) is a deliberate, documented divergence** from
  Perl's flag meaning. The never-silent stderr/README/report announcement is the right mitigation. One thing
  to watch: this means Rust `--hisat2 --multicore N` is **not byte-identical to Perl `--hisat2 --multicore N`**
  (different model) — only to Perl `--hisat2 -p N` (B-faithful) or Rust single-core (B-strong). The plan is
  honest about this (lines 88-95) but the conformance/gate harness must compare against the *right* Perl
  invocation (`-p N`, never `--multicore N`), or it will "fail" against the wrong oracle. Make the oracle
  command explicit in the gate.

---

## 7. Action items

### Critical (resolve before the spike runs)
1. **Re-specify the spike's pivot comparison (§2.1):** the `-p 1` baseline is the **no-`-p`-flag single-core
   default** (Perl *dies* on literal `-p 1`, `bismark:7994`; the shipped HISAT2 gates used no `-p`). Define
   B-strong as "`-p N --reorder`, N≥2, content == bare single-core default content". Without this the spike
   can be run against a non-existent baseline.
2. **State the report-line gate exclusion (§4 item 4):** under B-strong the `_report.txt` "specified options"
   line legitimately gains `-p N --reorder` that the single-core oracle lacks → the report is NOT
   byte-identical even with a perfect BAM. The gate/conformance must normalise or exclude the options line,
   or it fails on the report. This is the most likely silent gate-trap.

### Important
3. **Demote the "whole read set splice discovery" claim to a spike question (§2.2)** and have the spike
   explicitly diff the spliced (N-CIGAR) record subset across `-p N` vs single-core, not just total content.
   The stated mechanism is imprecise (Bismark feeds no cross-read splice table; discovery is per-read), so
   single-instance does not *automatically* guarantee identical splices — the spike must prove it.
4. **Add the per-read-RNG-seed hypothesis (§2.3)** citing the combined-index finding: HISAT2's tie-break seed
   is read-derived (thread-independent), which is the strongest a-priori argument FOR B-strong — and tells
   the spike author that a B-strong failure implies thread-shared mutable state. State it as the tested
   hypothesis.
5. **Make run-to-run determinism a measured spike step (§5.2):** run `-p N` ≥2× at the same N, assert
   byte-identical, BEFORE the `-p N`-vs-`-p 1` comparison. One passing run does not establish thread
   determinism.
6. **Specify the implementation gate matrix or a justified subset (§5.1):** SE+PE × {dir, non-dir} at minimum,
   plus `--ambig_bam`/`--unmapped`/`--ambiguous` cells and several N for the B-strong N-invariance proof;
   FastA may be a one-cell subset with stated justification. "Several N / decompressed BAM" alone is
   insufficient for an aligner whose single-core gate covered the full matrix.
7. **Name the dispatch + option-injection seams in Phase 1 (§4 items 2,3):** `lib.rs:144/180` must fall
   through to `run_se`/`run_pe` for HISAT2 (not `parallel::run_*_multicore`), AND `-p N` must reach
   `aligner_options` (built once at `config.rs:326` from `cli.bowtie_threads`, not `cli.multicore`); ensure no
   double-application of skip/upto/chunk arithmetic on the single path. "Route, not rebuild" is fair but is
   ~2 substantive edits + the report subtlety, not one line.

### Optional / polish
8. **Conformance-flip detail (§4 item 6):** specify the new assertion AND note the flipped test will now
   reach `detect_aligner` (`config.rs:325`) — it may need a HISAT2 fixture/stub or to move to the oxy gate,
   since it can no longer fail-fast before the version subprocess.
9. **State the `--reorder` perf caveat (§2.4):** the measured speedup is the reorder-penalised number
   (`bismark:9951-9953`), so it isn't later mis-quoted as raw `-p N`.
10. **Add the "HISAT2 honours `--reorder` like Bowtie 2" assumption (§3 missing assumption)** — low-risk
    (Bismark help names HISAT2 explicitly) but currently unstated; the spike content-diff covers it.
11. **Fix the stale `config.rs:251` cite in `methylseq_conformance.rs:208`** when the test flips (the reject
    is at `:254`).

---

## 8. Bottom line

Approach B is the right design and the spike is aimed at the genuinely open question (the prior Phase-1
determinism spike only tested no-`-p` single-instance — `-p N` threading is **not** previously answered, so
this spike is **not** redundant). The plan's facts all check out against source. The gaps are: a
mis-specified `-p 1` baseline that doesn't exist in Perl (Critical #1), a silent report-line byte-gate trap
(Critical #2), an over-stated splice-discovery mechanism that should be a spike question (Important #3), and
an under-specified validation matrix (Important #6). Fix the two Criticals before the spike runs; fold the
Importants into the spike design and the implementation plan. With those, B is well-scoped and the
B-strong/B-faithful gate framework is correct.
