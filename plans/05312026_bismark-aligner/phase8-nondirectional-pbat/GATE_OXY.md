# GATE_OXY — Phase 8 (non-directional + pbat, SE + PE, FastQ) 🎯

**Date:** 2026-06-02 · **Box:** oxy (`dcli ssh oxy`) · **Verdict: ✅ PASS — all 4 mode×layout cells byte-identical to Perl Bismark v0.25.1, at both 10k and 1M.**

## Oracle / toolchain (micromamba env `bismark-test`)
- Perl **Bismark v0.25.1** (`~/micromamba/envs/bismark-test/bin/bismark`)
- **Bowtie 2 2.5.5** (`bowtie2-align-s version 2.5.5`)
- **samtools 1.23.1**
- Rust **`bismark_rs` v1.0.0-alpha.1** — built ON oxy (`cargo build --release -p bismark-aligner`, ~20 s) from the uncommitted `rust/aligner` worktree (tar | `dcli ssh` → `/var/tmp/aligner_p8`).

## Datasets (real GRCh38 WGBS, `~/bismark_benchmarks/`)
- SE: `10M_SE/directional_10M_R1_val_1.fq.gz`
- PE: `10M_PE/directional_10M_R1_val_1.fq.gz` + `directional_10M_R2_val_2.fq.gz`
- genome: `genome/` (GRCh38 primary assembly + `Bisulfite_Genome/{CT,GA}_conversion`)

## Method (`phase8_gate.sh`, this dir — ephemeral, not committed)
Per cell: run Perl `bismark` then Rust `bismark_rs` with **byte-identical argv into the SAME `-o`** (so the `@PG CL:` line matches), Perl's outputs moved aside between runs. Compare:
- **BAM** — `diff` of `samtools view -h` decompressed content, with the samtools `@PG` line filtered (`grep -v ID:samtools`; Perl writes BAM via `samtools view -bSh -`, Rust via noodles has no such line — gate policy P1). The Bismark `@PG` line (incl. `CL:"bismark …"`) is compared and matches.
- **Report** — `diff` with the `^Bismark completed in ` wall-clock line filtered.

Cells: `se_nondir` (`--non_directional`), `se_pbat` (`--pbat`), `pe_nondir`, `pe_pbat` — each at `--upto 10000` then `--upto 1000000`.

## Results

| Cell | 10k records | 1M records | Verdict |
|---|---|---|---|
| `se_nondir` | 8,403 | 847,434 | ✅ BAM + report byte-identical |
| `se_pbat`   | 41    | 4,645     | ✅ BAM + report byte-identical |
| `pe_nondir` | 16,896 | 1,703,244 | ✅ BAM + report byte-identical |
| `pe_pbat`   | 24    | 3,182     | ✅ BAM + report byte-identical |

Both tiers: **ALL 4 CELLS PASS** (`PHASE-8 GATE (upto=…): ALL 4 CELLS PASS`).

## Caveat (Open Q3 — acknowledged, by design)
The gate reuses the **directional** 10M datasets with `--non_directional`/`--pbat` (Felix's resolved decision): same reads, same mode, both tools must agree byte-for-byte — which they do. But a directional library lands **very few** reads on the complementary strands: pbat sees only ~41 (10k) / ~4.6k (1M) SE and ~24 / ~3.2k PE alignments, since the G→A-only instances mostly fail to map directional reads. So the gate confirms **byte-identity at scale**, but does NOT *characterize* the new CTOT/CTOB (SE eff 2/3) / PE index-1/2 strand arithmetic at volume. That coverage comes from the **integration tests** (`tests/cli.rs`): GA-emitting fake-bowtie2 scripts engineer reads onto those strands and byte-assert FLAG/SEQ/XR/XG/XM. The two together are the proof — gate = bit-lock at scale, integration tests = first-live-path correctness. (Both code reviewers independently re-derived the GA-branch XM bytes from the Perl source and confirmed them.)

## Infra notes (for the next gate)
- `/var/tmp` on oxy is **node-local ephemeral** — wiped on workspace stop/restart/reschedule (lost the build + a mid-run gate once). The home volume (`~`, EBS) persists.
- oxy SSH endpoint can drop / reschedule mid-run (`Connection closed by remote host`, then a cached-port `Connection refused`; `dcli activate oxy` re-resolves the node). Run long gates **`setsid nohup … < /dev/null &`** (detached, survives SSH drops) and poll frequently (≈30 s) — the polling SSH doubles as an idle-stop keep-alive.
- Launch trap: `cat file | dcli ssh 'cat > script && … && bash … &'` — the `&` backgrounds the **whole** AND-list, so the backgrounded `cat > script` reads `/dev/null` and writes a 0-byte script. **Transfer the script as its own step**, then launch the detached run separately.

## Cleanup
`/var/tmp/aligner_p8`, `/var/tmp/aligner_p8_gate`, `/var/tmp/phase8_gate.sh`, `/var/tmp/phase8_gate_1M.log` removed from oxy after the run.
