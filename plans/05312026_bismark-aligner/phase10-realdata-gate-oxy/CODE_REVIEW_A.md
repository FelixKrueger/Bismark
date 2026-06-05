# Code Review A — Phase 10 full-scale gate harnesses

**Reviewer:** A (independent) · **Date:** 2026-06-04
**Targets:**
- `phase10_subset_strict_gate.sh` (Gate A — 10M strict + worker-invariance + A-assumption)
- `phase10_fullscale_content_gate.sh` (Gate B — full-scale content multiset + report + count reconciliation + RNAME-set + aux + perf + V13)

**Context (ground truth, not reviewed):** `PLAN.md` (rev 2), `GATE_OXY.md` (verdict PASS, all 4 cells), `phase9b_worker_invariance_gate.sh` (precedent).

---

## Summary

Both harnesses are **fundamentally sound as gate scripts** and the reported PASS is **trustworthy** for the cells/inputs actually exercised. The core soundness properties hold:

- **Canonicalization is airtight.** Both scripts `export LC_ALL=C` at the top (A:30, B:34), so *every* `sort`/`cmp`/`md5sum`/`grep`/`comm`/`cut` inherits a byte-wise total order — the "two independent sorts equal iff multisets equal" guarantee is satisfied. Gate B's inline `LC_ALL=C` prefixes are redundant-but-harmless belt-and-suspenders.
- **The unit of comparison is the full record line.** `samtools view` (no `-h`) emits one complete SAM record (all columns + tags) per line; default `sort` keys on the whole line; `md5sum` over the sorted stream is collision-resistant. Two genuinely different multisets cannot collide.
- **The B1.5 count guard fires before any hash** (B:131–146) and the *essential* `perl view -c == rust view -c` sub-check is independent of the (previously buggy) implied-count line. The discard-subtraction fix (`implied = (ubh − disc) × mult`) is **correct** — I verified the grep targets against `bismark` source: only the two report lines containing the literal substring "unique best hit" are SE `Number of alignments with a unique best hit from…` and PE `Number of paired-end alignments with a unique best hit:`; the "unique best **(first)**" lines do not contain "unique best hit" and won't be matched. `grep -m1` + `grep -oE '[0-9]+' | head -1`/`tail -1` extract the single count on each label line (labels carry no embedded digits). Robust.
- **FastQ aux is record-ized (`paste - - - -`) before any content sort** (A:102, B:180–181) and the in-order Gate A path uses plain `cmp` on `zcat` output (A:101,147–148). Correct.
- **`partner()` ambig suffix-match is safe.** I tested: the normalized temp suffixes (`.cmp.`/`.srt.`/`.hdr.`/`.rec.`/`.f.`/`.txt.`) are all appended *after* `.bam`/`.fq.gz`/`.txt`, so a file like `x_bismark_bt2.ambig.bam.srt.assume` does **not** match the `*.ambig.bam` glob (doesn't end in `.ambig.bam`) and does **not** match `*.bam` either. No temp-file pollution of source globs. The single-core-vs-multicore `.ambig.bam` name divergence is matched correctly.
- **pbat R1↔R2 swap** (`--pbat -1 pe_2 -2 pe_1`) is consistently applied to *both* Perl and Rust on the *same* swapped inputs (A:203, B:219) — a valid same-input comparison; the GATE_OXY 24-record delta vs directional pe confirms it genuinely exercises CTOT/CTOB.
- **`FAILED` propagates correctly** — all comparators/`run_cell` run in the main shell (not `| while` subshells), so `FAILED=1` mutations stick and `exit $FAILED` is honest. No `set -e`, so unchecked `$?` after `local x=$(pipe)` does not abort — intentional.

No **Critical** issue (nothing that would let a wrong port pass within the gated cells, and nothing that invalidates the reported result). The findings below are hardening/robustness gaps that matter if the harness is re-run on different inputs or if an alignment partially fails — worth recording, not blocking the verdict.

---

## Issues by area

### Logic / validation sufficiency

**[High] Gate A has no count/`wc -l` backstop — a zero-record-but-exit-0 BAM on the *reference* side would silently pass.**
`compare_dirs` (A:122–154) drives entirely off globbing `"$ref"/*.bam`. If the **reference** (Perl single-core) run ever produced *no* main BAM yet exited 0, the `for rb` body never executes and **no comparison and no failure is recorded** for that artifact. The MISSING-partner guard (A:126) only fires when REF *has* the file and OTHER lacks it — it cannot catch REF itself being empty. Gate B closes this with B1.5 (`view -c` + `wc -l` + implied), but Gate A — the *strict byte-identity* gate, the strongest signal — has no equivalent record-count assertion. In this run it didn't bite (millions of records confirmed), but as a gate it relies on the oracle never silently under-producing. *Recommendation:* add a one-line `samtools view -c` assertion per cell in Gate A (ref count > 0 and ref==other) before the byte cmp, mirroring B1.5. Low effort, removes the only silent-pass surface in Gate A.

**[Medium] Gate B: `PB`/`RB` are used unguarded after the `ls | grep | head` glob (B:120–121, 132).**
If the alignment exits 0 but the main-BAM glob matches nothing, `PB`/`RB` become empty strings. `samtools view -c ""` then errors to stderr and `cP`/`cR` capture empty strings. I traced the consequence: with `cP=""`,`cR=""`,`implied="0"`, the guard `[ "$cP" = "$cR" ] && [ "$cP" = "$implied" ]` evaluates `(""="" → true) && (""="0" → false)` → **false → COUNT MISMATCH → FAILED=1**, so it *is* caught — but only incidentally (because implied≠""). It's fragile (relies on implied being non-empty) and the failure message would be confusing. *Recommendation:* assert `[ -n "$PB" ] && [ -f "$PB" ]` (and RB) right after B:121 with an explicit `FAILED=1; return` on miss.

**[Low] Gate A empty-glob silent-skip extends to reports/aux too** (A:137,143). Same mechanism as the High item, lower stakes (reports/aux are corroborating, not the primary signal). Covered by the same suggested fix.

### Efficiency / resource

**[Medium] Gate A's content sorts use `sort -S 25%` (A:98,102), contradicting Gate B's deliberate absolute `-S 16G`.**
The rev-2 PLAN note (and Gate B:59–63) explains that `-S 25%` sizes against the node's advertised ~991 G RAM (not the 256 G cgroup) → a ~248 G buffer reservation that risks an OOM-kill. Gate A escaped this only because it ran on the **10M subset** (small input → GNU `sort` allocates lazily, never approaching 248 G). But the Gate A harness is explicitly parameterized to also run **strict-full RRBS** (PLAN §3.2, O1). A full RRBS A-assumption sort (~55 M records, multi-GB) under `-S 25%` could OOM on the 256 G cgroup. This is a latent footgun the rev-2 correction fixed in Gate B but **not** in Gate A. *Recommendation:* make Gate A's `n_sam_sorted`/`n_fq_sorted` use the same absolute `-S 16G` (and a `-T "$BASE/sorttmp"`, already present).

**[Low] Unchecked `sort` exit in `$(sort … | md5sum …)` could feed truncated input to `md5sum` on an OOM-spill failure** (B:151–152,180–181,188–189,203; A:88 via files). Under `pipefail` the overall pipe rc is non-zero, but it's captured in `$( )` and never checked. A partial sort would yield a *different* md5 (caught as a mismatch) in the asymmetric case; the dangerous case is both sides failing identically to empty (`md5 = d41d8cd9…` on both → false PASS). Mitigated in practice by the B1.5 `wc -l` guard running first on the same body files and by both sides being same-size. Worth a comment or an explicit `set -o pipefail` rc check on the sort. Informational.

### Errors / robustness

**[Low] `rep` filename assumes Perl and Rust emit the identical report basename** (B:122, used as `$d/perl/$rep` and `$d/rust/$rep`). True for the faithful port (verified: bismark writes one final `*_report.txt` to `-o`; temp reports go to `--temp_dir`, not globbed). If the basenames ever diverged, `$d/rust/$rep` would be a missing file and `grep -v … > rust.rep` would create an empty file → B1 cmp fails loudly (not a silent pass). Acceptable; noted for completeness.

**[Low — intentional] V13 mismatch prints a NOTE, does not set `FAILED`** (B:204–205). This matches PLAN §3.1/§8 ("V13 adds no independent-correctness signal"). Correct by design; flagging only so the asymmetry vs every other check is on record.

### Structure / clarity

**[Low] Gate B `rname_md5vec` writes one file per RNAME via `awk '{ print >> (d"/"$3) }'`** (B:89–95). With ~180 human contigs this opens ~180 fds; awk handles this but on a genome with thousands of scaffolds (`>1024` open files) some awk builds error. Only on the **B2-mismatch diagnostic path** (never reached on PASS), so it cannot affect the verdict — diagnostic-only. Informational.

**[Low] Gate A `cmp_files` diagnostic** (A:79–82) handles an empty `$off` gracefully (tested: `ln` stays 1); the `diff <(sed …) <(sed …)` window is bounded by `head -14`. Diagnostic-only (FAILED already set). Fine.

---

## What I explicitly verified holds (anti-false-pass checks)

- `LC_ALL=C` reaches every comparison op in both scripts (via top-level `export`). ✔
- Comparison unit = full SAM record line / 4-line FastQ record / header line (`@PG` filtered). ✔
- B1.5 essential guard (`perl view -c == rust view -c`) is independent of the implied-count line and fires before B2 hashing; the discard-subtraction fix is arithmetically correct and the greps are robust against the report's other "unique best" lines. ✔
- `partner()` / `*.ambig.bam` suffix-match does not mis-match a normalized temp file and correctly bridges the single-core↔multicore ambig-name divergence. ✔
- FastQ aux record-ized before content sort; in-order path uses plain `cmp`. ✔
- pbat swap applied symmetrically to both sides; V13 correctly skipped for pbat/rrbs and for subset-smoke runs. ✔
- `FAILED` mutates in the main shell (no subshell traps); `exit $FAILED` is honest. ✔
- Normalized temp suffixes never re-pollute the `*.bam`/`*.fq.gz`/`*_report.txt` source globs across the 3 rotating `compare_dirs` calls. ✔

---

## Recommendations (prioritized)

1. **[High]** Add a `samtools view -c` count assertion to Gate A `compare_dirs` (ref > 0, ref==other) so an empty-but-exit-0 reference BAM cannot silently pass the strict gate. *(Hardening; did not affect this run.)*
2. **[Medium]** Guard `PB`/`RB` for non-empty/existing in Gate B (B:120–121) with an explicit fail+return, so the count guard's emptiness-catch isn't incidental.
3. **[Medium]** Change Gate A's `-S 25%` sorts to the same absolute `-S 16G` as Gate B, to make the parameterized strict-full RRBS path OOM-safe under the 256 G cgroup.
4. **[Low]** Check the `sort` rc inside the `$(… | md5sum)` substitutions (or comment why it's safe) to eliminate the both-sides-truncate-to-empty false-PASS corner.
5. **[Low]** Note in `rname_md5vec` that it is diagnostic-only and may hit awk fd limits on high-scaffold genomes.

**None of these change the Phase 10 verdict.** The gate as written and run is sufficient to trust the reported PASS for the four gated cells; the items above harden it against re-runs on different inputs or partial-failure scenarios.
