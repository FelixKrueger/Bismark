# PLAN_REVIEW_A — HISAT2 multi-core support (Approach B), rev 1

**Reviewer:** A (fresh context)
**Plan:** `plans/06132026_aligner-hisat2-multicore/PLAN.md` (rev 1, scoping / spike-first)
**Base verified:** worktree `/Users/fkrueger/Github/Bismark-hisat2mc` on branch `rust/aligner-hisat2-multicore` @ `f1bcf42` — matches the plan's stated base. All file:line claims below were checked against *this* worktree, not the stale main checkout.

**Scope note:** Approach B is LOCKED by Felix. This review does NOT re-litigate A vs B. It critiques whether B is correctly scoped, the spike correctly aimed, the gates well-defined, the assumptions/edge cases complete, and the validation sufficient.

**Verdict: APPROVE-WITH-CHANGES.** The plan's core thesis is sound and well-grounded in the source: the single-instance `-p N --reorder` machinery genuinely already exists and is genuinely not Bowtie 2-gated, and routing `--multicore N`→`-p N` for HISAT2 sidesteps the chunked-splice-discovery breakage. The spike is correctly aimed at the one real unknown (`-p N` threading determinism). **But there is one Critical gap that, if left as-is, will make the B-strong gate fail on the report file** (the `aligner_options` string is printed verbatim in the `_SE/PE_report.txt`, so a `-p N`-routed run can never be byte-identical to single-core `--hisat2` *on the report*, only on the BAM). And the plan's Phase-1 implementation sketch names the wrong interception point (`config.rs:254`) while omitting the actual dispatch fork in `lib.rs`. Both are fixable in the implementation-plan stage; neither invalidates Approach B.

---

## 1. Logic review

### 1.1 Load-bearing claims — verified TRUE
- **`-p`/`--reorder` plumbing exists and is NOT Bowtie 2-gated.** `options.rs:149-158`: `if let Some(p) = cli.bowtie_threads { … opts.push("-p {p}"); opts.push("--reorder") }`. The block runs for whatever aligner resolves (the `minimap2_options` path at `options.rs:221-223` *discards* this base and rebuilds, but Bowtie 2 + HISAT2 both keep it). The `// Bowtie 2 intra-instance threads` comment at `options.rs:149` is a misnomer, not a gate — `bismark_rs --hisat2 -p N` does emit `-p N --reorder` today. ✅ Plan §"Key facts" #2 is correct.
- **Perl ships a faithful `--hisat2 -p N --reorder` mode.** `bismark:7993-8007`: `if ($parallel){ die unless >1; push "-p $parallel"; push "--reorder"; … warn "Each HISAT2 instance is going to be run with $parallel threads" }`. The `--reorder` comment (`bismark:7999`) explicitly names "Bowtie 2 **or** HISAT2". ✅ Plan §"Key facts" #1 correct, including the HISAT2-specific warning at `bismark:8004`.
- **`--parallel` is an alias for `--multicore`, NOT for `-p`.** `bismark:7361` `'parallel|multicore=i' => \$multicore`; `bismark:7348` `'p=i' => \$parallel`. ✅ Plan's "naming trap" table is correct. Rust mirrors this: `cli.rs:187` `#[arg(long = "multicore", visible_alias = "parallel")]` for `multicore`, and `cli.rs:176` `#[arg(short = 'p')]` for `bowtie_threads`. Good — the alias topology matches Perl.
- **The reject's worker-variance evidence is real.** `config.rs:244-262` documents the 1310-vs-1219 spliced finding and rejects `aligner == Hisat2 && cli.multicore.unwrap_or(1) > 1`. ✅
- **The conformance flip-detector exists.** `methylseq_conformance.rs:211` `methylseq_align_hisat2_multicore_known_unsupported` asserts the reject fires for `--hisat2 --multicore 2`. ✅ (Minor: its docstring at line 208 cites `config.rs:251`; the actual reject is `:254`. The plan already flags the rev-0 `:251`→`:254` slip — worth fixing in the test docstring too when the test flips.)
- **README stop-gap.** `rust/README.md:64-72` documents the cpus-cap workaround verbatim as the plan describes, including the `ext.args` last-wins trap. ✅

### 1.2 The `--ambig_bam` claim — verified TRUE, but the line cite is imprecise
The plan (Assumption 3, §Phase-0 #5) says Perl's multicore temp-name builder is "Bowtie 2-only (`bismark:676-684`)". **The underlying claim is correct but the line numbers are wrong/misleading.** `bismark:676-684` is the *HISAT2* per-chunk output/report temp-name builder (the `else { # HISAT2 }` arm). The thing that is genuinely Bowtie 2-only is the **`@temp_ambig_bam`** array: it is pushed ONLY inside the `if ($bowtie2)` arms — SE `bismark:656` & `:661`, PE `:715` & `:720`, each annotated `# only for Bowtie 2`. The `mm2` and HISAT2 arms (664-682, 723-742) never push `@temp_ambig_bam`. **So the conclusion holds**: under Perl fork-multicore, `--hisat2 --ambig_bam` produces no merged ambiguous BAM. And under Approach B there is one instance, so the single-instance `--ambig_bam` path (already supported, `_bismark_hisat2.ambig.bam`) applies. **Action:** correct the cite to "`@temp_ambig_bam` pushed only in the `if ($bowtie2)` arms — SE 656/661, PE 715/720" so the spike/implementer doesn't go looking at 676-684 and get confused. The spike's plan to *confirm* (not assume) `--ambig_bam`+B is good.

### 1.3 CRITICAL — the report file breaks the B-strong gate as currently defined
This is the one substantive logic gap.

`report.rs:67-72` builds the report header line **"Bismark was run with HISAT2 … with the specified options: {aligner_options}"** by printing `h.aligner_options` *verbatim*. Approach B injects `-p N --reorder` into `aligner_options` (that is precisely the mechanism the plan reuses). Therefore:

- single-core `--hisat2` report options: `… --ignore-quals …` (no `-p`).
- B-routed `--hisat2 --multicore N` report options: `… -p N --reorder --ignore-quals …`.

These **differ in the report**, even if the BAM content is identical. The plan's B-strong gate is defined twice and inconsistently:
- §"two candidate gates" and Validation/B-strong say "byte-identical to **single-core `--hisat2`**" — broad, implies report too.
- The Validation B-strong line then narrows to "(decompressed BAM, @PG-filtered)" — BAM only.

If B-strong is meant to cover the report, **it is unachievable**: the report under B is inherently B-faithful-shaped (it matches Perl `--hisat2 -p N`, which also prints `-p N --reorder`, not single-core). This is not a bug to fix — it is a definitional issue the plan must resolve. Recommended framing:

> Under B, the **BAM** may be B-strong (== single-core, if the spike confirms `-p N` content == `-p 1`), but the **report** is *always* B-faithful (== Perl `--hisat2 -p N`, because both embed `-p N --reorder` in the options string). The gate must be stated per-artifact: BAM gated vs single-core (B-strong) OR Perl `-p N` (B-faithful); report **always** gated vs Perl `--hisat2 -p N`.

Without this carve-out, the conformance suite's "assert any-N == single-core" idea (plan §"two candidate gates", B-strong bullet) will fail on the report bytes. **This is the highest-value catch in the review.**

(Sub-point: the `@PG` line in the BAM — added by samtools/the writer — will likewise carry the full argv including `-p N`; the gate harness already `@PG`-filters per the Validation line and the aligner epic's established practice, so the BAM side is fine. It's the *report* that has no @PG-style filter.)

### 1.4 IMPORTANT — Phase-1 names the wrong interception point and omits `lib.rs`
Plan §Phase 1: "Replace the `config.rs:254` reject with a **route**: … set the single-instance path and inject `-p multicore`". Replacing the `config.rs:254` reject is necessary but **not sufficient**, and on its own would route HISAT2 into the *fork* path, not the single-instance path. The actual dispatch fork is in **`lib.rs:115-187`** (`fn pipeline`):
- `lib.rs:144` SE: `} else if n > 1 { parallel::run_se_multicore(config, reads, n) }`
- `lib.rs:180` PE: `} else if n > 1 { parallel::run_pe_multicore(config, mates1, mates2, n) }`

`n` here is `config.multicore` (`lib.rs:119`), which is `cli.multicore.unwrap_or(1)` (`config.rs:363`). So if you merely delete the `config.rs:254` reject and leave `config.multicore = N`, **`--hisat2 --multicore N` will fall into `parallel::run_*_multicore` — the exact fork+chunk path the plan is trying to avoid** (and which produces the 1310-vs-1219 non-faithful output). Approach B must, for HISAT2:
1. NOT set `config.multicore > 1` (or otherwise gate the `n > 1` branch to Bowtie 2), AND
2. Inject `-p N --reorder` into `aligner_options` so the single direct path (`run_se`/`run_pe`) threads.

The cleanest shape (for the impl plan to decide): in `resolve()`, when `aligner == Hisat2 && cli.multicore > 1`, set `config.multicore = 1` (so `lib.rs` takes the direct path) and treat the requested N as the `-p` value fed into `build_aligner_options`. That keeps `lib.rs:144/180` Bowtie 2-only by construction. **The plan should name `lib.rs:115-187` as a required touch-point** — rev 1 lists only `config.rs:254`, which is misleading about where the real fork lives.

### 1.5 IMPORTANT — interaction with the existing `-p < 2` validation (Q3 is bigger than stated)
`options.rs:150-155`: if `cli.bowtie_threads` is `Some(p)` with `p < 2`, it errors "Please select a value for -p of 2 or more!" (mirrors Perl `bismark:7994`). Approach B *synthesizes* a `-p` value from `--multicore`. Two consequences the plan should pin in the impl plan:
- **Where does B inject `-p N`?** If it sets `cli.bowtie_threads = Some(N)` and re-runs `build_aligner_options`, the `p < 2` guard is fine for `N≥2` (the only case B fires). But if B instead pushes `-p N` directly into the options string it bypasses the guard — pick one path and keep the guard semantics.
- **Q3 conflict (`--multicore N` AND `-p M`)**: fail-loud is the right call (✅ agree with the plan's recommendation). Note there is **no silent default a user relies on** — `bowtie_threads` is `Option<u32>` defaulting to `None` (`cli.rs:177`), so today `--hisat2 -p M` and `--hisat2 --multicore N` are independent and the latter is rejected. So fail-loud on the *coexistence* of the two introduces no regression. Suggest the error name both flags and state that for HISAT2 `--multicore` is interpreted as `-p`.

---

## 2. Assumptions

- **Assumption 1 (the pivot — `-p N` determinism).** Correctly identified as the spike's go/no-go. ✅ See §3.1 for whether the spike *probe* is sufficient to establish it.
- **Assumption 2 (Bowtie 2 multicore untouched).** ✅ Sound — B is a HISAT2 branch; `lib.rs:144/180` stay Bowtie 2 once §1.4 is done correctly. Worth an explicit regression assertion that Bowtie 2 `--multicore` still routes to `parallel.rs` (a test that the SE/PE Bowtie 2 multicore path is unchanged).
- **Assumption 3 (`--ambig_bam` under B).** Conclusion correct; line cite wrong (§1.2). Confirm-in-spike is the right posture.
- **Assumption 4 (`-p`/`--reorder` already plumbed).** ✅ Verified (`options.rs:149`).
- **MISSING assumption — the report is options-bearing (§1.3).** The plan has no assumption acknowledging that the report header embeds `aligner_options` verbatim and therefore can never be byte-identical between a `-p`-bearing run and single-core. This should be an explicit assumption + gate carve-out.
- **MISSING assumption — `--reorder` semantics.** The plan asserts "`--reorder` fixes output order" so content comparison is "modulo nothing". This is the right instinct but is itself an assumption about HISAT2's `--reorder` (see §3.1) and should be labeled "(to confirm in spike)" rather than stated as fact in §Phase-0 #1.

---

## 3. Validation sufficiency

### 3.1 CRITICAL-adjacent — is the spike's determinism probe framed correctly?
The plan frames the probe as "`-p N` vs `-p 1` content-identical, modulo nothing because `--reorder` fixes order". Two refinements needed:

1. **`--reorder` guarantees output *order* == input order, not that record *content* is `-p`-invariant.** It is entirely possible for HISAT2 `-p N` (with `--reorder`) to be (a) deterministic run-to-run, (b) correctly ordered, yet (c) differ from `-p 1` on a *subset* of reads where multi-threading changes a tie-break, repeat-seed RNG draw, or pseudo-random multi-mapping pick. `--reorder` would *hide* such a difference at the row level only if it were an ordering artifact — but a content difference (different chosen alignment for an ambiguous read) survives reordering and shows up as a differing record at the same input position. So **content comparison (not just order) is exactly right** (the plan says this), but the plan should state that the comparison is **positional record-by-record after `--reorder`**, and that a `-p N ≠ -p 1` outcome is a *content* divergence, not an order one. The plan's three-way verdict (identical → B-strong; deterministic-but-≠ → B-faithful; non-deterministic → escalate) is the correct decision tree.

2. **Run-to-run determinism at fixed N must be tested with ≥2 repeats, not inferred.** The "deterministic-but-≠-`-p 1`" branch (B-faithful) requires that `-p N` is reproducible across runs. The spike must run `-p N` **at least twice** and diff the two, else B-faithful's per-N gate is unfounded. The plan implies this ("non-deterministic run-to-run even at fixed N") but doesn't list the repeat-run as a concrete spike step. **Add it.**

3. **Test more than one N.** §Q5 of the brief and my own read agree: B-strong's claim is *N-independence*. A single N (say N=8) proves nothing about N=2 or N=4. The spike (and later the gate) should cover **N ∈ {2, 4, 8}** at minimum. Cheap to add in the spike; essential for the conformance "any-N == single-core" assertion to mean anything. The plan's Phase-0 only says "`-p N`" (one value) — make it a small set.

4. **Scale risk.** The 1M oxy subset may be too small to surface a threading non-determinism that only manifests with enough reads to fill multiple thread work-queues with contended ambiguous reads. This is a known class of "passes small, fails large" trap. Mitigation: the spike should at least *note* the risk, and the eventual implementation gate (which the aligner epic runs at full scale, ~84M reads) is the real backstop. Recommend the plan explicitly defer the scale-confidence to the Phase-10-style full-scale gate and not claim node-independence from the 1M subset alone. (The existing aligner epic already gates at full scale, so this is a "carry it through" note, not new infrastructure.)

### 3.2 Gate definition completeness
- **BAM gate:** well-defined (decompressed, @PG-filtered) for both B-strong and B-faithful. ✅
- **Report gate:** UNDEFINED / wrong (§1.3). Must be added as: report always == Perl `--hisat2 -p N`.
- **`--ambig_bam` BAM:** the spike confirms it works under B; the gate should include an `--ambig_bam` cell so the single-instance ambig BAM is byte-checked (the plan mentions confirming it works but doesn't put it in the *implementation* gate matrix — add it).
- **PE coverage:** the plan's gate language is SE-flavored. HISAT2 PE is shipped and `-p N --reorder` applies to PE too (`bismark:7998-7999` is library-agnostic). The gate matrix should include **SE and PE** HISAT2 multicore cells.
- **Conformance flip:** correct (`methylseq_conformance.rs:211` → accept row). Remember to also update the test docstring's stale `:251` cite.

### 3.3 Regression coverage
Good: the plan explicitly fences Bowtie 2 `--multicore` (Phase 9b) and single-core `--hisat2`. Add one concrete regression assertion that `--hisat2 -p N` (explicit, no `--multicore`) is **unchanged** — that path already runs today and B must not perturb it (it's the same `options.rs:149` mechanism, so a test pinning the emitted options is cheap insurance).

---

## 4. Alternatives / trade-offs (within the B-locked frame — NOT re-litigating A)

- **Implementation shape for §1.4.** Two ways to make `lib.rs` take the direct path while threading: (i) in `resolve()` set `config.multicore = 1` and carry N as the `-p` value; (ii) keep `config.multicore = N` but gate `lib.rs:144/180` `n > 1` with `&& aligner == Bowtie2`. Option (i) is cleaner (the `n > 1` fork stays a pure Bowtie 2 concept and `config.multicore` keeps its documented "file-level worker count" meaning, `config.rs:202-205`). Option (ii) risks other readers of `config.multicore` (e.g. any future report/perf text) seeing N and assuming forking. **Recommend (i)** and note it in the impl plan. Either way, the plan should pick and document, since "set the single-instance path" (rev 1) is underspecified.
- **Q2 thread-count mapping.** Under B-strong, literal `-p N` is fine (output is N-invariant). Under B-faithful, the mapping is load-bearing (gate is per-N) — pin it to literal `-p N` and gate each N. The plan's default-to-literal is reasonable; just make the B-faithful per-N gate explicit (already implied).
- **Documentation-only mitigation for the semantic remap.** The plan relies on "never-silent documentation" (stderr + README + report). Given §1.3, the *report* will already visibly carry `-p N --reorder`, which is itself a form of disclosure to anyone reading the report. The stderr notice is still warranted for interactive users. Fine as planned.

---

## 5. Action items (prioritized)

### Critical (resolve before/at the implementation-plan stage)
- **C1 — Fix the B-strong gate definition for the report.** The report header embeds `aligner_options` verbatim (`report.rs:67-72`), so a `-p N`-routed run can never be byte-identical to single-core `--hisat2` *on the report*. Redefine the gate per-artifact: **BAM** may be B-strong (vs single-core) or B-faithful (vs Perl `-p N`) per the spike; **report is ALWAYS gated vs Perl `--hisat2 -p N`** (both embed `-p N --reorder`). Drop/qualify the "conformance asserts any-N == single-core" idea accordingly — it can hold for the BAM only.
- **C2 — Name `lib.rs:115-187` as the real interception point.** Deleting only the `config.rs:254` reject routes HISAT2 into the fork path (`lib.rs:144/180` `else if n > 1 → parallel::run_*_multicore`) — the very breakage being avoided. The plan must specify that B sets the single-instance direct path (recommend: `config.multicore = 1` + carry N as `-p`) so `lib.rs`'s `n > 1` fork stays Bowtie 2-only.

### Important
- **I1 — Spike: test ≥2 N values and ≥2 repeat runs per N.** B-strong's N-independence and B-faithful's per-N determinism are both unproven by a single N / single run. Cover N ∈ {2,4,8}, each run twice (run-to-run determinism), all `--reorder`. (§3.1)
- **I2 — Spike: clarify `--reorder` is order-only.** State the comparison is positional record-by-record post-`--reorder`; a `-p N ≠ -p 1` outcome is a content divergence `--reorder` does NOT hide. Re-label "modulo nothing" as the *hypothesis under test*, not a fact. (§3.1, §Assumptions)
- **I3 — Correct the `--ambig_bam` citation.** It's not `bismark:676-684` (that's the HISAT2 output-name builder); the Bowtie-2-only thing is the `@temp_ambig_bam` push, SE `bismark:656/661`, PE `715/720` (`# only for Bowtie 2`). Conclusion unchanged; cite needs fixing. (§1.2)
- **I4 — Gate matrix must include PE and `--ambig_bam` cells**, not just SE. `-p N --reorder` is library-agnostic in Perl (`bismark:7998-7999`); HISAT2 PE is shipped. (§3.2)
- **I5 — Q3 fail-loud: confirm no regression + name both flags.** `cli.bowtie_threads` defaults `None` (`cli.rs:177`), so `--multicore N` + `-p M` coexistence is a new combination — fail-loud introduces no regression. Decide whether B sets `cli.bowtie_threads` (re-using the `options.rs:150` `p<2` guard) or pushes `-p N` raw. (§1.5)
- **I6 — Defer node-independence confidence to the full-scale gate.** 1M may mask a scale-only threading non-determinism; don't claim node-independence from the subset alone — carry it to the Phase-10-style full-scale gate. (§3.1.4)

### Optional
- **O1 — Update the conformance test docstring** (`methylseq_conformance.rs:208`) stale `config.rs:251` → `:254` when the test flips.
- **O2 — Add a regression test pinning `--hisat2 -p N` emitted options** (unchanged today; cheap insurance against B perturbing the shared `options.rs:149` path).
- **O3 — Fix the misleading `// Bowtie 2 intra-instance threads` comment** at `options.rs:149` (it runs for HISAT2 too) when this lands.

---

## Summary
Approach B is correctly grounded and the spike is aimed at the right pivot. **Two Critical items before implementation:** (C1) the B-strong gate is unachievable for the *report* because `aligner_options` (incl. `-p N --reorder`) is printed verbatim — the report is intrinsically B-faithful-shaped; gate per-artifact. (C2) the plan must intercept at `lib.rs:115-187` (the `n>1` fork), not merely delete the `config.rs:254` reject, or HISAT2 falls straight back into the fork+chunk path it's trying to avoid. Important: spike must vary N and repeat runs (I1), treat `--reorder` as order-only (I2), fix the `--ambig_bam` cite (I3), cover PE + `--ambig_bam` in the gate (I4). All are fixable at the impl-plan stage; none invalidate B.
