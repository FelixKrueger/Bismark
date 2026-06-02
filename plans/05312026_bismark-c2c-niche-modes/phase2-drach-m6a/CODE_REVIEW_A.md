# Phase 2 (`--drach`/`--m6A`) — Code Review A

**Reviewer:** Code Reviewer A (independent; no shared state with Reviewer B)
**Date:** 2026-05-31
**Worktree:** `/Users/fkrueger/Github/Bismark-c2c` (branch `rust/c2c-v1x`, uncommitted working tree)
**Scope:** the just-implemented Phase 2 of the coverage2cytosine v1.x epic — porting Perl `generate_DRACH_report` (`--drach`/`--m6A`) to Rust, byte-identical to Perl v0.25.1 (STDERR exempt).

## Top-line verdict: **APPROVE**

The implementation is byte-identical to Perl v0.25.1 across every high-risk mode I could construct. The full crate suite (155 tests) is green, `clippy --all-targets -D warnings` is clean, and `cargo fmt --check` is clean. Seven of my own independent fixtures — built from the Perl source, not from the shipped goldens — diffed byte-for-byte against live Perl with zero divergence. I found **0 Critical** and **0 High** issues. Three Low/informational notes below, none blocking.

---

## Build / lint / test (all green)
- `cargo test -p bismark-coverage2cytosine` → **155 passed, 0 failed** (92 lib + 18 phase1 + 12 phase2 + 11 B + 7 C + 10 D + 5 sanity; counts match the plan's claim exactly).
- `cargo clippy -p bismark-coverage2cytosine --all-targets -- -D warnings` → clean.
- `cargo fmt -p bismark-coverage2cytosine -- --check` → clean.

## Live-Perl byte-identity checks I ran independently (own fixtures, not the shipped goldens)
All diffed `target/debug/coverage2cytosine_rs` vs `perl ./coverage2cytosine` (worktree Perl v0.25.1). `gz` decompressed before comparison.

| Fixture (genome / cov) | Mode | Targets | Result |
|---|---|---|---|
| `GTACGTACGT`, cov@1,6 | `--drach` | **bottom-strand `pos<4` GT** (negative-substr wrap; V7) | **identical** — both emit NOTHING, no panic |
| `AAATGTTCAAAGTACGTACGT`, full cov 1..21 | `--drach` | **bottom `pos-1` C anchor** (§3.6 derivation genome) | **identical** — `chrD 5 - 5 1 GAACA CAT` agrees; independently confirms the `pos-1` coordinate |
| `TTTGAANATTTNTACATTT`, full cov | `--drach` | **N inside motif window** (non-ACGT through `revcomp` + filter) | **identical** — both empty |
| `TTTGAACATTT`, cov@7 dup'd (4,2)/(99,1) | `--drach` | **duplicate cov positions (last-write-wins)** | **identical** — `(99,1)` wins |
| `ACACACGTGTGT`, full cov | `--drach` | **adjacent AC/GT** (Rust `i+=1` vs Perl `/(AC)/g` match-set equivalence) | **identical** — both empty (no spurious/missing hits) |
| `TACAA`, cov@3 | `--drach` | **top-strand `pos=3` wrap** (1-byte drach → R missing → fail) | **identical** — both empty (distinct from the `pos=2` golden) |
| `chr1 TTTGAACATTT` + `chr2 AAATGTTCAAA`, cov@7/5 | `--drach --gzip --split_by_chromosome` | **gzip+split combined, `.chrchr1`/`.chrchr2` doubling** | **identical** — file sets match; all 4 `.gz` decompress byte-for-byte |
| `TACAA`, `--drach --CX --merge_CpGs` | mutex | **general CX×merge mutex under `--drach`** | both **die** (no DRACH output, no 20 s sleep) |
| `TACAA`, `--drach --merge_CpGs --coverage_threshold 5` | mutex | **general threshold×merge mutex under `--drach`** | both **die** |

I deliberately avoided regenerating the shipped goldens and instead diffed against live Perl on fixtures the goldens do **not** cover (pos=3 wrap, N-windows, duplicate positions, adjacent AC/GT, gzip+split-combined, the full §3.6 derivation genome).

## Independently re-verified byte-identity claims
1. **`pos = i+2`; top C @ `pos`; bottom C @ `pos-1`** — confirmed against the Perl source (`:1217`, `:1278`, `:1371`) and live `--CX`-style agreement on the §3.6 genome.
2. **`perl_substr` for both `tri_nt` and the 5-mer on both strands**, incl. the chromosome-start negative wrap that the top strand EMITS (`ACAAA` cov@2 → `chrA 2 + 9 1 AA CAA`) — the `pos=2` golden + my `pos=3` fixture both agree, and a naive slice would have underflowed/panicked.
3. **`is_drach_motif` short-slice safety** — `.first()`/`.get(1)`/`.get(4)` with Perl-`substr`-empty semantics; never indexes `[0]`/`[1]`. Verified: pos-0 missing→pass, pos-1 missing→fail, pos-4 missing→pass; non-ACGT pos-1 passes / pos-2 fails (guaranteed by AC) / pos-5 passes; the 2-byte `AA` wrap passes; 0/1-byte never panic. Confirmed it cannot panic on any slice length 0..=5+.
4. **`revcomp` == Perl `tr/ACTG/TGAC/` + `reverse`** (A↔T, C↔G, other byte unchanged incl. N) — matches the bottom-strand transform; N-fixture confirms.
5. **Threshold auto-set** `None if nome || self.drach => 1` matches Perl `:2188-2194` `unless ($threshold > 0)`; explicit value survives; explicit 0 is rejected *before* the auto-set (Perl `:2178`), identical to Rust ordering.
6. **General mutexes preserved** (CX×merge, threshold×merge) fire under `--drach` while `--drach`+`--CX` and `--drach`+`--merge_CpGs` alone are accepted — matches Perl running `process_commandline` before the `:38` early-exit. No spurious `--drach` mutex.
7. **Early-exit** in `lib.rs` (`if config.drach { return drach::run_drach(...); }`) before `report::run_report` — mirrors Perl `:38-42`; the `--drach --CX` golden confirms no normal report / summary / merge is written.
8. **Filenames** from raw `-o` (no strip; `.CpG_report.txt` suffix golden), `.chrchr1` doubling, `_DRACH_report.txt`/`_DRACH.cov`, `+.gz`, no header line.
9. **Empty cov → two 0-byte files, no panic** (single mode opens writers up front; the `Option`-guarded final flush skips the phantom `""`-chromosome) — golden + verified the never-set-`cur_chr` path cannot `unwrap`.
10. **Last-write-wins** on duplicate positions; **covered-only**, **cov-appearance order**, **all `+` then all `-`** within a chromosome.
11. **`pct6` reuse** — only called when `meth+nonmeth >= threshold >= 1`, so no division by zero; `{:.6}` matched Perl `%.6f` on the classic `83.333333`/`66.666667` rounding cases.

## Findings by area

### Logic / correctness — none (Critical/High/Medium: 0)
Both motif walks, the DRACH filter, the cov lookups, the threshold gate, the chromosome flush order, and the driver/early-exit all reproduce Perl exactly. The `AC`/`GT` `i += 1` scan is provably equivalent to Perl's `/(AC)/g` `pos`-advance for these distinct-byte 2-mers (confirmed empirically on `ACACACGTGTGT`).

### Errors / panics — none
- No indexing that can panic: `is_drach_motif` is `.get`-based; the strand loops gate on `i + 1 < seq.len()`; `perl_substr` is bounds-clamped; `pos - 1` has `pos >= 2` so no `u32` underflow. A 0/1-byte chromosome never enters either scan.
- The `perl_substr` `offset < -len` divergence (helper clamps `start` to 0; Perl shrinks `want`) is **unreachable** from DRACH: the most-negative offset is `pos-4 >= -2` and any chromosome with an `AC`/`GT` has `len >= 2`, so `offset >= -len`. Re-derived and confirmed at the `len==2` boundary (`substr("GT",-2,3)="GT"`).

### Structure / style — clean
`drach.rs` mirrors `gpc.rs` (the structural twin): per-chr `HashMap` buffering, flush-on-transition, single/split drivers, `ReportWriter` reuse, `Option<Vec<u8>>` cur-chr skeleton. Doc comments are accurate and cite the exact Perl lines. Reuses `perl_substr`/`revcomp`/`pct6`/`ReportWriter`/`open_cov`/`parse_cov_line` rather than re-deriving. `#[allow(clippy::too_many_arguments)]` on `push_drach_report` is justified (7 output fields).

### Low / informational (non-blocking)
- **L1 (shared, pre-existing).** `parse_cov_line` returns `MalformedCovLine` on a short/non-numeric cov line, whereas Perl `:1144` `split /\t/` silently tolerates malformed input (undef fields). This is the deliberate Phase-B cov-parse policy (stricter than Perl on malformed input) inherited by both the main report and DRACH — not a Phase-2 regression. Only matters on genuinely malformed cov files, which the byte-identity contract does not target.
- **L2 (untested edge, correct by construction).** A cov chromosome **absent from the genome** yields empty bytes (`genome.get` → `None` → `return`), matching Perl's empty `while (undef =~ /(AC)/g)` walk (with a STDERR "uninitialized value" warning, exempt). In `--split_by_chromosome` mode this still produces two empty per-chr files (Perl opens the filehandles in `filehandles_func` before the empty walk). Not covered by a golden; verified by reading both code paths. Optional: a fixture with a cov chr missing from the FASTA would pin it.
- **L3 (out of scope, tracked).** The latent `perl_substr` `offset < -len` divergence is documented in `report.rs` and confirmed unreachable here; the plan correctly defers it.

### Incidental working-tree note (not Phase-2 source)
`plans/.../phase3-ffs/PLAN.md` and `PROGRESS.md` are modified in the working tree — these are documentation-only (Phase 3 rev-1 plan-review folds) and not part of the Phase 2 source change. `gpc.rs`/`report.rs` are touched only to add `drach: false` to their in-test `ResolvedConfig` literals (required because the struct gained a field) — correct and necessary.

## Summary
Phase 2 is a faithful, byte-identical port of Perl `generate_DRACH_report`. Every behavioral claim in PLAN.md rev 3 — the top-strand `pos<4` emit-via-`perl_substr`, the `.get`-based `is_drach_motif`, the bottom `pos-1` anchor, the general-mutex preservation, the empty-cov guard, the truncated-5-mer pass-and-emit, and the single/split/gzip writers — held under both the shipped goldens and my independent live-Perl checks. No defect found. **APPROVE.**
