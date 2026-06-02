# GATE_OXY — Phase 9a (FastA input, SE + PE × {directional, non-directional}) 🎯

**Date:** 2026-06-02 · **Box:** oxy (`dcli ssh oxy`) · **Verdict: ✅ PASS — all 4 cells byte-identical to Perl Bismark v0.25.1 + Bowtie 2 2.5.5, at both 10k and 1M.**

## Oracle / toolchain (micromamba env `bismark-test`)
- Perl **Bismark v0.25.1**, **Bowtie 2 2.5.5**, **samtools 1.23.1** (`~/micromamba/envs/bismark-test/bin`).
- Rust **`bismark_rs` v1.0.0-alpha.1** — built ON oxy (`cargo build --release -p bismark-aligner`, ~20 s) from the uncommitted `rust/aligner` worktree (re-based onto `origin/rust/iron-chancellor 7f7d77d`; tar | `dcli ssh` → `/var/tmp/aligner_p9a`).

## Datasets
Real GRCh38 WGBS, **converted FastQ→FastA** in-gate (first N reads, `@id`→`>id`, `+`/qual dropped) from `~/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz` and `10M_PE/directional_10M_R{1,2}_val_{1,2}.fq.gz`. Genome `~/bismark_benchmarks/genome` (GRCh38 + Bisulfite_Genome).

## Method (`phase9a_fasta_gate.sh`, this dir — ephemeral, not committed)
Per cell: run Perl `bismark` then Rust `bismark_rs` with **byte-identical argv incl. `-f`** into the SAME `-o` (so the `@PG CL:` matches), Perl's outputs moved aside. Compare `samtools view -h` (samtools `@PG` filtered — gate policy P1) and the `_*_report.txt` (`^Bismark completed in ` filtered). **pbat is EXCLUDED** — `--pbat ⊕ -f` dies at config (Perl 8155).

## Results

| Cell | argv | 10k records | 1M records | Verdict |
|---|---|---|---|---|
| `se_dir`    | `-f … se.fa`                    | 8,403  | 848,131   | ✅ BAM + report byte-identical |
| `se_nondir` | `-f --non_directional … se.fa`  | 8,405  | 847,444   | ✅ |
| `pe_dir`    | `-f … -1 pe_1.fa -2 pe_2.fa`    | 16,902 | 1,703,304 | ✅ |
| `pe_nondir` | `-f --non_directional -1 … -2 …`| 16,902 | 1,703,190 | ✅ |

Both tiers: **ALL 4 CELLS PASS**. The synthesized FastA QUAL (Phred 40, `'I'×len`), the 2-line conversion + re-read, the `-f` flag, and the format dispatch are all byte-faithful at scale. The directional-vs-non-dir count delta (10k: non-dir +2; 1M: dir +687 / PE +114) is identical between Perl and Rust — evidence the `directional`-gated reject + the cross-instance ambiguity behave identically on FastA.

## Notes
- FastA output names keep `.fa` (`strip_fastq_suffix` is FastQ-only — Perl 1622): `se.fa_bismark_bt2.bam`, etc.
- Caveat (as in Phase 8 Open Q3): the directional 10M libraries land few reads on the complementary strands under `--non_directional`; the new-strand/QUAL arithmetic is pinned by the integration tests (FastA-aware `NR%2` fakes, QUAL=Phred 40), the gate confirms byte-identity at scale.
- Infra (unchanged from Phase 8): `/var/tmp` is ephemeral; SSH can drop/reschedule mid-run → ran `setsid nohup … &` detached + frequent-poll keep-alive.

## Cleanup
`/var/tmp/aligner_p9a`, `/var/tmp/aligner_p9a_gate`, `/var/tmp/phase9a_fasta_gate.sh`, `/var/tmp/phase9a_fasta_1M.log` removed from oxy after the run.
