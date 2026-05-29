# Phase B PLAN — Plan Review A

**Reviewer:** Plan Reviewer A (independent, fresh context)
**Target:** `phase-b-core-report/PLAN.md` (rev 0)
**Contract:** byte-identical to Perl `coverage2cytosine` v0.25.1 (file outputs; STDERR exempt)
**Verdict:** **APPROVE WITH CHANGES** — the coordinate arithmetic, the single-kernel equivalence, the chromosome-ordering model, and the context-summary layout are all *verified faithful to the Perl* by line-by-line tracing + executable cross-checks (Perl ⇄ Rust). No Critical defects. A handful of Important clarifications/test-gaps should be folded before implementation; the `%.2f` open question is empirically a non-issue.

All Perl line numbers below refer to `/Users/fkrueger/Github/Bismark-c2c/coverage2cytosine`.

---

## What I verified by execution (not just by reading)

I ran the actual Perl `substr`/`tr`/regex semantics and the actual Rust `format!` to ground the highest-risk claims rather than trusting prose:

1. **Coordinate arithmetic** — ran Perl's exact `substr`/revcomp on `seq="CGTACGN"` for every C/G at i=0, 1, interior, len-2:
   - i=0 C, pos=1: Perl `tri="CGT"`, `up="N"` (negative wrap to last char). Plan §3.3 `tri=seq[0..3]="CGT"`, `up=perl_substr(seq,-1,3)="N"`. **MATCH.**
   - i=1 G, pos=2: Perl `tri="CG"` (len 2, dropped), `up="ACG"`. Plan `tri=seq[0..2]="CG"`. **MATCH.**
   - i=5 G, pos=6: Perl `tri=revcomp("ACG")="CGT"`, `up=revcomp("CGN")="NCG"`. Plan `tri=revcomp(seq[3..6])="CGT"`, `up=revcomp(perl_substr(seq,4,3))="NCG"`. **MATCH.**
   The plan's slice formulas (§3.3, assumption 1) are **byte-faithful** to Perl `:263/:288` (forward-C), `:294-302/:335-337` (reverse-G).

2. **`perl_substr` negative-wrap reaches output** — confirmed the i=0 forward-C `upstream` wrap (`substr(seq,-1,3)`) feeds `ubase = upstream[0]`, which **does** reach the context summary and **does** accumulate when pure-ACTG (P3 is load-bearing, not cosmetic). Plan correctly isolates it (assumption 2) and tests it (V14 "i=0 wrap ubase").

3. **`tri_nt` never uses a negative offset** (assumption 2) — verified across all three blocks: forward-C `substr(seq,i,3)` (i≥0); reverse-G `else` branch only runs when `pos-3≥0` (i≥2); reverse-G `i<2` branch uses offset 0. **TRUE.** Only `upstream` wraps. Reverse-G `upstream` at i=0 would also wrap, but those positions are dropped by len<3 **before** `context_reporting` (verified: i=0 G → tri len 1; i=1 G → tri len 2), so the reverse-G wrap never reaches output.

4. **THE single-kernel claim (§3.2)** — traced all three blocks guard-by-guard:
   - Covered (`:343` len<3 → `:347` last-base → `:355` lookup → `:361` threshold → `:365` classify → `:381` accumulate → `:384` emit).
   - Last-chr (`:588` lookup → `:594` threshold → `:597` len<3 → `:600` last-base → `:608` classify → `:624` accumulate → `:628` emit).
   - Uncovered (`:1495` len<3 → `:1500` last-base → `:1505` classify → `:1519` emit; **no** lookup, **no** threshold, **no** `context_reporting`).
   Every guard is a `next` (skip-only). The emitted set = the AND of all guard predicates, which is **order-independent**. I deliberately probed the adversarial cases the task names — (a) a len<3 chr-edge position that *also* has stored coverage ≥ threshold, and (b) a sub-threshold position that is *also* unclassifiable: in both, the last-chr block evaluates the guards in a different order but reaches the **same `next`** (skip), and the only divergence is *which* `warn` fires (STDERR, exempt). **No input produces a different emitted line set or a different emitted-position order.** The single-kernel claim is **correct**.
   - Summary-accumulation gating also matches: `context_reporting` runs **after** the classify-`else{next}` in both covered (`:374-381`) and last (`:618-624`) blocks (so unclassifiable never accumulates), and is **absent** in the uncovered block — exactly the plan's `accumulate_summary=false` for uncovered (§3.3 step 7, §3.5).

5. **Chromosome ordering** — verified Perl's `%processed` is pre-seeded with **every genome chromosome name = 0** at genome-load (`:1712`, `:1734`), set to 1 for covered chrs (`:240`, `:478`); the uncovered loop is `foreach sort keys %processed` skipping value==1 (`:722-727`). The plan's model (covered via streaming `seen`; uncovered = `names_sorted() \ seen`) is **equivalent**, including the tricky case of a **cov chr absent from the genome**: Perl adds it to `%processed=1` so it's in the sort but skipped; the plan adds it to `seen` but it's not in `names_sorted()` (genome-only) so it's naturally excluded. **Same emitted uncovered set, same sorted order.** Empty-cov die (`:472-474`) precedes the uncovered loop (`:718`) — plan §3.1 step 4 / assumption 5 faithful.

6. **`cov chr absent from genome`** — ran `while(undef =~ /[CG]/g)` → **0 iterations**, non-fatal (`-w` warning only). Plan §3.2 "emit nothing, continue" is correct; the chr is still recorded in `seen` (matches `$processed=1`). **Correct.**

7. **Context summary** — reproduced Perl's 64-key init + `sort keys` double-loop: 64 rows, primary sort tri_nt (`CAA, CAC, …, CTT`), secondary ubase (`A,C,G,T`); columns `ubase\ttri_nt\tubasetri_nt\tm\tu\tperc`. Plan §3.6 **matches exactly** (header bytes, row order, `N/A` vs `%.2f`). Also verified the grid can never gain a 65th key: forward-C and revcomp-G `tri_nt` always start with `C`, and the pure-ACTG accumulate-gate (`:1984`) restricts to `C{ACGT}{ACGT}` = exactly the 16 pre-initialized keys → no autovivified extra rows.

8. **`%.2f` rounding parity (Open Q #10)** — compiled and ran Rust `format!("{:.2}", f64)` vs Perl `sprintf "%.2f"` on (a) literal half-way doubles `0.125/0.135/2.675/50.005/12.345/87.655/0.005/1.005` and (b) realistic `m/(m+u)*100` percentages incl. `403/803=50.19`, `2005/4005=50.06`, `1/3=33.33`. **Every case is byte-identical.** Both round-half-to-even on the actual stored IEEE-754 double and agree. → **The open question is empirically a non-issue.** (See Important-2: downgrade it.)

---

## Logic review

- **Guard order in the kernel (§3.3):** the plan adopts the *covered-chr* order (len<3 → last-base → lookup → threshold → classify → accumulate → emit). This is the correct choice — it's the order with the most early-exits and is provably output-equivalent to the other two blocks (verified above). Good.
- **`pos = i+1` (§3.3, P4):** correct — Perl `pos()` returns offset-past-match; a base at 0-based `i` yields `pos=i+1`, then `substr(pos-1,3)=seq[i..i+3]`. The zero_based report value `pos-1==i` (§3.4) is consistent.
- **Last-base guard (§3.3 step 3):** `(seq.len() as u32 - pos) == 0`. Perl `:347` / `:600` / `:1500` use `length($chr) - $pos == 0`. With `pos=i+1`, this fires exactly at `i==len-1` (the final genome base). Correct. (Note: this guard uses `pos`, the **1-based** value, *before* any zero_based subtraction — the plan computes zero_based `pos-1` only at emit time in §3.4, so the guard is unaffected. Good — matches Perl, where `$pos -= 1` happens only inside the print branches at `:397/:431/etc.`.)
- **Streaming flush (§3.1):** matches Perl's `while(<IN>)` first-chr init (`:212-220`) + flush-on-chr-change (`:227-468`) + final flush (`:476-690`). **One implementation detail to make explicit (Important-1):** when the chr field changes, Perl stores the *triggering* line's `(start,meth,nonmeth)` into the **fresh** buffer *after* flushing the old one (`:453-455`). The plan says "clear the buffer and start the new one" but never states that the triggering line's datum must be inserted into the new buffer. An implementer who flushes-then-`continue`s would silently drop one covered position per chromosome boundary. Spell this out.

---

## Assumptions

- **Assumption 1 (coordinate arithmetic is the single source of truth):** verified faithful (above). ✔
- **Assumption 2 (only `upstream` negative-wraps):** verified TRUE (above). ✔ — but the prose under §3.3 reverse-G is slightly imprecise: it shows the `i<2` branch as `tri_nt = seq[0..i+1]` *without* revcomp ("will be <3 → dropped"), whereas Perl applies `reverse`+`tr` **unconditionally** (`:301-302`, outside the if/else). The *outcome* is identical (these are dropped by len<3 regardless), but an implementer who reuses the `tri_nt` value for anything else would be surprised. See Optional-1.
- **Assumption 3 (single kernel ≡ all three blocks):** verified TRUE (above). ✔
- **Assumption 4 (cov chr absent from genome → emit nothing):** verified TRUE. ✔
- **Assumption 5 (empty cov → error before uncovered pass):** verified TRUE. ✔
- **Assumption 6 (`f64` `%.2f` parity):** verified TRUE empirically. ✔ (stronger than the plan claims).

No hidden/unstated assumption was found to be wrong. One genuinely-subtle equivalence the plan **relies on but does not document** is the `%processed`-pre-seeding ⇄ `names_sorted() \ seen` identity (incl. the absent-from-genome chr behavior). It is correct, but worth a one-line note so a future reader doesn't "fix" it (Important-3).

---

## Efficiency

Sound and explicitly matches Perl's single-threaded, whole-genome-in-RAM model: O(genome length) walk, O(1)-amortized per-position `HashMap` lookup, one chromosome's cov buffer resident at a time, 8 KiB `BufWriter`. No premature parallelism (correctly deferred to v1.x per SPEC §10.7). One micro-note: building the report line into a reusable byte buffer (§3.4) and `write_all`-ing it avoids per-field `write!` syscalls — the plan already implies this ("written via a byte buffer"); good. No concerns.

---

## Validation sufficiency

V1–V15 cover the crux primitives, both strands, edges, the threshold/zero_based/CpG-vs-CX matrix, both ordering rules, empty-cov, absent-chr, the summary, and a Perl-golden integration test. This is a strong matrix. Gaps to close (Important-4 / Optional):

- **No test for a covered chromosome appearing *mid-file* in a multi-FASTA** where genome order ≠ cov order ≠ sorted order. V10 (`chrB,chrA`) exercises 2-chr reversal but not the three-way "covered chr is neither first nor last in the genome, interleaved with uncovered chrs." Add a fixture: genome `[chrA,chrB,chrC,chrD]`, cov touches `chrC` then `chrA` → expect report order `chrC, chrA` (covered, appearance) **then** `chrB, chrD` (uncovered, sorted). This is the exact byte-identity trap P1 guards and deserves a dedicated assertion. **(Important-4.)**
- **No explicit assertion on the raw report-line bytes** (chr + tri_nt emitted as raw, un-revalidated bytes; tab separators; trailing `\n`; `+`/`-` strand byte). V15 covers this transitively via the golden diff, but a focused unit test on one emitted line (asserting the exact `b"chr1\t3\t-\t0\t0\tCG\tCG\n"`-style bytestring) would localize a regression that the integration diff only reports as "files differ." **(Important-4.)**
- **`%.2f` half-way case** (Open Q #10): now empirically shown to match — add ONE golden row with a non-trivial percentage (e.g. m=403,u=400 → `50.19`) to V14/V15 to lock it, then close the question. **(Important-2.)**
- **Threshold>0 suppressing the uncovered pass** — V11 says "none when threshold>0," good; ensure the integration matrix (V15) actually runs a `--coverage_threshold N>0` case end-to-end and asserts **zero uncovered chromosomes** in the output (Perl `:714`), not merely the unit-level check. **(Optional-2.)**
- **classify "CCG"→CHG** (the `^CG`-fails-then-`^C.G$`-matches path) is implied by V3 but not enumerated; add it so the `^CG` vs `^C.G$` precedence is pinned. **(Optional-3.)**
- **A position with stored coverage at a len<3 chr-edge / last-base** (the adversarial guard-order case) — add a unit case proving the kernel skips it (no emit) regardless, to lock the single-kernel equivalence at the test level. **(Optional-4.)**

The integration golden (V15) is the real safety net and is well-placed; the unit gaps above are about *localizing* failures, not about uncaught silent-wrong-output (I found no scenario the matrix would let through silently).

---

## Alternatives considered

- **Replicating Perl's three-block duplication instead of one kernel** — rejected correctly; the plan's single-kernel is provably equivalent and avoids the dual-driver back-port trap the memory warns about. Endorse.
- **Modeling `perl_substr` exactly vs only its used domain** — the plan models the full negative-from-end / end-truncation / empty-if-OOR semantics. This is the right call: it's only ~10 lines, it's unit-tested (V1), and it removes a class of off-by-one reasoning. Endorse. (Confirm the spec also returns empty — not panic — for a *positive* offset ≥ len, since reverse-G upstream at i=0 computes `perl_substr(seq,-1,3)` on a value that's then discarded; a panic there would be a latent bug even though the result is unused.)
- **`indexmap`/`Vec` for the covered list** — N/A here: the plan streams and flushes per-chromosome (no covered-list structure needed at all for ordering, because output happens at flush time in appearance order). This is actually *simpler* than the SPEC §10.4 "insertion-ordered structure" framing and is correct. The only ordered structure that matters is the implicit cov-file read order. Good.

---

## Action items (prioritized)

### Critical
*(none — no defect that would break byte-identity was found.)*

### Important
1. **(§3.1 step 3)** State explicitly that on a chromosome-change, the **triggering line's** `(start, meth, nonmeth)` must be inserted into the **fresh** buffer after flushing the previous chromosome (Perl `:453-455`). As written ("clear the buffer and start the new one"), an implementer could drop the first covered position of every non-first chromosome.
2. **(§10 Open Q #10, V14/V15)** Downgrade the `%.2f` rounding question to **resolved/non-issue**: Rust `format!("{:.2}",f64)` and Perl `sprintf "%.2f"` were verified byte-identical on literal half-way doubles *and* realistic `m/(m+u)*100` values. Add one golden row with a non-round percentage (e.g. `403/803 → 50.19`) to lock it; drop the "switch to explicit rounding helper" contingency or keep it only as a dormant note.
3. **(§3.5 / assumptions)** Add a one-line note that `names_sorted() \ seen` reproduces Perl's `sort keys %processed` (pre-seeded with all genome names at load, `:1712/:1734`), **including** that a cov chr absent from the genome lands in `seen` but not in `names_sorted()` and is therefore correctly excluded from the uncovered pass. Prevents a future "simplification" from breaking it.
4. **(§9 / V10 / V15)** Add the three-way ordering fixture (covered chr mid-genome, interleaved with uncovered) and a focused raw-report-line bytestring assertion. These localize the two highest-value byte-diff classes (chromosome order P1; exact line format — the contract for Phases C/D and the extractor).

### Optional
1. **(§3.3 reverse-G prose)** Note that Perl applies `reverse`+`tr` to the `i<2` branch `tri_nt` **unconditionally** (`:301-302`); the plan's "no revcomp, dropped anyway" is outcome-correct but prose-imprecise.
2. **(V15 matrix)** Ensure a `--coverage_threshold N>0` run is in the end-to-end matrix and asserts **zero** uncovered-chromosome lines (Perl `:714`).
3. **(V3)** Enumerate `CCG→CHG` to pin the `^CG` vs `^C.G$` precedence.
4. **(V4/V6)** Add a unit case: a len<3 chr-edge / last-base position that *also* has stored coverage ≥ threshold → still skipped (locks the single-kernel guard-order equivalence at test level).
5. **(perl_substr spec)** Confirm `perl_substr` returns empty (not panic) for a positive offset ≥ len too (reverse-G i=0 computes-then-discards a wrapped value).

---

## Summary

The plan is **algorithmically faithful** to Perl v0.25.1 on every byte-identity-critical axis I could test: coordinate arithmetic (forward-C and reverse-G `tri_nt`/`upstream`, the i=0 negative-wrap, the `i<2` edge, the last-base exclusion), the single-shared-kernel equivalence across all three Perl blocks (covered/last/uncovered — verified output-set-identical including the named adversarial cases), the covered-appearance vs sorted-uncovered chromosome ordering (incl. empty-cov-die-first and absent-chr handling), and the 64-row context summary (layout, sort, `N/A` gating, and — newly confirmed — `%.2f` parity). No Critical issues. Fold the four Important clarifications (especially the chr-boundary triggering-line insertion and the three-way ordering + raw-line-bytes tests) and this is ready for implementation.

**File written:** `/Users/fkrueger/Github/Bismark-c2c/plans/05292026_bismark-coverage2cytosine/phase-b-core-report/PLAN_REVIEW_A.md`
