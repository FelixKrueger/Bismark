# SPIKE — HISAT2 `-p N` determinism for Bismark aligner Approach B

**Date:** 2026-06-13 (oxy) · **Plan:** `../PLAN.md` (Phase 0) · **Status:** ✅ COMPLETE — **decisive: B-strong is DEAD, B-faithful is VIABLE.**

## 1. Question, success criteria, scope

**Question (the B-strong vs B-faithful pivot):** Does Bismark `--hisat2 -p N --reorder` (ONE instance,
whole read set, N≥2) produce decompressed-BAM content **identical to the bare-no-`-p` single-core run**,
deterministically run-to-run, for N ∈ {2,4,8}? And does `--multicore N` (fork+chunk) differ (the
worker-variance B is meant to avoid)?

⚠️ **`-p 1` does not exist** — Perl dies (`bismark:7994`, `die unless $parallel>1`), Rust mirrors it
(`options.rs:151`). "Single-core" = the **bare no-`-p`** invocation (the shipped faithful HISAT2 path).

**Success criteria (what each outcome means):**
- `-p N` body == bare-single-core body for all N, both repeats → **B-strong** (== single-core, node- AND N-independent).
- `-p N` deterministic per-N but ≠ single-core → **B-faithful** (gate vs Perl `--hisat2 -p N` per N).
- `-p N` non-deterministic run-to-run → no clean gate (escalate/defer).

**Scope (throwaway):** Perl-only (the oracle drives HISAT2 exactly as the Rust port would → answers the
gate question without a Rust build); SE directional; 1M real GRCh38 WGBS SE reads (the documented scale
where the fork-variance shows, 1310 vs 1219); HISAT2 2.2.2. Out of scope: PE/non-dir/pbat/FastA (Phase-1
gate matrix); the Rust↔Perl `-p N` byte-identity (the Phase-1 gate); `--multicore` repeated ×N (1 run — see Limitations).

## 2. Script + how to run

`spike_hisat2_p_determinism.sh` (this dir). Self-contained bash; subsamples 1M reads, runs Perl
`bismark --hisat2` in 9 configs (single-core ×2, `-p {2,4,8}` ×2 each, `--multicore 4` ×1), compares
decompressed-BAM bodies (in-order md5, sorted/content md5, spliced N-CIGAR subset count+md5), prints a verdict block.
Run on oxy (HISAT2 2.2.2 + Perl Bismark v0.25.1 in `~/micromamba/envs/bismark-test/bin`, genome
`~/bismark_benchmarks/genome`):
```
dcli ssh oxy 'cat > /var/tmp/spike_hisat2_p.sh' < spike_hisat2_p_determinism.sh
dcli ssh oxy 'setsid nohup bash /var/tmp/spike_hisat2_p.sh > /var/tmp/spike_hisat2_p.out 2>&1 < /dev/null &'
```
Raw output: `spike_run.out` (this dir) + durable oxy copy `~/hisat2mc_spike_artifacts/`.

## 3. Results (1M reads, GRCh38 SE directional, HISAT2 2.2.2)

| config | records | spliced (N-CIGAR) | run-to-run determinism | == single-core (content)? |
|--------|---------|-------------------|------------------------|---------------------------|
| **single-core** (no `-p`) ×2 | 844,267 | **1310** | ✅ YES (a==b, byte-identical) | — (reference) |
| `-p 2` ×2 | 844,296 (+29) | 1307 (−3) | ✅ YES (a==b) | ❌ NO (in-order, sorted, AND spliced subset) |
| `-p 4` ×2 | 844,305 (+38) | 1303 (−7) | ✅ YES (a==b) | ❌ NO |
| `-p 8` ×2 | 844,316 (+49) | 1298 (−12) | ✅ YES (a==b) | ❌ NO |
| `--multicore 4` ×1 | 844,296 | **1237** (−73) | (1 run) | ❌ NO (worker-variance, as expected) |

**Timings (wall, 1M, 2 strand-instances run concurrently):** single-core 2:05 · `-p 2` 1:48 · `-p 4` 1:38 · `-p 8` 1:36 · `--multicore 4` **1:14**.

## 4. Findings

1. **🔴 B-strong is DEAD: HISAT2 `-p N` is NOT content-identical to single-core.** The difference is
   not merely output order (`--reorder` is on) — the **sorted/content md5 differs**, the **record count
   changes** (844,267 → 844,316 as N grows), and the **spliced subset changes** (1310 → 1298). HISAT2's
   *threading itself* perturbs the alignments, even in a single instance over the whole read set. The
   plan's premise ("single instance ⇒ whole-read-set splice discovery ⇒ == single-core") is **falsified**.
2. **✅ B-faithful is VIABLE: `-p N` is deterministic run-to-run.** Every N's two repeats are
   **byte-identical even in-order** (a==b). So Rust `--hisat2 --multicore N` → single-instance
   `-p N --reorder` can be **byte-identical to Perl `--hisat2 -p N`** for a fixed N — a clean faithful
   gate. Output is **N-dependent** (≠ single-core, ≠ other N), so under methylseq's auto-derived
   `--multicore = cpus/3` the methylation calls are **node-size-dependent** — *not* the node-independence
   the rev-1 plan hoped for, but deterministic and reproducible per N.
3. **Mechanism (confirms reviewer B's hypothesis #3/#4):** the monotonic trend — more parallelism →
   more reads aligned, **fewer spliced** (single-core 1310 → `-p 8` 1298 → fork `--multicore 4` 1237) —
   is consistent with HISAT2 building a **dynamic splice-site database as it processes reads, in
   thread/partition order**. Single-thread sees all prior reads' sites (most splices); `-p N` threads
   see fewer (slightly fewer splices); N forked chunks each see only 1/N the reads (fewest splices). So
   the fork model's drift IS thread-/partition-sensitivity, and **single-instance `-p N` shares it** — just
   less severely. This is why `-p N` lands *between* single-core and the fork model.
4. **`--multicore 4` (fork) is the FASTEST multicore option (1:14) but the most variant (1237 spliced).**
   `-p`-threading has diminishing returns (Perl warns at `-p>4`, `bismark:7996`); the fork model
   parallelizes the 2 strand-instances × N chunks more aggressively.
5. **Worker-variance of the fork path confirmed** (`--multicore 4` ≠ single-core), reproducing the
   rev-0 `config.rs:243-253` evidence — so the reject's rationale is sound.

## 5. Implications for the plan (carry to the decision)

- **No multicore HISAT2 mode (A or B) is node-independent.** The spike kills that hope for *both*.
  Only the single-core path (the shipped stop-gap) is node-independent (1310, fully reproducible).
- **B collapses to B-faithful only** (gate = byte-identical to Perl `--hisat2 -p N` per N), reusing the
  existing `-p`/`--reorder` plumbing. Pros vs A: a **clean faithful gate** (single instance — no
  contiguous-vs-modulo chunking-model mismatch with Perl, which would make A's byte-match fragile — see
  §7), **less worker-variance** (closer to the single-core reference), and **lower memory** (1 HISAT2
  instance, not N). Cons: **slower** than the fork model, **N-dependent** output, and a **semantic remap**
  (`--multicore`→`-p` for HISAT2).
- This **materially changes the basis on which Approach B was chosen** (it was picked believing it was
  node-independent == single-core). → **escalate to Felix before writing the implementation plan**
  (B-faithful vs reconsider A vs keep the stop-gap). See the plan's `## Spike Results`.

## 6. Reference snippets (carry to implementation, if B-faithful proceeds)

- **The exact `-p N` HISAT2 invocation Bismark emits** (from a `-p 8` run.log, CT instance):
  `hisat2 -q --score-min L,0,-0.2 -p 8 --reorder --ignore-quals --no-softclip --omit-sec-seq --norc -x <BS_CT> -U <C_to_T.fastq>`
  → Rust route: single instance, inject `-p N --reorder` into `aligner_options` (already plumbed,
  `options.rs:149`); force the single-instance direct path (`config.multicore=1`, carry N as `-p` — see PLAN rev-2 Phase 1).
- **Gate = Perl `--hisat2 -p N`** (NOT single-core, NOT `--multicore N`), per matching N; deterministic so byte-identity is achievable.

## 7. Limitations

- **`--multicore N` (fork) reproducibility not verified** — ran once. Relevant only if Approach A is
  reconsidered: Rust's Phase-9b chunking is **contiguous**, Perl's `--multicore` is **modulo** → for the
  read-set-sensitive HISAT2 these would discover *different* per-chunk splices → Rust contiguous
  `--multicore N` would **NOT** be byte-identical to Perl modulo `--multicore N` without replicating
  Perl's exact modulo assignment. (For Bowtie 2 this is moot — read-independent. For HISAT2 it makes A
  fragile/expensive.) If A is reconsidered, a follow-up spike must verify Perl `--multicore N`
  run-to-run reproducibility AND the chunking-model match.
- SE directional, 1M, one dataset, HISAT2 2.2.2 only. The determinism + ≠-single-core result is clear
  and library-independent in mechanism, but the Phase-1 gate (if B-faithful proceeds) must still run the
  full SE+PE × {dir,non-dir,pbat} × {FastQ,FastA} + `--ambig_bam` matrix vs Perl `-p N`.
- Did not build/run the Rust port — Rust↔Perl `-p N` byte-identity is the Phase-1 gate, not this spike
  (the plumbing exists; the Phase-2a/2b HISAT2 gates proved Rust↔Perl parity for the no-`-p` path).
