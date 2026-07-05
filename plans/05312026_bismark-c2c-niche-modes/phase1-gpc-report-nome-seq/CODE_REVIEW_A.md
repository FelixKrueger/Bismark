# Code Review A — Phase 1 (GpC report `--gc`/`--gc_context` + NOMe-Seq `--nome-seq`)

**Reviewer:** Code Reviewer A (independent; fresh context; recommend-only).
**Target:** working-tree (uncommitted) changes in `rust/bismark-coverage2cytosine` on branch `rust/c2c-v1x` (worktree `/Users/fkrueger/Github/Bismark-c2c`).
**Oracle:** repo-root `./coverage2cytosine` v0.25.1 (self-contained Perl).
**Date:** 2026-05-31.

## Top-line verdict: **APPROVE**

Zero Critical, zero High, zero Medium. Two Low (cosmetic) observations only. The implementation is byte-identical to live Perl v0.25.1 across every mode I tested — including a set of **fresh adversarial fixtures I authored independently of the committed goldens** (N-bases, GC at chromosome start, `GCGCGC` runs, near-end GC, asymmetric per-strand thresholds, cov-chr-not-in-genome, GC-less chromosomes, non-contiguous chromosome re-appearance). The documented rev-2 deviation (NOMe core `.cov` derives from the raw `-o`, not the stem) is correct and I re-verified it against live Perl. The `pct6` promotion is behaviour-preserving (all 10 Phase-D goldens still green).

`cargo test -p bismark-coverage2cytosine` = **131 passed / 0 failed** (80 lib + 18 phase1 + 11 B + 7 C + 10 D + 5 sanity). `cargo clippy --all-targets -- -D warnings` clean. `cargo fmt --check` clean.

---

## Independent byte-identity verification (live Perl vs the Rust binary)

I did **not** rely on the committed goldens. I built my own fixtures and diffed `target/debug/coverage2cytosine_rs` against `perl ./coverage2cytosine`, decompressing `.gz` before comparing. All comparisons were on the **file set** (names) AND every file's bytes.

### Fixture 1 — adversarial single-chromosome genome
`>chr1  NGCGCGCAACGTTAGCATCGTTGCNN` (deliberately: leading N, an N-flanked GC at the end producing a `CNN`/CHH GpC context, a `GCGCGC` run, ACG/TCG-upstream CpGs at pos 10/19 and a CGA at pos 20, near-end GC). Coverage on 12 mixed positions. Modes diffed (all **byte-identical**, file-set + contents):

| Mode | Result |
|---|---|
| `--gc` | OK (incl. the `CNN` CHH GpC at pos 24, bottom-before-top order, `GCGCGC` non-overlapping) |
| `--gc --gzip` | OK (decompressed) |
| `--gc --split_by_chromosome` | OK |
| `--gc --zero_based` | OK (GpC frozen 1-based; core shifts — the §3.2/§3.3 asymmetry) |
| `--gc --coverage_threshold 2` | OK (core@2, GpC@max(2,1)=2) |
| `--gc --CX` | OK; GpC report **byte-identical with/without `--CX`** (confirms the GpC walk is CpG_only-independent, as the Perl has no `$CpG_only` branch) |
| `--nome-seq` | OK (NOMe core keeps only ACG/TCG-upstream CpGs 10/11/19/20; GpC drops CG-context, keeps CHH only) |
| `--nome-seq --zero_based` | OK (core report + `.NOMe.CpG.cov` point coord shift to pos-1; GpC frozen) |
| `--nome-seq --split_by_chromosome` | OK |
| `--nome-seq --gzip` | OK (decompressed) |
| `--nome-seq --coverage_threshold 5` | OK (explicit threshold honoured; pos 19 cov-4 dropped, 10/11/20 kept) |
| plain (no flags) | OK (no `.cov`/`.GpC.*`/`.NOMe.*` files — V17 regression) |

### Fixture 2 — per-strand asymmetric threshold (the highest-risk GpC arithmetic)
A `GC` dinucleotide whose top C (pos 5, cov 3) and bottom C (pos 6, cov 2) straddle the threshold: at `--coverage_threshold 2` **both** emit; at `3` **only the top** emits (bottom cov 2 < 3). Byte-identical at both thresholds → confirms the guard is **per-strand**, not whole-dinucleotide. (The two `len<3` + classify-both guards remain whole-dinucleotide; only coverage + NOMe-CG-skip are per-strand — exactly Perl `:917`/`:929`.)

### Fixture 3 — raw-`-o` filename divergence (the load-bearing rev-2 deviation)
- `-o foo.CpG_report.txt --nome-seq`: file set **byte-identical** to Perl —
  - core **report** `foo.NOMe.CpG_report.txt` (stem-stripped),
  - core **`.cov`** `foo.CpG_report.txt.NOMe.CpG.cov` (**raw, un-stripped**),
  - GpC `foo.CpG_report.txt.NOMe.GpC_report.txt` + `.GpC.cov` (raw),
  - summary `foo.cytosine_context_summary.txt` (stem).
  This is precisely the documented deviation, and it is **more faithful** to Perl than the plan's §3.2.3 prose (`handle_filehandles` only suffix-strips `$cytosine_report_file`, never `$cytosine_coverage_file` — Perl `:96-101,:104-112,:122`). Confirmed against live Perl. ✔
- `-o foo.CpG_report.txt --gc`: GpC files use the raw `-o` (`foo.CpG_report.txt.GpC_report.txt`). Byte-identical. ✔

### Fixture 4 — non-contiguous chromosome re-appearance (V18, no per-name caching)
Two-chromosome genome, cov ordered `chr1, chr2, chr1`. All four combos **byte-identical**:
- `--gc` single-file → the chr1 GpC block is **re-emitted** (a second chr1 segment appears after chr2).
- `--gc --split` → `sample.chrchr1.GpC_report.txt` is **re-truncated** to the LAST chr1 segment only.
- `--nome-seq` single and split → same contract, byte-identical.
This proves the implementation does not cache a per-name writer or buffer (the B-I1 trap).

### Fixture 5 — uncovered chromosome & cov-chr-not-in-genome
Genome `chrA`+`chrB`; cov on `chrA` only plus a `chrZ` absent from the genome. `--gc`, `--nome-seq`, plain all byte-identical:
- core report (plain & `--gc`) includes chrB's uncovered all-zero CpGs and excludes chrZ;
- the **GpC report shows chrA only** — no chrB (no uncovered pass) and no chrZ (empty walk over a missing sequence).

### Fixture 6 — GC-less chromosome in split mode
Genome `AATTAATT…` (no `GC`). `--gc --split` produces a byte-identical empty `s.chrchrX.GpC_report.txt` + `s.chrchrX.GpC.cov` (Perl opens the per-chr writers at the transition regardless of emitted lines). ✔

**Summary of independently re-verified byte-identity claims:** the GpC coordinate arithmetic (`pos=j+2`, top@`pos`, bottom@`pos-1`); the bottom-before-top emit order; both `len<3` guards + classify-both gating the whole dinucleotide; the non-overlapping `/(GC)/g` walk on `GCGCGC`; the per-strand threshold; the NOMe ACG/TCG upstream filter for both strands (incl. the `-`-strand revcomp); the `.NOMe.CpG.cov` point coordinate honouring `--zero_based`; the GpC report/cov being frozen 1-based; the raw-`-o` vs stem filename split (the rev-2 deviation); the no-uncovered-pass / covered-chromosomes-only GpC contract; non-contiguous re-emit/re-truncate; the `--gc`-leaves-core-threshold-untouched semantics; the threshold-1-gated NOMe summary with the non-NOMe base name. **No divergence found.**

---

## Source review (logic / structure / errors / efficiency)

### gpc.rs
- `emit_gpc_dinucleotide` (`:212`) maps exactly onto Perl `:848-940`/`:966-1060`. `pos = j+2` (`:222`), `tri_top = perl_substr(seq, pos-1, 3)` = `seq[j+1..j+4]`, `tri_bottom = revcomp(perl_substr(seq, pos-4, 3))` with the negative-wrap drop for `j<2`. Both `len<3` guards (`:233`) and the both-context-classify guard (`:238-242`) skip the whole dinucleotide; coverage lookup (`:245-246`) and the NOMe CG-skip (`:250`,`:265`) are per-strand. Bottom block precedes top. All verified above. ✔
- The combined guard `if m+u >= thr && !(nome && ctx==Cg)` (`:250`,`:265`) skips `pct6` when NOMe drops a CG strand, where Perl computes-then-discards. Output-identical (`pct6` is side-effect-free) and no div-by-zero (threshold ≥ 1). ✔
- The driver mirrors `report::run_single`/`flush_split_chromosome` minus the summary and uncovered pass; fresh truncating writers per chromosome (no caching). ✔
- `gpc_base`/`gpc_report_path`/`gpc_cov_path` (`:325-354`) build from `output_raw` + `.chr` + `.NOMe`, matching Perl `:795-799`. ✔
- The only `unwrap()`s are in the `#[cfg(test)]` `walk` helper (`:388-389`). Production code is panic-free.

### report.rs (NOMe additions)
- `emit_position` (`:169`): the NOMe ACG/TCG filter (`:219`) runs **after** `context_reporting`/summary accumulation (`:207-209`) and after the CpG-only emit filter (`:211`), matching Perl `:381`→`:384-393` (so filtered positions still count in the summary). The `.NOMe.CpG.cov` companion (`:243-257`) is a point coordinate honouring `--zero_based`, written only when `nome`. ✔
- The uncovered pass is gated `config.threshold == 0 && !config.nome` (`:377`, `:445`) — equivalent to Perl's `if($nome){skip} elsif($threshold>0){skip} else{process}` (`:708-718`). The `!nome` is redundant given NOMe always has threshold ≥ 1, but it is harmless and matches Perl's distinct `if($nome)` branch — good defensive clarity. ✔
- `report_name` (`:498`) inserts `.NOMe` before the suffix; `nome_cov_path` (`:556`) uses the raw `-o` base. Both verified. ✔

### cli.rs
- NOMe block ordered before the merge-threshold mutex (`:179-186`) → `--nome-seq --merge_CpGs --coverage_threshold 5` dies `NomeWithMerge` (Perl `:2147` < `:2174`). ✔
- `--nome-seq` ⇒ `gc_context = true`, threshold default 1 (`:203-212`); explicit threshold kept; explicit `0` still rejected (`:196`). `--gc` alone leaves threshold 0 (`:208-212`). `--drach`/`--ffs` still rejected (`:156-161`). Matches V1/V2/V3 and live Perl. ✔

### merge.rs
- Local `pct6` removed; `round6` + the discordant/merged cov writes now call `report::pct6` (`:35`, `:163-179`, `:196`). Identical formatter; all 10 Phase-D goldens green. ✔

### lib.rs
- `gpc::run_gpc` runs LAST, after `run_report` → `run_merge` (`:57-65`), matching Perl `:44`→`:58`→`:82`. `--nome-seq` ✗ `--merge_CpGs` so the merge and GpC arms never co-occur. ✔

---

## Findings

| # | Severity | Area | Finding |
|---|----------|------|---------|
| 1 | Low | Structure / docs | `report.rs:377` comment says the uncovered pass is skipped "when a positive threshold is set" — but the actual code gates on `threshold == 0`, so a `--gc --coverage_threshold N` run (core@N>0) **also** skips the uncovered pass, exactly like Perl `:714`. The behaviour is correct (verified in the `gc_thr2`/`gc_thr3` fixtures); only the inline comment under-describes the threshold>0 case. Recommend a one-line clarification; **no code change**. |
| 2 | Low | Structure | The `gpc.rs::run_gpc_single`/`run_gpc_split` chromosome-streaming loop is a near-verbatim duplicate of `report.rs::run_single`/`run_split` (read-until / parse / on-transition-flush). This is **intentional and plan-sanctioned** (the GpC walk re-reads the cov independently; the bodies diverge in the flush callback and the absence of summary/uncovered/seen-set). Not worth extracting a shared streaming driver for two callers, but noting it as the one place a future cov-format change must be edited twice. **No change recommended.** |

No Critical/High/Medium issues. No off-by-one in the GpC arithmetic, no filename mismatch, no Phase-D regression from the `pct6` move, no division-by-zero, no panic on any tested input.

---

## What I could NOT exhaust (honest scope note)
- I tested on tiny fixtures only (the Phase-4 oxy real-data gate is the contract for full-genome scale). The streaming model is identical to the already-real-data-validated v1.0 core, so this is low risk.
- I did not test `--ffs`/`--drach` interactions with `--nome-seq` (out of scope — both still rejected at CLI).
- gzip comparison is by decompressed content (gzip headers/mtime are not byte-stable and are out of the byte-identity contract, per the established Phase-D/C discipline).
