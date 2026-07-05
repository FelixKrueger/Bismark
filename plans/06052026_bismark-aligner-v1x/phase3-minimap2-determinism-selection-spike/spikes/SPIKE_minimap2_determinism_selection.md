# SPIKE — minimap2 determinism + both-strand selection (v1.x Phase 3)

> **🔴 [CORRECTION 2026-06-05, Phase-4 dual review] — IGNORE this report's "the merge must read `s2:i:`" claims (Q4 / §3 / §4 / §5).** A Perl-source trace for the Phase-4 plan showed Bismark's parse loop (`bismark` 2772-2796) has **no `s2:i:` branch** — Bismark **IGNORES `s2`**, so minimap2's within-instance 2nd-best is **always undef → `second_best=None`** (the existing no-2nd-best path; `calc_mapq` then uses the cross-instance runner-up + default `(0,-0.2)`). **Do NOT add an `s2:i:` parse branch** — it would silently break MAPQ byte-identity. The determinism / order / both-strand / options / `/1`-retention findings below stand; only the `s2`→merge claim is wrong. (Also: the `/1` retention is **PE-only**; SE appends no suffix.)

- **Date:** 2026-06-05 · **Epic:** `06052026_bismark-aligner-v1x` Phase 3 (the highest-risk phase, OQ3).
- **Box:** oxy `dockyard-oxy-0` — Perl Bismark **v0.25.1**, **minimap2 2.31-r1302**, samtools 1.23.1, human GRCh38 (`.mmi` index present, ~7.9 GB each).
- **Script:** `spikes/spike_minimap2_determinism_selection.sh` (run on oxy: `bash … 10000`). Throwaway.

## 1. Question / success / scope
- **Q1 (gating):** is Perl `bismark --minimap2` + minimap2 2.31 byte-deterministic run-to-run on real bisulfite reads (→ byte-identity reachable)?
- **Q2:** what `aligner_options` does Bismark assemble for minimap2?
- **Q3:** minimap2 has `--norc`/`--nofw` **commented out** (Perl 7012/7015) → each instance aligns BOTH strands. How does that change the unique-vs-ambiguous arithmetic vs the strand-restricted Bowtie2/HISAT2 model?
- **Q4:** which tags does minimap2 emit (the 2nd-best the merge needs)?
- **Q5:** is input ORDER preserved in the output (Bismark's lockstep parse needs it)?
- **Success:** two `bismark --minimap2` runs byte-identical (decompressed SAM, `@PG` filtered) + characterize the both-strand selection + decide byte-identity-vs-concordance.
- **Scope:** SE directional, 10k, human GRCh38, minimap2 only. NOT the Rust port (Phase 4), NOT PE/non-dir/pbat, NOT full scale.

## 2. Results

| Q | Result |
|---|---|
| **Q1 Determinism** | ✅ **HOLDS.** run1 vs run2 decompressed SAM differed by **exactly the `@PG CL` line** (it embeds `-o run1` vs `-o run2` — a harness artifact, same as the multicore lesson); the **alignment body is byte-identical run-to-run**, even with `-t 2`. minimap2 2.31 is run-to-run deterministic at the Bismark level. |
| **Q2 `aligner_options`** | **`-a --MD --secondary=no -t 2 -x map-ont -K 250K`** (default, no preset flag). **🔴 `-x map-ont`** (Oxford Nanopore preset) — **NOT `-ax sr`** as the SPEC §3.2 / the Perl code comments (7007-7009) assumed. Positional invocation: `minimap2 <opts> <BS_CT.mmi> <reads>` (Perl 7025); reads the C→T temp `…_C_to_T.fastq`. |
| **Q3 Both-strand selection** | **Largely MOOT under the real options.** With `-x map-ont --secondary=no` on the CT instance (no `--norc`): **4354 forward (flag 0), 0 reverse (flag 16), 0 secondary/supplementary.** The commented-out `--norc` would suppress reverse-strand hits — but `--secondary=no` + map-ont emit **one forward primary per mapped read**, so the suppression essentially never fires. (Under the more-sensitive `-ax sr` the spike saw 17/~6000 reverse + 81 supplementary — preset-dependent, but `sr` is NOT the default.) The Bismark `--minimap2` SE report (both instances): 10000 analysed → **7933 unique-best, 1755 no-align, 312 ambiguous, 1 discard** (vs HISAT2 8361 / 484 / 1155 — minimap2+map-ont is **less sensitive** for short reads ⇒ more no-align, fewer multi). |
| **Q4 Tags / 2nd-best** | minimap2 emits `AS:i: ms:i: s1:i: s2:i: NM:i: de:f: tp:A: cm:i: nn:i: rl:i:` (+ `SA:Z:` on chimeric). **No `ZS:i:`/`XS:i:`.** The within-instance 2nd-best = **`s2:i:`** (the 2nd-best chaining score; 540/10000 reads have `s2>0`). The Phase-4 merge's 2nd-best source for minimap2 is `s2:i:`, not the `XS`/`ZS` the Bowtie2/HISAT2 parser uses. |
| **Q5 Order** | ✅ **Preserved.** Raw minimap2 qnames come out in input order (`SRR…​.1, .2, .3, …`) even at `-t 2` — minimap2 collects minibatch output in input order, so Bismark's read-ID lockstep parse works. |

## 3. Findings
- **Byte-identity is REACHABLE for minimap2** — deterministic run-to-run, output in input order, no seed/reorder surprises. **No concordance fallback needed.**
- **The headline risk (both-strand selection) is much milder than the SPEC feared.** `--secondary=no` + `-x map-ont` produce a single forward primary per mapped read; the commented-out `--norc`/`--nofw` has ~no practical effect on the default-preset output (0 reverse on the CT instance at 10k). So the Phase-4 **merge adaptation is small** — NOT a fundamental selection rewrite. The cross-instance (CT vs GA) merge is the same 2/4-instance model; the new work is the **2nd-best tag (`s2:i:`)**, the **options assembly** (map-ont preset), the **positional `.mmi`** invocation, and **`/1 /2` retention**.
- **Two SPEC corrections the spike caught:** (a) default preset is **`map-ont`**, not `sr`; (b) 2nd-best tag is **`s2:i:`**, not `ZS`/`XS`.

## 4. Reference snippets (carry to Phase 4)
- **Options (default):** `-a --MD --secondary=no -t 2 -x map-ont -K 250K`. Presets: default `map-ont`; `--mm2_short_reads`→`sr`; `--mm2_pacbio`→ pacbio; `--mm2_nanopore`→ ont (trace the exact `process_command_line` assembly in Phase 4). NO `--score-min L,0,-0.2`.
- **Invocation (Perl `single_end_align_fragments_…_minimap2`, 6982-7028):** `minimap2 <opts> <BS_{CT,GA}.mmi> <C_to_T/G_to_A temp> |` piped; 2 instances SE-directional (CTreadCTgenome on BS_CT, CTreadGAgenome on BS_GA), **no `--norc`/`--nofw`** (7012/7015 commented).
- **2nd-best:** `s2:i:` (minimap2 2nd-best chaining score). The merge must read `s2:i:` for minimap2 (cf. `XS:i:` Bowtie2 / `ZS:i:` HISAT2).
- **`/1 /2`:** minimap2 does NOT strip the trailing `/1`/`/2`; Bismark appends `/1`/`/2` to the identifier (Perl 5947/5955) — converter/ID delta.
- **Naming/report:** `_bismark_mm2{,_pe}.bam` / `_bismark_mm2_{SE,PE}_report.txt`; report line "Bismark was run with minimap2 …".
- **Version parse:** minimap2 reports only the version number (Perl 7081-7083) — the existing `split("version")` parser won't match; Phase 4 needs a minimap2-specific version parse.

## 5. Recommendation
**PROCEED to Phase 4 (minimap2 wrapper) targeting a byte-identity gate.** Determinism + order hold; the both-strand merge concern is small under the default `map-ont --secondary=no` options. Phase-4 scope: generalize `aligner.rs` for minimap2 (binary resolve + version pin 2.31-r1302 + the minimap2-only version parse); `options.rs` minimap2 assembly (`-a --MD --secondary=no -t 2 -x map-ont -K 250K` + the preset flags); positional `.mmi` discovery + invocation; the `s2:i:` 2nd-best in `align.rs`/the merge; `/1 /2` retention in `convert.rs`; `_bismark_mm2` naming + report wording. **⚠️ the dovetail trap from 2b applies (config.dovetail must NOT come from the option string).** Then the byte-identity gate (SE → PE → non-dir/pbat per the faithful-port precedent).

## 6. Limitations
- SE directional, 10k, default preset (`map-ont`) only. The both-strand population is **preset-dependent** (`-ax sr` showed a small reverse + supplementary population); Phase 4 should characterize the non-default presets if they're gated, and confirm determinism at scale (1M) + multi-minibatch ordering in the gate (10k already spans ~3 minibatches at `-K 250K`, and held).
- The raw both-strand characterization re-created the C→T conversion with `awk gsub(/C/,T)` on the read lines — a faithful approximation of Bismark's directional read-1 conversion; the exact temp the merge consumes was deleted by the Bismark run.
- minimap2's `s2:i:`→2nd-best→MAPQ path was not traced through the Perl merge here (Phase 4): how Bismark maps `s2` to the `$second_best` it feeds `calc_mapq`, and whether minimap2 even uses the same MAPQ formula (minimap2 emits its own MAPQ in col 5 — Bismark may recompute or pass through; trace in Phase 4).
