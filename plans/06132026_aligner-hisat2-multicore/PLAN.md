# PLAN (scoping / spike-first) — HISAT2 multi-core support in the Bismark aligner (v1.x)

**Date:** 2026-06-13
**Crate:** `rust/bismark-aligner` · **Base:** `rust/iron-chancellor` (`f1bcf42`)
**Status:** PLAN rev 2 (scoping) — manual review ✅ + **Approach B locked (Felix, 2026-06-13)** +
**dual plan-review ✅** (`PLAN_REVIEW_A.md`/`PLAN_REVIEW_B.md`, both APPROVE-WITH-CHANGES, all findings
folded). Awaiting **Phase-0 spike (decides B-strong vs B-faithful gate)** → implementation plan → implement trigger.
**Origin:** the methylseq CLI-surface conformance pass tracked this as GAP-2 (`KnownUnsupported`).

**rev-2 delta (dual plan-review, both reviewers source-verified every load-bearing claim):**
- 🔴 **The `-p 1` baseline does NOT exist** — Perl dies on `-p 1` (`bismark:7994`); Rust mirrors it
  (`options.rs:151`). "Single-core" = the **bare no-`-p`** invocation. The spike's comparison is
  `-p N --reorder` (N≥2) vs **bare-no-`-p` single-core**, NOT `-p N` vs `-p 1`. (Both reviewers / B-C1.)
- 🔴 **Per-artifact gate.** The `_report.txt` prints `aligner_options` verbatim (`report.rs:67-72`),
  so a `-p N` report can never be byte-identical to a single-core report. **BAM** may be B-strong;
  the **report is intrinsically B-faithful-shaped** (matches Perl `--hisat2 -p N`). (A-C1 ≡ B-#2.)
- 🔴 **Interception is `lib.rs:144/180`, not the `config.rs:254` reject.** Deleting the reject alone
  routes HISAT2 into `parallel::run_*_multicore` (the fork+chunk breakage). B must force the
  single-instance direct path (`config.multicore = 1` + carry N into `-p`). (A-C2 ≡ B-#7; impl-plan.)
- **Splice discovery is per-read, NO cross-read table** — "whole read set ⇒ identical splices" is a
  spike *question*, not a fact; the spike diffs the spliced (N-CIGAR) subset. (A-I2 ≡ B-#3.)
- Spike **varies N ∈ {2,4,8}** and **repeats each ≥2×**; the **per-read-RNG-seed hypothesis** (B-#4)
  frames what a failure means (thread-shared state). `--ambig_bam` cite corrected: the Bowtie-2-only
  piece is `@temp_ambig_bam` (SE 656/661, PE 715/720), not `676-684` (the HISAT2 name builder). (A-I3.)

---

## Decisions locked (Felix, 2026-06-13)

- **Q1 → Approach B.** Route `--hisat2 --multicore N` to a **single** HISAT2 instance with
  `-p N --reorder` (whole read set, threaded), **not** the fork+chunk-split path. Deterministic
  and node-independent. The fork+chunk path stays **Bowtie 2-only**.
- **The spike still runs** (it picks the *gate*, B-strong vs B-faithful — see below — and sizes
  the work), but A-vs-B is no longer open: B is the target.

---

## Goal

Let `--hisat2` run with `--multicore N > 1` in the Rust `bismark` aligner. Today it is
**hard-rejected** at `bismark-aligner/src/config.rs:254` (`AlignerError::Unsupported`; the cited
"`:251`" in rev 0 was the comment-block start). Because nf-core/methylseq **auto-derives
`--multicore = cpus/3`** on large (`process_high`) nodes, `--aligner bismark_hisat` currently
**fails on any node with ≥ 6 CPUs** under the Rust suite (documented stop-gap: cap the align
step below 6 CPUs — `rust/README.md:64-72`).

**Intended outcome:** `--hisat2 --multicore N` runs and produces **deterministic, node-independent**
output — gated byte-identical to either single-core `--hisat2` (B-strong) or Perl `--hisat2 -p N`
(B-faithful), per the spike's determinism verdict.

---

## Bismark's two parallelism knobs — the crux of the design

Bismark has **two independent** ways to use more cores, and they are NOT the same (a naming trap):

| Knob | Perl var / option | Meaning | Worker-invariant? |
|------|-------------------|---------|-------------------|
| **`-p N`** | `$parallel` (`'p=i'`, `bismark:7348`) | **Threads inside ONE aligner instance**, over the whole read set. Emitted as `-p N --reorder` (`bismark:7998-7999`). | n/a (one instance, whole set) |
| **`--multicore N` / `--parallel N`** | `$multicore` (`'parallel\|multicore=i'`, `bismark:7361`) | **N forked Bismark instances**, each aligning a 1/N chunk (fork+modulo). | Bowtie 2: yes (Phase 9b). **HISAT2: NO** (splice discovery is per-chunk → 1310 vs 1219 spliced). |

⚠️ **The trap:** `--parallel` is an alias for `--multicore`, **not** for `-p`. They mean opposite
things despite sharing the word "parallel".

**Key facts that make Approach B cheap and well-oracled (verified against `f1bcf42`):**
1. **Perl already ships a faithful `--hisat2 -p N` threaded mode.** `bismark:7998-7999` pushes
   **both `-p $parallel` and `--reorder`** to the aligner regardless of backend; the inline
   comment: *"re-orders the Bowtie 2 **or HISAT2** output so that it does match the input files.
   This is absolutely required for parallelization to work."* (`bismark:8004` even has the
   HISAT2-specific "Each HISAT2 instance is going to be run with $parallel threads" warning.)
2. **The Rust `-p`/`--reorder` plumbing already exists and is NOT Bowtie 2-gated.** `options.rs:149-157`
   (`cli.bowtie_threads` → `-p {p}` + `--reorder`) is emitted for whichever aligner resolves. So
   `bismark_rs --hisat2 -p N` **already runs today** — it has simply never been byte-*gated* (all
   shipped HISAT2 gates were `-p 1`).

So Approach B is **not a from-scratch architectural branch** — it is *reusing* the existing
single-instance `-p`/`--reorder` machinery and **routing `--multicore N` into it** (one instance,
`-p N`) for HISAT2, instead of forking.

---

## Why it's currently rejected — and what B changes

The reject (`config.rs:254`) is correct *for the fork+chunk model*: HISAT2 discovers splice sites
**across the whole input read set**, so chunk-splitting changes the discovered splices and the
spliced alignments. Perl itself is **not worker-invariant** here — single-core 1310 spliced vs
`--multicore 8` 1219 on the 1M oxy subset (`config.rs:243-253`).

**Approach B sidesteps this entirely:** a single `-p N` instance sees the **whole read set**, so
splice discovery is identical to single-core — the chunking that breaks faithfulness never happens.
The only question is whether HISAT2's *threading itself* (`-p N`) perturbs the alignments. That is
exactly what the spike resolves.

### The two candidate gates (the spike picks which)

- **B-strong** — byte-identical to **single-core `--hisat2`** (`-p 1`). Requires `-p N` content
  == `-p 1` (threading changes nothing but speed). **Node- AND N-independent** — the ideal. The
  conformance suite can then assert any-N == single-core.
- **B-faithful** — byte-identical to **Perl `--hisat2 -p N`** for matching N. Requires only Rust↔Perl
  `-p N` parity (deterministic for a fixed N). Survives even if `-p N` ≠ `-p 1` (e.g. a thread-count
  -dependent tie-break), but the output is then N-dependent, so methylseq's auto-derived N would make
  results node-size-dependent (acceptable but weaker; document loudly).

### Semantic remap — never-silent

Perl `--hisat2 --multicore 8` means *8 forked instances*. Approach B makes the Rust tool treat
`--hisat2 --multicore 8` as *1 instance with `-p 8`* — i.e. Perl's **`-p`** semantics, **not**
Perl's **`--multicore`** semantics. This is a deliberate divergence from Perl's flag meaning,
chosen for determinism. It must be **documented loudly** (stderr notice + README + report): for
HISAT2, `--multicore N` is interpreted as `-p N` intra-instance threading. (For a methylseq
drop-in this is strictly better — methylseq only needs `--multicore N` to yield correct, fast
output, and B gives deterministic == single-core output. But it is a divergence, so it is announced.)

---

## Proposed approach — spike-first, then one implementation phase

### Phase 0 — Spike (`/spike`, on oxy: 1M subset + HISAT2 2.2.2, the gate arch)

The pivot is unchanged, but B-locked sharpens what it must produce:

Tool: Perl `bismark --hisat2` is the oracle and drives HISAT2 exactly as the Rust port would, so
the spike is **Perl-only** for the pivot (no Rust build needed — the Rust↔Perl `-p N` parity is the
Phase-1 gate, like the local-mode spike deferred its end-to-end diff). Compare **decompressed BAM
content** (`samtools view`, no header / @PG-filtered).

1. **THE go/no-go probe — HISAT2 `-p N --reorder` vs bare-no-`-p` single-core, content-determinism.**
   ⚠️ **`-p 1` does not exist** (Perl/Rust both die, ≥2 required) → "single-core" is the **bare
   no-`-p`** run. For **N ∈ {2,4,8}**, run Perl `bismark --hisat2 -p N` and compare its BAM body to
   the bare single-core BAM body; **repeat each run ≥2×** (run-to-run determinism is the canonical
   threading-failure mode). Also **diff the spliced (N-CIGAR) subset specifically** — that is where
   the fork model drifted (1310 vs 1219), so it is where `-p N` is most likely to differ.
   - **`-p N` body == bare-single-core body, for all N, both repeats** → **B-strong** (node- AND
     N-independent — the ideal).
   - **deterministic-per-N but ≠ single-core** → **B-faithful** (gate vs Perl `--hisat2 -p N` per N).
   - **non-deterministic run-to-run even at fixed N** → B has no clean gate → escalate (likely defer
     to the stop-gap; never ship non-deterministic output).
   - **Hypothesis (B-#4, frames a failure):** HISAT2's per-read tie-break seed is read-derived /
     thread-independent → a-priori expect B-strong; a divergence implies thread-*shared* state
     (e.g. a thread-order-dependent splice-site table), which would also threaten determinism.
2. **Cross-check the rejected fork path:** Perl `--hisat2 --multicore N` (N matching) — confirm the
   rev-0 worker-variance (≠ single-core; the 1310-vs-1219 shape) so we are demonstrably *right* to
   avoid it under B.
3. **Measure the `-p N` speedup** vs single-core (sizes the perf payoff + informs Q2's thread-count mapping).
4. **`--ambig_bam` under B** (Assumption 3): under B there is **one** instance, so Perl's Bowtie-2-only
   ambiguous-BAM temp machinery (`@temp_ambig_bam`, SE `bismark:656/661`, PE `715/720`, each
   `# only for Bowtie 2` — NOT `676-684`, which is the HISAT2 output-name builder) is irrelevant, and
   the single-instance `--ambig_bam` path (already supported, `_bismark_hisat2.ambig.bam`) just works.
   Sanity-confirm in the spike or note as a Phase-1 gate cell; don't assume.

**Output:** B-strong vs B-faithful verdict + the per-artifact gate definition (BAM vs report) + a
sized implementation estimate.

### Phase 1 — implement Approach B (Bowtie 2 untouched)

- **Route, in `config.rs` + `lib.rs` (NOT just deleting the reject):** for `aligner == Hisat2 &&
  multicore > 1`, carry N into the aligner `-p` and force the **single-instance direct path**.
  ⚠️ Deleting the `config.rs:254` reject *alone* sends HISAT2 into `lib.rs:144/180`
  → `parallel::run_*_multicore` (the fork+chunk breakage). The route must set the single-instance
  path — recommended shape: `config.multicore = 1` (so `lib.rs` takes the direct path) + a new
  field (e.g. `hisat2_threads`/reuse `bowtie_threads`) that feeds `-p N --reorder` into
  `aligner_options` (built once from the thread knob, `options.rs:149`). The impl plan pins the exact seam.
- **Conflict (Q3):** fail-loud if the user passes BOTH `--hisat2 --multicore N` AND `-p M`.
- Emit the never-silent semantic-remap notice (stderr + report + README).
- **Per-artifact gate (rev-2):** **BAM** = B-strong (== single-core `--hisat2`) or B-faithful
  (== Perl `--hisat2 -p N`) per the spike; the **report** = always vs Perl `--hisat2 -p N` (it
  carries `-p N --reorder` in its options line, so it is never single-core-identical).
- Each phase: full plan → dual plan-review → implement → dual code-review + plan-manager → oxy gate.

---

## Validation

- **Spike gate:** HISAT2 `-p N` vs `-p 1` content-determinism (the B-strong/B-faithful pivot) +
  the captured Perl oracles + the `-p N` speedup + the `--ambig_bam`-under-B confirmation.
- **Implementation gate (B-strong) — BAM:** Rust `--hisat2 --multicore N` content **byte-identical
  to Rust `--hisat2` single-core** (decompressed BAM, @PG-filtered), for several N — proving
  node-independence. **Report** = always vs Perl `--hisat2 -p N` (the report's options line carries
  `-p N --reorder`, so it is not single-core-identical). Matrix: SE+PE × {dir,non-dir,pbat} ×
  {FastQ,FastA} + `--ambig_bam`/`--unmapped`/`--ambiguous`, or a justified subset (cheap: Rust-vs-Rust).
- **Implementation gate (B-faithful, if B-strong fails) — BAM + report:** Rust `--hisat2 --multicore N`
  byte-identical to **Perl `--hisat2 -p N`** for the matching N.
- **Conformance flip:** `bismark-aligner/tests/methylseq_conformance.rs::methylseq_align_hisat2_multicore_known_unsupported`
  **flips** → move `--hisat2`+multicore to an accept row + add the chosen-gate coverage.
- **README:** relax the `bismark_hisat` cpus-cap stop-gap note (`rust/README.md:64-72`) once shipped.
- **Regression:** Bowtie 2 `--multicore` worker-invariance (Phase 9b) **untouched** (B is a
  HISAT2-only branch); single-core `--hisat2` (`-p 1`) untouched; existing `--hisat2 -p N` behavior
  preserved.

---

## Assumptions

1. **(To confirm in spike — the pivot)** HISAT2 `-p N` is *at least* deterministic run-to-run at a
   fixed N (→ B-faithful), and ideally content-identical to `-p 1` (→ B-strong). If neither holds,
   B has no clean byte-gate → escalate (likely defer; the stop-gap already unblocks methylseq).
2. **(Fixed)** Bowtie 2's `--multicore` worker-invariance (Phase 9b) is unaffected — HISAT2-only branch.
3. **(To confirm in spike)** Under B (single instance), Perl's Bowtie-2-only multicore temp-name
   builder (`bismark:676-684`) is irrelevant; `--ambig_bam` uses the existing single-instance path.
4. **(Fixed)** `-p`/`--reorder` is already plumbed for HISAT2 (`options.rs:149`, not Bowtie 2-gated);
   B reuses it rather than building new threading.

---

## Questions or ambiguities

- **Q2 (impl-plan detail):** what does `--multicore N` map to under B — literal `-p N`, or a
  cpu-aware value? Under **B-strong** the thread count is a *perf-only* choice (any N → same output),
  so literal `-p N` is simplest. Under **B-faithful** the mapping must be pinned (the gate is per-N).
  *Spike's speedup measurement informs the perf side; default to literal `-p N`.*
- **Q3 (conflict edge case, impl-plan):** if a user passes BOTH `--hisat2 --multicore N` AND `-p M`
  (Rust `cli.bowtie_threads`), B's remap collides with the explicit `-p`. Decide: error (fail-loud,
  preferred), or `-p` wins / `--multicore` wins. *Recommend fail-loud on the explicit conflict.*
- **Q4 (priority — informational):** methylseq's default aligner is Bowtie 2; `bismark_hisat` is the
  less-common path, and the cpus-cap stop-gap already unblocks it. B is cheap (plumbing exists) so
  worth doing, but it is **not** an announcement blocker — the spike sizes it; if the spike surfaces
  non-determinism, defer cleanly.

---

## Self-Review

- **Logic:** B-locked. Grounded in the actual reject (`config.rs:254`) + its measured worker-variance,
  and correctly re-aimed: the fork-model breakage (chunked splice discovery) is *avoided* by the
  single-instance `-p N` whole-set model, which has an existing faithful Perl precedent
  (`bismark:7998-7999`) and existing Rust plumbing (`options.rs:149`). The only residual unknown is
  `-p N` threading determinism — exactly the spike's pivot, now picking the *gate* (B-strong vs
  B-faithful) rather than A-vs-B.
- **Scope discipline:** spike-first because the gate (and therefore the conformance assertion) is
  unknown until `-p N` determinism is established; Bowtie 2 + single-core HISAT2 fenced off.
- **Integration:** HISAT2-only branch; Bowtie 2 multicore + single-core HISAT2 untouched; the
  conformance `KnownUnsupported` row + the README stop-gap are the flip-detectors/handoffs.
- **Remaining risk:** if HISAT2 `-p N` is non-deterministic run-to-run, B has no clean gate (escalate
  / defer). The spike surfaces this cheaply before any implementation commitment. The semantic remap
  (`--multicore`→`-p` for HISAT2) is a documented, never-silent divergence from Perl's flag meaning.

---

## Spike Results (Phase 0 — 2026-06-13, oxy)

**Spike:** `spikes/SPIKE_hisat2_p_determinism.md` (+ `spikes/spike_hisat2_p_determinism.sh`, raw `spikes/spike_run.out`).
**Validated:** the B-strong-vs-B-faithful pivot — is HISAT2 `-p N --reorder` content-identical to bare
single-core, deterministically, for N ∈ {2,4,8}? (1M GRCh38 SE directional reads, HISAT2 2.2.2.)

**Outcome: B-strong REJECTED; B-faithful CONFIRMED.**
- **`-p N` is deterministic run-to-run** (every N: the two repeats byte-identical even in-order) ✅ →
  B-faithful (gate vs Perl `--hisat2 -p N` per N) is achievable.
- **`-p N` is NOT content-identical to single-core** ❌ — record count 844,267 (sc) → 844,316 (`-p 8`),
  spliced 1310 → 1298, sorted/content md5 differs. HISAT2's **threading itself perturbs the alignments**
  (dynamic, thread-order-dependent splice discovery). So **B-strong (== single-core) is impossible.**
- **`--multicore 4` ≠ single-core** (1237 spliced) — fork worker-variance confirmed (matches the reject's evidence).
- **No multicore HISAT2 mode is node-independent** — single-core (the shipped stop-gap) is the only
  node-independent/reproducible path; `-p N` lands *between* single-core (1310) and the fork model (1237).
- **Perf:** `-p 8` 1:36 vs single-core 2:05 (~24% faster) vs `--multicore 4` 1:14 (fork is the *fastest*
  multicore option; `-p` has diminishing returns, `bismark:7996`).

**[REVISED BY SPIKE]** — Assumption 1 and "The two candidate gates" / "B-strong" are superseded:
- B-strong (== single-core) is **unreachable**. The gate is **B-faithful only** (== Perl `--hisat2 -p N`
  per N). Output is **N-dependent** (node-size-dependent under methylseq's auto-derived N) — deterministic
  and Perl-faithful, but NOT the node-independence the rev-1 framing assumed.
- This **changes the basis on which Approach B was chosen** (it was locked believing it was
  node-independent == single-core). Approach A is *also* worker-variant AND has a fragile gate (Rust
  Phase-9b **contiguous** chunking ≠ Perl **modulo** chunking → not byte-identical for read-set-sensitive
  HISAT2 without replicating Perl's modulo split). The stop-gap (force single-core) is the only
  node-independent option. → **ESCALATED to Felix before writing the implementation plan** (B-faithful /
  reconsider A / keep stop-gap). See the chat decision.
