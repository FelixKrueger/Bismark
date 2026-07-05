# Phase 10 — oxy full-scale real-data gate (results)

> **Epic:** `05312026_bismark-aligner/EPIC.md`, Phase 10 (the LAST phase of the faithful aligner port).
> **Status:** ✅ COMPLETE — PASS. All 4 cells (SE/PE/RRBS/pbat) byte/content-identical to Perl v0.25.1 at full scale (Gate A 10M + Gate B full). See the Verdict section below.

- **Date:** 2026-06-03 → 2026-06-04
- **Box:** oxy (`dockyard-oxy-0`); Felix's allocation **32 cores / 256 GB** (the pod advertises the node's 128c/991G).
- **Pins:** Perl Bismark **v0.25.1** · Bowtie 2 **2.5.5** · samtools **1.23.1** · `bismark_rs` **1.0.0-alpha.1** (built on oxy from iron-chancellor `15a34f1`; `git diff origin/rust/iron-chancellor -- rust/bismark-aligner` empty — V1).
- **Worker count:** gates run at **P=8** (16 Bowtie 2 processes; headroom on 32 cores). Correctness is worker-count-invariant, so P does not affect the byte/content verdict.

## Reproduction tuple
```
iron-chancellor commit : 15a34f10d182cc5a84f046239803ecdaee383547
bismark_rs             : Bismark Aligner (Rust port) Version: 1.0.0-alpha.1
perl bismark           : v0.25.1
bowtie2                : 2.5.5
samtools               : 1.23.1
datasets (~/bismark_benchmarks): full_size SE=83,985,631 reads; full_size PE=83,985,631 pairs;
                         RRBS_PE=46,706,133 pairs (mouse GRCm39); 10M_SE/10M_PE=10,000,000
```

## Acceptance model (PLAN rev 2)
Hybrid: **Gate A** = strict byte-identity on the 10M subset (`Rust --parallel 1` == `Perl single-core`) + worker-invariance + the **direct A1-assumption** (`Perl --multicore` == `Perl single-core` multiset); **Gate B** = full-scale content-identity (`Rust --parallel P` vs `Perl --parallel P`, multiset after `LC_ALL=C sort`) + report identity + count reconciliation + distinct-RNAME set + aux + perf, with the V13 cross-check vs the pre-existing Perl `--parallel 4` BAM. Strict ordering at full scale is covered by Gate A (10M) + 9b (1M coprime).

---

## Gate A — 10M subset (P=16; PASSED) ✅

`phase10_subset_strict_gate.sh 16 10000000 "se_dir pe_dir rrbs_pe_dir"` → **ALL CELLS PASS**.
(Run at P=16 before the 32c/256G correction; result is P-invariant. Full log: `~/p10_artifacts/gateA_full.out`.)

| Cell | Genome | Main BAM recs | A-strict (Rust p1==Perl sc, byte) | A-worker (Rust pP==p1) | A-assumption (Perl mc==sc, multiset) |
|---|---|---|---|---|---|
| se_dir | GRCh38 SE | 8,501,508 (`cc92a5bb…`) | ✅ byte-identical | ✅ | ✅ |
| pe_dir | GRCh38 PE | 17,084,770 (`945e9d73…`) | ✅ byte-identical | ✅ | ✅ |
| rrbs_pe_dir | GRCm39 PE | 12,558,088 (`420ffb5e…`) | ✅ byte-identical | ✅ | ✅ |

- A-strict covered main BAM + `--ambig_bam` + report + `--unmapped`/`--ambiguous` aux, all byte-identical.
- **A-assumption PASS** is the key unlock: Perl `--multicore` emits the same record multiset as single-core (measured, not assumed) → Gate B's content compare vs Perl `--parallel P` is trustworthy.

### Gate A wall-clock (10M, P=16)
| cell | Perl single-core | Rust `--parallel 1` | Perl `--multicore 16` | Rust `--parallel 16` |
|---|---|---|---|---|
| se_dir | 26:50 | 25:12 | 5:08 | 4:26 |
| pe_dir | 58:35 | 55:35 | 7:37 | 7:29 |
| rrbs_pe_dir | 1:04:07 | 1:03:49 | 7:31 | 8:04 |

*Single-core → `--parallel 16` is a ~6× scaling win of identical shape on both sides (it comes from N independent Bowtie 2 pipelines, not the wrapper language). Bowtie 2 (~74%, unchanged by the port) dominates wall-clock; the Rust contribution is the wrapper delta. No per-core Rust-vs-Perl figure is published (`feedback_extractor_parallel_cpu_messaging`).*

---

## Gate B — full scale (P=8) — 3 realistic cells ✅ CONTENT BYTE-IDENTICAL

`phase10_fullscale_content_gate.sh 8 "" "se_dir pe_dir rrbs_pe_dir"` (2026-06-04 06:58→13:10 UTC). Full log: `~/p10_artifacts/gateB_full.out`.

| Cell | Reads | B1 report | B1.5 count (Perl==Rust) | B2 content md5 | B2.5 RNAME-set | B3 aux+ambig | V13 (vs old `--p4`) |
|---|---|---|---|---|---|---|---|
| se_dir | 83,985,631 | ✅ | ✅ 71,325,696==71,325,696 | ✅ `ec132828…` (71,325,696 recs) | ✅ **173 contigs** | ✅ | ✅ same md5 |
| pe_dir | 83,985,631 pairs | ✅ | ✅ 143,434,086==143,434,086 | ✅ `ef33d791…` (143,434,086 recs) | ✅ **181 contigs** | ✅ | ✅ same md5 |
| rrbs_pe_dir | 46,706,133 pairs | ✅ | ✅ 55,387,646==55,387,646 | ✅ `1ea3c26b…` (55,387,646 recs) | ✅ **52 contigs** (mouse) | ✅ | n/a (regenerated) |

- **Every realistic cell is content byte-identical Perl vs Rust at full scale** (B2 sorted-multiset md5 identical). Header byte-identical (`@PG` filtered): `@HD VN:1.0 SO:unsorted`, `@SQ`=194 (human) / 61 (mouse), **no `@CO`/`@RG`** → resolves the reviewer-B header-completeness concern (nothing path-derived survives the filter). Distinct-`RNAME` set identical → full scaffold/chromosome-diversity coverage (incl. GRCm39).
- **V13 (layout-invariance + provenance):** for SE+PE, the pre-existing Perl `--parallel 4` BAM has the **same** content md5 as the fresh Perl `--parallel 8` (and as Rust) → three Perl worker-layouts + the Rust port all converge on identical content; retires the old-BAM's unrecorded Bowtie 2 version.

> **B1.5 formula (fixed + re-verified):** the as-run harness computed `implied = unique_best_hit × mate-factor`, which over-counts by the **genomic-seq-extraction discards** (reads with a unique hit but no extractable genomic sequence are not written to the BAM). This false-flagged se_dir (off by 36) and pe_dir (off by 74 = 37 pairs × 2); RRBS had 0 discards and reconciled outright. The **essential guard `Perl view -c == Rust view -c` PASSED on all three**, and B2 content md5 is identical, so the port is correct — the implied line was benign and self-validating (off by *exactly* the documented discard count). Corrected formula `implied = (unique_best_hit − discarded) × mate-factor` re-verified against the finished BAMs: se_dir 71,325,732−36 = 71,325,696 ✅; pe_dir (71,717,080−37)×2 = 143,434,086 ✅ — both `== perl_view == rust_view`. Fix committed to the harness.

### Gate B full-scale wall-clock (P=8) + peak RSS
| Cell | Perl `--parallel 8` | Rust `--parallel 8` | Perl maxRSS | Rust maxRSS |
|---|---|---|---|---|
| se_dir (84.0M SE) | 33:22 | 35:60 | 3.44 GB | 3.44 GB |
| pe_dir (84.0M pairs) | 1:13:03 | 1:18:52 | 3.45 GB | 3.44 GB |
| rrbs_pe_dir (46.7M pairs) | 45:47 | 46:48 | 3.11 GB | 3.11 GB |

*Honest framing: at full scale / P=8 Rust is ~4–8% slower wall-clock than Perl, whereas at 10M / P=16 (Gate A) Rust was ~5–14% faster — the Amdahl signature of a Bowtie 2-dominated (~74%, unchanged) workload where the comparison really measures wrapper overhead in the remaining ~26%, and that delta shifts with P and data size. The port's value is byte-fidelity + `--multicore` scaling (~6× from `--parallel 1`→`16`, identical shape on both sides), NOT a single-config wall-clock ratio; no per-core Rust-vs-Perl number is published. (maxRSS is per-launcher-process, not aggregate.)*

## pbat_pe cell — Felix-directed (O5) ✅ PASS
R2-as-`-1` + R1-as-`-2` + `--pbat` on the directional PE data → genuine CTOT/CTOB alignments at full scale (directional data run plain `--pbat` lands ~0 reads; the swap makes it a real test). Same swapped input to Perl + Rust. Logs: `~/p10_artifacts/gateA_pbat_full.out`, `~/p10_artifacts/gateB_pbat_full.out`.

- **Gate A pbat (10M, P=8): ✅ ALL CELLS PASS** — A-strict (Rust p1==Perl sc byte), A-worker (pP==p1), **A-assumption (Perl mc==sc multiset)** all green; main BAM `b5b41c41…` 17,084,760 recs, ambig 1,030,546. Confirms the multicore-invariance premise holds for the pbat 4-instance path at scale.
- **Gate B pbat (full, 84M pairs, P=8): ✅ ALL CELLS PASS** — content multiset identical Perl vs Rust (`6ff34d1f…`, **143,434,062 recs**), B1.5 reconciles cleanly (first full-scale run on the *fixed* harness → confirms the discard-subtraction fix in situ), header identical, RNAME-set 181 contigs, aux + ambig identical. Perf P=8: Perl 1:12:58 / Rust 1:18:30. (No V13 — no pre-existing pbat oracle BAM.)
- pbat's 143,434,062 records vs directional pe's 143,434,086 (24 fewer) confirms the complementary-strand search makes genuinely different per-read decisions — a real pbat exercise, not a directional re-run — and Rust matches Perl on that distinct count.

---

## Verdict — ✅ PASS (Phase 10 complete)

**The faithful Bismark aligner Rust port is byte/content-identical to Perl Bismark v0.25.1 + Bowtie 2 2.5.5 at full real-data scale**, across **every gated cell**:

| Library | Genome | Scale | Gate A (10M strict+worker+assumption) | Gate B (full content) |
|---|---|---|---|---|
| SE directional | GRCh38 | 84.0M reads | ✅ | ✅ 71,325,696 recs / 173 contigs |
| PE directional | GRCh38 | 84.0M pairs | ✅ | ✅ 143,434,086 recs / 181 contigs |
| RRBS PE directional | GRCm39 | 46.7M pairs | ✅ | ✅ 55,387,646 recs / 52 contigs |
| pbat PE | GRCh38 | 84.0M pairs | ✅ | ✅ 143,434,062 recs / 181 contigs |

- **Content byte-identical** (decompressed SAM multiset), **headers** identical (`@PG` filtered), **reports** identical, **`--unmapped`/`--ambiguous`/`--ambig_bam`** identical, **distinct-RNAME sets** identical (full scaffold diversity + GRCm39).
- **Strict ordering** locked at 10M (Gate A, in-order `cmp` vs Perl single-core) + 1M coprime (9b); **worker-invariance** re-confirmed at 10M.
- **A1 assumption measured** (not assumed): Perl `--multicore` ≡ single-core multiset on all four cells at 10M.
- **V13:** the pre-existing Perl `--parallel 4` SE+PE BAMs carry the same content md5s as fresh Perl `--parallel 8` and Rust → four independent layouts converge byte-for-byte at full scale.
- **Perf (honest):** Bowtie 2-dominated (~74%, unchanged); the `--multicore` scaling win (~6× from `--parallel 1`→`16`) is identical in shape on both sides; wrapper-overhead delta is ±~4–14% depending on P/scale. No per-core Rust-vs-Perl figure published.

**Residuals (accepted, PLAN §9):** non-directional not gated at full scale (covered at 1M/9b + 10k Phase 8; same genome/scaffolds as the gated directional+pbat cells → no new coverage forgone); the O6 reordering-only-at-huge-chunk ordering risk is the one thing the order-normalized full-scale gate cannot see (mitigated by Gate A worker-invariance at the Gate-B P + 9b coprime proof).

This closes Phase 10 — the **last phase of the faithful aligner epic**. The aligner now does SE+PE, FastQ+FastA, all library types, byte-identical, worker-invariant, **validated at full real-data scale.**
