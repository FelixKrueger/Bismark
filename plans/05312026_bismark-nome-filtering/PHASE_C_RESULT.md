# Phase C — real-data byte-identity gate: RESULT ✅ GREEN

**Date:** 2026-06-01 · **Host:** oxy (`dockyard-oxy-0`, Linux x86_64, 991 GB RAM) · **Env:** micromamba `bismark-test`
**Perl reference:** `~/Bismark/NOMe_filtering` **v0.25.1** · **Rust:** `NOMe_filtering_rs 0.1.0-beta.1` (release, cargo 1.96.0), built from `origin/rust/nome-filtering` @ `bb612c7`
**Genome:** `~/bismark_benchmarks/genome/Homo_sapiens.GRCh38.dna.primary_assembly.fa` (plain `.fa` → two-plain-tier glob finds it)
**Input:** real `--yacht` from Perl `bismark_methylation_extractor -s --yacht` on the **10M SE** Bismark BAM (`directional_10M_R1_val_1_bismark_bt2.bam`) → `any_C_context_…txt` = **10.29 GB**

## Verdict: byte-identical (Rust ≡ Perl) on real data

| Cell | Input | Result | Output lines | decompressed md5 |
|------|-------|--------|--------------|------------------|
| **C1** | full plain `.txt` (10.29 GB) | ✅ PASS byte-identical | 8,494,374 | `7bdf7d5d9735246d10aa657c27695ce4` |
| **C2** | gz-input (subset) | ✅ PASS byte-identical | 80,571 | `86bce8f39f0ddbbc506d52911a9c5830` (== plain-subset) |

Comparison method: `cmp <(gunzip -c perl) <(gunzip -c rust)` — **decompressed** content (the gzip *container* differs by design: Perl `gzip -c` 156.8 MB vs Rust `flate2` 162.1 MB for identical content; SPEC §6 / pitfall P8).

## Performance (advisory — not a gate)
Single-threaded, full 10.29 GB input, genome warm in page cache:

| | wall clock | max RSS |
|---|---|---|
| Perl `NOMe_filtering` | 5:00.9 | 3.28 GB |
| Rust `NOMe_filtering_rs` | 1:27.8 | 3.11 GB |

→ **~3.4× faster**, near-identical memory (both dominated by the hg38 genome held in RAM, matching Perl's model). Reported as honest single-threaded wall-clock at equal work.

## Checklist status
- [x] oxy: rustup + `cargo build --release` clean; `cargo test -p bismark-nome-filtering` green on Linux (49 unit + 6 cli + 9 golden + 1 doctest).
- [x] Perl `NOMe_filtering --version` == v0.25.1.
- [x] Real `--yacht` input generated (10.29 GB; 8,494,374 suitable-read output lines).
- [x] **C1 (full plain) PASS** byte-identical.
- [x] **C2 (gz-input) PASS** byte-identical, md5 == plain.
- [~] **C3** (native single-cell NOMe-Seq SE sample): none present on oxy → gated on the benchmark **10M SE** `--yacht` instead (a real, full-scale stressor; the comparison is Perl-NOMe vs Rust-NOMe on a common input regardless of provenance).
- [x] CHANGELOG + crate README added.
- [x] Committed the gate driver (`tests/data/phase_c/nome_gate.sh`) + this result + version bump (0.1.0→1.0.0-beta.1) in release commit `2a2a6a5`.
- [x] **Tagged `bismark-nome-filtering-v1.0.0-beta.1`** (at `2a2a6a5`) + branch + tag pushed to `origin`.
- [x] oxy gate artifacts purged (~10 GB); `~/nome-build` worktree + binary kept.

## Reproduce
Driver: `tests/data/phase_c/nome_gate.sh` (committed). On oxy: launch from the per-side output dir with a bare filename (Perl checks `-e` at launch-cwd then opens relative to `--dir`; launching from the dir with no `--dir` satisfies both — the extractor's real invocation pattern).
