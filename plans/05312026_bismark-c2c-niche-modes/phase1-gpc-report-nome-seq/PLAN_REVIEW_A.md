# Phase 1 PLAN review — Reviewer A

**Plan:** `plans/05312026_bismark-c2c-niche-modes/phase1-gpc-report-nome-seq/PLAN.md` (rev 0)
**Reviewer:** A (independent; no shared state with Reviewer B)
**Method:** plan-reviewer SKILL.md methodology + **live-Perl-v0.25.1 verification** of every surprising claim on tiny fixtures (repo-root `./coverage2cytosine`, run sandbox-disabled). Recommend-only — no tracked file edited.

---

## TOP-LINE VERDICT: **APPROVE-WITH-CHANGES**

The plan is excellent: nearly every surprising assertion was independently reproduced against live Perl v0.25.1, the coordinate arithmetic is correct, the reuse of v1.0 infrastructure is sound, and the test matrix (V1–V17) covers the high-risk paths. I found **no Critical issues** and **no byte-identity divergence in any output stream**. The changes I request are two **Minor** faithfulness/clarity gaps (one STDERR-only error-precedence ordering; one filename-prose imprecision for the NOMe×split combination) plus two **Nits**. None block implementation; all are cheap to fold into rev 1.

### Live-Perl checks I ran (all PASS unless noted)

| Claim (plan §) | Fixture | Result |
|---|---|---|
| GpC report bytes + bottom-before-top order (§3, §3.3) | `AGCAGCGCATGCGGCATTAGCTAGC`, cov 2,3,6,7,8 | **Confirmed** — output `6 +`,`7 -`,`8 +`; within the pos-8 GC, bottom(7) precedes top(8). |
| `pos=j+2`, top@`pos` `+`, bottom@`pos-1` `-`, `tri_top=substr(pos-1,3)`, `tri_bot=revcomp(substr(pos-4,3))` (§3.3) | traced all 7 GC matches | **Confirmed** exactly. |
| GpC edge guards — first/last GC dropped, interior emits `4 - CTG`,`5 + CTT` (§3.6.1/.2) | `GCAGCTTAGC`, cov 1,2,4,5 | **Confirmed** — first GC drops (bottom tri `GC` len 2); last GC drops (top tri `C` len 1). |
| GCGC non-overlapping but consecutive (§3.6.3) | `ATGCGCGCAT`, cov 3–8 | **Confirmed** — matches at 0-based j=1,3,5 (resume at j+2); 6 report lines, none double-counted. |
| GpC has **no** `--zero_based` branch (§3.3, Assumption 2, V10) | `--gc --zero_based` vs `--gc` | **Confirmed** — GpC report+cov byte-identical; only the **core** report shifts (`5/6` vs `6/7`). |
| `--gc` core stays at threshold 0; GpC uses max(thr,1) (§3.1 step 4, V5) | plain vs `--gc` core diff | **Confirmed identical**; GpC has 3 lines (uncovered dropped) vs core 4 (uncovered all-zero kept). |
| GpC = covered chromosomes only, no uncovered pass — split **and** non-split (§3.3, V14) | 3-chr genome, cov chr1 only | **Confirmed** — split writes only `s.chrchr1.GpC*`; non-split GpC = chr1 only; core writes all genome chrs at thr 0. |
| Raw-`-o` doubling (§3.4, V15) | `-o foo.CpG_report.txt --gc` | **Confirmed** — `foo.CpG_report.txt.GpC_report.txt` + `.GpC.cov`; core `foo.CpG_report.txt`; summary `foo.cytosine_context_summary.txt`. |
| `--gzip` → GpC `.gz`, summary plain, decompressed identical (V8) | `--gc --gzip` | **Confirmed**. |
| `--split` per-chr GpC files `*.chrchr1.GpC*`, byte-identical content (V9) | 2-chr cov | **Confirmed**; single-chr split (last-chr inline-reopen path :1043/:1055) also correct. |
| NOMe files + summary keeps non-NOMe base (§3.2.3, V13) | `--nome-seq` on primary fixture | **Confirmed** — `*.NOMe.{CpG,GpC}_report.txt` + `*.NOMe.{CpG,GpC}.cov` + `*.cytosine_context_summary.txt` (no `.NOMe`). |
| NOMe GpC drops CG-context GpC, keeps CHH (§3, V12) | same | **Confirmed** — only `8 + CHH CAT` survives. |
| Non-NOMe writes **no** core `.cov` (§3.2.2) | plain + `--gc` runs | **Confirmed** — only `.GpC.cov`, never a core `.cov`/`.CpG.cov`. |
| NOMe ACG/TCG **upstream** filter, both strands (§3.2.1, V11) | `TTACGTTAGCATCGTT`, cov 4,5,13 | **Confirmed** — keeps 4(+ ACG),5(- revcomp ACG),13(+ TCG); `.NOMe.CpG.cov` companion with `%.6f`. |
| CLI: `--nome-seq --CX` die / `--nome-seq --merge_CpGs` die / `--coverage_threshold 0` die / `5` honored (§3.1, V2) | dedicated runs | **Confirmed** all four. |
| NOMe summary == plain(threshold 1) summary (ACG/TCG filter is *after* `context_reporting`) | `TTACG…` | **Confirmed identical** (relevant to the Assumption-5 nit below). |

---

## Findings by severity

### Critical
**None.** No claim in the plan was contradicted by live Perl; no output-byte divergence found.

### Important
**None.**

### Minor

**M1 — CLI error precedence: the NOMe mutex checks must be inserted *before* `MergeCpgsWithThreshold`, not "after the merge-mutex checks" (plan §3.1 step 2; §5 step 1).**
*Issue.* The plan says "after the existing … merge-mutex checks, add the NOMe block." The current Rust `validate()` (`cli.rs:168-176`) runs three merge mutexes in this order: `MergeCpgsWithCx` (168) → `MergeCpgsWithSplit` (171) → `MergeCpgsWithThreshold` (174). Inserting the NOMe block *after* line 176 puts `MergeCpgsWithThreshold` *ahead* of `NomeWithMerge`. But in Perl the merge+threshold die lives in the **threshold block** (`:2174-2176`), which runs **after** the NOMe block (`:2147-2161`). I verified the precedence on live Perl:
- `--nome-seq --merge_CpGs --coverage_threshold 5` → Perl dies with **"NOMe-Seq filtering does not work in conjunction with --merge_CpG"** (the `:2149` die), *not* the threshold-merge die.
- `--nome-seq --merge_CpGs --CX` → Perl dies with the **merge+CX** message (`:2140`, which *is* before NOMe).

So the faithful Rust order is: `MergeCpgsWithCx` → `MergeCpgsWithSplit` → **`NomeWithCx` → `NomeWithMerge`** → `MergeCpgsWithThreshold` → discordance → `threshold==Some(0)`.
*Why it matters.* These are all `die`/`Err` paths and the message text is **STDERR-only (exempt)**, so this is **not a byte-identity violation** — both Rust and Perl reject the run. But the plan's stated insertion point would emit a *different* typed error than Perl for the `--nome-seq --merge_CpGs --coverage_threshold` combo, and V2 asserts the typed errors, so the implementer should know the exact slot.
*Fix.* Re-word §3.1 step 2 / §5 step 1 to insert `NomeWithCx`/`NomeWithMerge` **between `MergeCpgsWithSplit` and `MergeCpgsWithThreshold`** (i.e. move the existing `MergeCpgsWithThreshold` check to *after* the NOMe block), mirroring Perl `:2147` preceding `:2174`. Optionally add a V2 sub-case pinning `--nome-seq --merge_CpGs --coverage_threshold 5` → `NomeWithMerge`.

**M2 — The NOMe core report filename in `--split_by_chromosome` mode is `{raw}.chr{name}.NOMe.CpG_report.txt`, not `{stem}.NOMe.CpG_report.txt` (plan §3.2.3 prose; §4 `report.rs` signature note).**
*Issue.* §3.2.3 and the §4 signature sketch describe the NOMe core filename only as `{stem}.NOMe.CpG_report.txt` / `{stem}.NOMe.CpG.cov`. That is the **non-split** form. In split mode, Perl `handle_filehandles` appends `.chr${my_chr}` to the **raw** `$cytosine_out` *before* the suffix strip (`:101`) and *before* the `.NOMe` suffix (`:121-122`). I confirmed live Perl emits `s.chrchr1.NOMe.CpG_report.txt` + `s.chrchr1.NOMe.CpG.cov` for `--nome-seq --split_by_chromosome`. The existing `report_name` already encodes the right base logic (raw+`.chr` for split, stem for non-split — `report.rs:430-433`), so the implementer must thread the **NOMe variant through the same base derivation**, not hard-code `{stem}`.
*Why it matters.* If the implementer literally builds `{output_stem}.NOMe.CpG_report.txt` in split mode, the filename would be wrong (missing the `.chrchr1` infix / wrong base) — a byte-identity miss on *file existence/naming* for `--nome-seq --split`.
*Fix.* In §3.2.3 / §4, state the NOMe core filename as: split → `{raw}.chr{name}.NOMe.CpG_report.txt[.gz]`; non-split → `{stem}.NOMe.CpG_report.txt[.gz]` (same `.cov` pairing). Reuse the existing `report_name` base-selection. Add a golden cell to V14 (or a new V) pinning the **NOMe×split filenames + bytes** (the current V14 fixture is single-cov-chr, non-split-specific; it does not pin the NOMe split filename). *(Note: `--nome-seq ✗ --merge_CpGs` but NOMe **does** compose with `--split` — confirmed running cleanly — so this combination is reachable and untested in the matrix.)*

### Nit

**N1 — Assumption 5 over-states the summary as "unchanged by `--gc`/NOMe".**
The context summary accumulation is gated by the threshold guard (Rust `emit_position:188` / Perl `:594`), which precedes `context_reporting` (`:624`). For `--gc` (no NOMe, threshold stays 0) the summary is indeed identical to a plain run. But `--nome-seq` forces `threshold=1`, so uncovered (0,0) positions are **excluded** from the summary — it differs from a plain *threshold-0* summary (though it equals a plain *threshold-1* summary, which I verified). This is **not a code bug** (the v1.0 summary code already gates on `config.threshold`, which the plan resolves to 1 under NOMe, so Perl parity holds automatically) — only the prose and the V13 expectation ("content == the all-context summary") are imprecise. *Fix:* re-phrase to "content == the summary at the resolved coverage threshold (=1 under NOMe)"; no implementation change.

**N2 — Single-kernel collapse: note the covered-chr vs last-chr block *reorder* explicitly.**
The two Perl GpC blocks differ in the **order** of coverage-lookup vs context-classification (covered-chr `:875-914` looks up coverage *before* classifying; last-chr `:992-1031` classifies *before* looking up). Both are side-effect-free skip-guards, so output is identical and a single kernel is correct — but §3.3/§11 assert the collapse without calling out the reorder. Worth one sentence so the implementer/reviewer doesn't think the blocks are line-for-line identical (they are not — only output-equivalent). No change to logic or tests required.

---

## Validation-sufficiency assessment

V1–V17 cover the highest-risk paths well: the GpC coordinate arithmetic (V4/V6/V7 against live Perl), the no-`--zero_based` asymmetry (V10), the threshold split (V5), the ACG/TCG filter (V11/V12), gzip/split invariance (V8/V9), raw-`-o` doubling (V15), and the div-by-zero guard (V16). **Gaps the changes above close:** (a) no cell pins the **NOMe×split** filename+bytes (M2); (b) no cell pins the **NOMe+merge+threshold** error precedence (M1). Both are cheap additions. The two "Open (non-critical)" questions (FFS composition; the split-mode `GCCOV`-not-closed quirk) are correctly classified as non-Critical — I confirmed the fresh-writer-per-chr approach yields byte-identical per-chr `.GpC.cov` files in multi-chr split, sidestepping the Perl quirk with no output consequence.

## Efficiency / alternatives
No concerns. The second genome pass + second cov read is Perl-faithful and necessary for byte identity; the genome is shared by reference; per-chr cov buffering is freed on transition. A parallel walk is correctly out of scope (epic §2). No alternative would preserve byte identity more cheaply.

---

## Summary for orchestrator

**VERDICT: APPROVE-WITH-CHANGES.** Critical 0 · Important 0 · Minor 2 · Nit 2. Every surprising claim independently confirmed against live Perl v0.25.1 (coordinate model, bottom-before-top, no-`--zero_based`, threshold split, raw-`-o` doubling, ACG/TCG upstream filter, NOMe `.cov` companion, no-uncovered-pass, mutexes). No byte-identity divergence found.
- **M1 (Minor, §3.1 step 2 / §5 step 1):** insert the `NomeWithCx`/`NomeWithMerge` checks *between* `MergeCpgsWithSplit` and `MergeCpgsWithThreshold` (Perl `:2147` precedes the threshold block `:2174`), not after all merge mutexes — else `--nome-seq --merge_CpGs --coverage_threshold` emits the wrong typed error vs Perl (STDERR-exempt, both reject; affects V2).
- **M2 (Minor, §3.2.3 / §4):** NOMe core filename in split mode is `{raw}.chr{name}.NOMe.CpG_report.txt` (reuse `report_name`'s base selection), not `{stem}.NOMe...`; `--nome-seq` composes with `--split` (verified) but the matrix never pins the NOMe×split filename/bytes — add a golden.
