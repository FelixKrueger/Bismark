# IMPL (TDD) — HISAT2 `--local` alignment (byte-identical to Perl `--hisat2 --local`)

**Source plan:** `PLAN.md` rev 2 (dual-reviewed; the Critical = `score_min_params` is the load-bearing MAPQ edit, not `calc_mapq`).
**Crate:** `rust/bismark-aligner` · **Base:** fresh worktree off **`origin/rust/iron-chancellor` (beta.9, `478c974`)** — NOT the current `Bismark-hisat2mc` worktree HEAD (stale local beta.8 bump; #986 is already upstream as `4b93f5b`).
**Goal (one line):** un-reject `--hisat2 --local`, emit Perl's HISAT2-local option delta (L-form `--score-min`, drop `--no-softclip`, no `--local`) + the local `ln()` MAPQ with `(0,−0.2)`; byte-identical to Perl v0.25.1 `--hisat2 --local`, SE+PE × dir/non-dir/pbat. minimap2-`--local` stays rejected.
**Mode:** TDD (Rust `cargo test`). **Status:** plan only — **dual-reviewed ✅** (`IMPL_REVIEW_A/B.md`, both APPROVE-WITH-CHANGES, folded below) — awaiting implement trigger.

## IMPL-review delta (folded)
- 🔴 **A1 (Task 1 RED was unachievable):** after the reject lifts, `resolve(--hisat2 --local …)` with no genome returns a *different* pre-I/O `Err("No genome folder specified")`, never `Ok`. RED must assert **"NOT the local-reject error"** (Ok OR a non-local-reject Err), not `Ok`. Production code unaffected.
- **A4+B1 (Task 4 was vacuous):** `(0,−0.2)` → sub-unity `diff = 0.2·ln(readLen)` (0.74–1.00); the **no-second-best** SE ladder only reaches buckets 44/22 there, so the `ln()`-sensitive interior is UNREACHABLE. Mandate the `ln()`-ULP-sensitive paths: the **`best_over == diff` exact-equality leaf** (AS_best=0 + a second-best → buckets 35/34/33/32/31) AND a **PE summed-`ln()`** intermediate-bucket case; Perl-cross-checked; SE+PE.
- **B2+B3 (gate non-vacuity):** `S`-count > 0 proves Perl-local-identity but not that dropping `--no-softclip` *did* anything → add a same-dataset **`--hisat2` vs `--hisat2 --local` cross-check** (local soft-clip set non-empty AND ≠ end-to-end ~0); name a concrete soft-clip-*inducing* dataset + fallback up front.
- **B4 (base/version):** branch off **`origin/rust/iron-chancellor` (beta.9, `478c974`)**, NOT the worktree HEAD (`6260a1c` = stale local beta.8 bump atop the already-upstream-squashed `4b93f5b`). Next beta = **beta.10**.
- **A2/A3/B5:** minimap2 reject test asserts the message `.contains("local by design")`; add **`config.rs:291-294`** (reject-block comment) to the docs-flip set; Task 3 must keep the byte-frozen option-string tests green (`options.rs:478,497,555,607,776`).
- **Minor:** Task 5 CIGAR is **`2S4M`** (8 bp test genome), not `2S62M`.

## What the dual review settled (the contract this implements)
- 🔴 The MAPQ fix is in **`score_min_params` (`options.rs:347`)** — it hardcodes G-form `(20,8)` on `cli.local` alone; `calc_mapq` is correct/sign-agnostic and unchanged.
- Soft-clip non-vacuity must be **enforced** (soft-clip-prone reads + blocking `S`-count > 0), else the gate passes vacuously.
- The `(0,−0.2)` sub-unity-`diff` MAPQ regime needs a **Perl-cross-checked** unit test (the existing test is `(20,8)`-only).
- `--non_bs_mm` is globally rejected → no mutex to handle. Hard-clip/supplementary orthogonal (Assumption 6). Report echo auto-flows.

## Key seams (verified against current source)
| Edit | Location | Change |
|------|----------|--------|
| Reject lift | `config.rs:295` | `aligner != Bowtie2` → `aligner == Minimap2`; minimap2 msg states "local by design" |
| MAPQ params (🔴) | `options.rs:347` `score_min_params` | add `aligner`; G-form `(20,8)` only for `local && Bowtie2`, else L-form `(0,−0.2)` |
| call site | `config.rs:~360` | `score_min_params(cli, aligner)` |
| local opts block | `options.rs:82-103` | aligner branch: Bowtie2 = `--local` + G-form (current); HISAT2 = L-form, **no `--local`** |
| HISAT2 softclip tail | `options.rs:328` | `cli.local` → `--omit-sec-seq` only; else `--no-softclip --omit-sec-seq` |
| amend reject test | `config.rs:1060` | `resolve_rejects_local_with_non_bowtie2` → HISAT2 OK / minimap2 Err |
| update opts test | `options.rs:516` | `score_min_params_local_defaults_and_parses_g` (signature + add aligner) |
| docs | `README.md:61-62`, `cli.rs:169`, `config.rs:178` | flip "HISAT2-local unsupported" → supported; aligner-conditional help/doc |

## Plan coverage checklist
| # | Plan item | Task |
|---|-----------|------|
| 1 | Reject lift (Minimap2-only) + minimap2 "local by design" msg | Task 1 |
| 2 | Amend reject test (HISAT2 OK / minimap2 Err) | Task 1 |
| 3 | `score_min_params(cli, aligner)` G/L branch (🔴 Critical) | Task 2 |
| 4 | Update `score_min_params` test (signature + L-form HISAT2-local) | Task 2 |
| 5 | options local block: HISAT2 L-form, no `--local` | Task 3 |
| 6 | HISAT2 softclip tail: drop `--no-softclip` for local | Task 3 |
| 7 | Mandatory `(0,−0.2)` Perl-cross-checked MAPQ test (sub-unity diff) | Task 4 |
| 8 | e2e fake-HISAT2-local soft-clip round-trip (SE + PE) | Task 5 |
| 9 | Docs flips (README/cli/config) | Task 6 |
| 10 | oxy gate SE+PE × dir/non-dir/pbat vs Perl `--hisat2 --local` + soft-clip non-vacuity + `--multicore` cell | Final |
| 11 | `--non_bs_mm` (no action — globally rejected) / hard-clip orthogonal (no action) | — (documented no-ops) |

## Test infrastructure
- Unit: `config.rs` + `options.rs` + `mapq.rs` `#[cfg(test)]` (`cli_from` helper; fixture-free).
- e2e: `tests/cli.rs` — extend the fake-HISAT2 harness (`make_fake_hisat2_mapped`) with a **soft-clip-emitting** variant (CIGAR with `S`, e.g. `2S4M`) to exercise the local soft-clip path.
- Runner: `cargo test -p bismark-aligner -- --test-threads=2`; gates `cargo clippy -p bismark-aligner --all-targets -- -D warnings` + `cargo fmt -p bismark-aligner -- --check`.

---

## Task 1 — Lift the `--local` reject to minimap2-only (+ "local by design" msg)
**Files:** `src/config.rs:295` (reject) + `:1060` test (amend) + the doc comments `:178` AND `:291-294` (reject-block comment).
- **RED (A1 — corrected):** amend `resolve_rejects_local_with_non_bowtie2` → for `--hisat2 --local`, assert the result is **NOT the local-reject error** (after the lift, resolve falls through to a *different* pre-I/O `Err("No genome folder specified")` with the fixture-free `cli_from` args — so assert `!err.contains("only supported with Bowtie 2")` / not-the-`--local`-message, accepting Ok OR a non-local Err; do NOT assert `Ok`). For `--minimap2 --local`, assert **Err** whose message **`.contains("local by design")`** (A2 — pin the message, not just `is_err`). Run → fails (HISAT2 still rejected).
- **GREEN:** change the gate to `if cli.local && aligner == Aligner::Minimap2 { Err(Unsupported("--local is not supported with --minimap2: minimap2 performs local (soft-clipping) alignment by design — there is no end-to-end vs local distinction to toggle. Use --bowtie2 or --hisat2 for --local.")) }`. Update the `:178`/`:292-294` doc comments.
- **Verify:** `cargo test -p bismark-aligner config:: -- --test-threads=2`.

## Task 2 — 🔴 `score_min_params(cli, aligner)` G/L branch (the MAPQ fix)
**Files:** `src/options.rs:347` (signature + branch) + `:516` test (update) + `src/config.rs:~360` (call site).
- **RED:** update `score_min_params_local_defaults_and_parses_g` to the new signature + add cases: `score_min_params(&cli_local, Bowtie2)` → `(20.0, 8.0)` (G-form, accepts `G,…`, rejects `L,…`); **`score_min_params(&cli_local, Hisat2)` → `(0.0, −0.2)`** (L-form, accepts `L,…`, rejects `G,…`); end-to-end (any aligner) → `(0.0, −0.2)`. Won't compile (old signature) → RED.
- **GREEN:** `pub fn score_min_params(cli: &Cli, aligner: Aligner) -> Result<(f64, f64)>` with `let (prefix, default) = if cli.local && aligner == Aligner::Bowtie2 { ("G,", (20.0, 8.0)) } else { ("L,", (0.0, -0.2)) };` (rest unchanged). Update the call site `config.rs:~360` to pass `aligner`. Update the docstring (`:343-346`) — G-form is `local && Bowtie2`, not all `local`.
- **Verify:** `cargo test -p bismark-aligner options::tests::score_min -- --test-threads=2`.

## Task 3 — HISAT2-local option delta (L-form score-min, no `--local`, drop `--no-softclip`)
**Files:** `src/options.rs:82-103` (local block) + `:307-328` (HISAT2 tail).
- **RED:** options-string unit tests: `build_aligner_options(&cli_local, Hisat2, FastQ, false, None)` →
  contains `--score-min L,0,-0.2` + `--omit-sec-seq`, and **does NOT** contain `--local` or `--no-softclip`;
  a user `--score_min L,0,-0.6` is accepted (L-form), `G,20,8` rejected; **Bowtie2-local unchanged**
  (`--local --score-min G,20,8`, byte-frozen). PE variant too.
- **GREEN:**
  - Local block (`:82`): replace `debug_assert_eq!(aligner, Bowtie2)` with a match — Bowtie 2: push `--local`
    + G-form (current, byte-frozen); HISAT2: push L-form `--score-min` (validate `valid_score_min_l`, default
    `L,0,-0.2`), **do not** push `--local`; Minimap2: `unreachable!` (rejected at resolve).
  - HISAT2 tail (`:327-328`): `if cli.local { tail.push("--omit-sec-seq".into()) } else { tail.push("--no-softclip --omit-sec-seq".into()) }`.
- **Verify:** full options test module green; **regression (B5) — the byte-frozen Bowtie 2-local + HISAT2
  end-to-end option-string tests must stay green** (`options.rs:478,497,555,607,776`): Bowtie 2-local still
  `--local --score-min G,20,8`, HISAT2 end-to-end still `--no-softclip --omit-sec-seq`.

## Task 4 — Mandatory `(0,−0.2)` Perl-cross-checked MAPQ test — TARGET the `ln()`-sensitive buckets
**Files:** `src/mapq.rs` `#[cfg(test)]` (no production change — `calc_mapq` is sign-agnostic).
⚠️ **A4+B1:** at `(0,−0.2)`, `diff = 0.2·ln(readLen)` is sub-unity (0.74–1.00). The **no-second-best** SE ladder
only reaches buckets 44 (AS_best≥0) / 22 (AS_best<0) at these `diff` values — the interior `ln()`-sensitive
buckets are UNREACHABLE there, so an `AS_best ∈ {0,−1}` sweep would be **vacuous**. The genuinely ULP-sensitive
paths to assert:
- **SE second-best `best_over == diff` exact-equality leaf** (`AS_best = 0` with a defined second-best →
  buckets 35/34/33/32/31, keyed on `bestDiff = |AS_best|−|AS_secBest|` vs `diff·{0.9..0.1}`) — this is where a
  1-ULP `ln()` wobble in `diff` flips a bucket.
- **PE summed-`ln()` case:** `scMin = (0 + −0.2·ln(l1)) + (0 + −0.2·ln(l2))` (`mapq.rs:35-37`, `bismark:3935`)
  with a second-best, landing on an interior bucket boundary.
- Sweep read lengths (e.g. 40/50/75/100/150) and assert buckets against the **Perl ladder arithmetic**
  (`bismark:3932`/`3935`/`4082-4178`) transcribed as literals (or a tiny embedded oracle) — NOT self-consistency.
- **Verify:** `cargo test -p bismark-aligner mapq:: -- --test-threads=2`.

## Task 5 — e2e: fake-HISAT2-local soft-clip round-trip (SE + PE)
**Files:** `tests/cli.rs` (+ a soft-clip-emitting fake HISAT2).
- Add a fake `hisat2` variant that emits a **soft-clipped** CIGAR (`2S4M` — the 8 bp test genome, matching
  `make_genome_ht2`/`cli.rs:1967`; NOT `2S62M`) for `--local` runs; assert
  `--hisat2 --local` runs (exit 0), the report's `aligner_options` line shows `--score-min L,0,-0.2
  --omit-sec-seq` (no `--local`, no `--no-softclip`), and the output BAM carries the `S` CIGAR (methylation
  call over the soft-clipped read succeeds — `methylation.rs:174` `S`-as-`I`). SE + PE.
- **Verify:** `cargo test -p bismark-aligner --test cli -- --test-threads=2`.

## Task 6 — Docs (flip the stale surfaces)
**Files (implementation-first):** `rust/README.md:61-62` (HISAT2-local unsupported → **supported**, byte-identical to Perl `--hisat2 --local`; minimap2-local stays rejected — **local by design**); `src/cli.rs:169` `--local` help (aligner-conditional); `src/config.rs:178` `score_min_local` doc (G-form is Bowtie 2-local, L-form `(0,−0.2)` HISAT2-local).
- **Verify:** `grep` the surfaces read correctly; no remaining "HISAT2 … local … not supported".

---

## Final verification
1. **Local:** `cargo test -p bismark-aligner -- --test-threads=2` (all green; **regression: Bowtie 2-local #981, HISAT2 end-to-end, single-core + multicore HISAT2, minimap2 untouched**) + clippy `-D warnings` + fmt `--check`.
2. **oxy byte-identity gate** (build on oxy from a fresh worktree off latest iron-chancellor; HISAT2 2.2.2 + Perl v0.25.1):
   - Oracle = **Perl `--hisat2 --local`**. Compare decompressed BAM (@PG-filtered) + report (wall-clock/version-filtered).
   - **Matrix:** SE + PE × {directional, non-directional, pbat}. **+ one `--hisat2 --local --multicore N`** cell (== Perl `--hisat2 --local -p N`) — proves local + the #986 remap compose.
   - **🔴 Non-vacuity (blocking, B2+B3):** byte-identity to Perl-local is necessary but not sufficient — it
     must also be shown that dropping `--no-softclip` *did something*. Two blocking asserts:
     (a) `samtools view <local.bam> | awk '$6 ~ /S/' | wc -l` **> 0** (the local run produces soft-clips);
     (b) a **same-dataset cross-check** — run `--hisat2` (end-to-end) on the identical reads and assert its
     `S`-count is ~0 AND its BAM **differs** from `--hisat2 --local` (proves the toggle changes alignments,
     not a no-op). **Concrete soft-clip-inducing dataset (decide up front, don't discover at gate time):**
     clean WGBS often yields ~0 soft-clips even in local mode → use reads with a few non-genomic bases at the
     ends (e.g. a lightly adapter-tailed / mismatched-tail subset, or longer reads spanning indels);
     **fallback:** if a real subset won't induce soft-clips, build a small synthetic soft-clip-prone FastQ.
     If `S`-count is 0, the gate **FAILS** (vacuous) — do not pass it.
   - **Q4 (blocking prereq):** Perl `--hisat2 --local` run twice = same md5 (determinism) before trusting the byte-gate.
   - Write `GATE_OXY.md`.
3. Dual `/code-reviewer` (Agent, fresh) + `/plan-manager` → COVERAGE COMPLETE.

## Commit plan
- One commit: `feat(aligner): HISAT2 --local mode (byte-identical to Perl --hisat2 --local)`.
- Stage: `src/{config.rs,options.rs,mapq.rs,cli.rs}`, `tests/cli.rs`, `rust/README.md`, + the plans/ artifacts.
- rust/README Milestones line added **at merge into iron-chancellor**. On merge: cut the next beta (beta.9) + bump methylseq pin (only on explicit go). Minimap2-local stays rejected — no methylseq impact (not in its surface).
