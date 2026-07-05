# Phase 7 — oxy PE byte-identity gate (§7 #21)

**Box:** oxy (`dockyard-oxy-0`), env `bismark-test`. **Oracle:** Perl Bismark **v0.25.1** + Bowtie 2 **2.5.5** +
samtools **1.23.1**. **Rust:** `bismark_rs` built on oxy (`cargo 1.96`, `--release`) from the uncommitted
`rust/aligner` worktree. **Dataset:** `~/bismark_benchmarks/10M_PE/directional_10M_R{1,2}_val_{1,2}.fq.gz`
(PE-directional WGBS, GRCh38), subset to N pairs. **Genome:** `~/bismark_benchmarks/genome` (GRCh38 + bisulfite index).

## Method
Both tools run with **identical argv into the SAME `--output_dir`** (Perl moved aside between runs), so the
`@PG CL:"bismark <argv>"` line and the report's embedded read/genome paths match by construction:

```
--genome <G> -1 <subR1> -2 <subR2> --output_dir <OUT> --temp_dir <T> --unmapped --ambiguous --ambig_bam
```

Comparisons (`scripts`-style harness, `pe_gate.sh`):
- **BAM** `_pe.bam`: `diff` of `samtools view -h` both sides, filtering the samtools-injected `@PG ID:samtools`
  line (env-specific path; Phase-0 finding A — gate decompressed content, not raw BGZF).
- **`_PE_report.txt`**: `diff` filtering `^Bismark completed in ` (wall-clock).
- **`_1`/`_2` unmapped + ambiguous**: `diff` of the **decompressed** (`zcat`) FastQ (flate2 ≠ Perl gzip bytes).
- **`_pe.ambig.bam`**: `samtools view -h`, same `@PG ID:samtools` filter.

## 🔴 Gate-found defect (FIXED) — `--ambig_bam` dropped RNEXT/PNEXT/TLEN
The first 10k run was byte-identical on the BAM + report + all 4 aux files, but the **ambig BAM** differed in
**fields 6/7/8 only**: Perl's raw bowtie2 PE line carries `= <mate-pos> <tlen>`, but `build_raw_record`
(shared with the SE `--ambig_bam` path) parsed only fields 0–5, 9–10 + tags and **dropped 6/7/8** → noodles
rendered `* 0 0`. Invisible to unit tests (SE raw lines always carry `*/0/0` there) and to the dual code-review
(`--ambig_bam` is a raw passthrough, bypassing the FLAG/TLEN logic). **Fix:** `build_raw_record` now preserves
RNEXT (`=`→own tid / de-convert otherwise), PNEXT, and TLEN; SE path unchanged (still `*/0/0`). Locked by
`pe_ambig_lines_strip_read_tag_and_deconvert_rname_only` (now asserts the mate fields).

## Results

| Subset | BAM | _PE_report | unmapped _1/_2 | ambiguous _1/_2 | ambig BAM | Verdict |
|---|---|---|---|---|---|---|
| **10,000 pairs** | ✅ 16,896 rec | ✅ | ✅ 3,764 / 3,764 ln | ✅ 2,444 / 2,444 ln | ✅ 1,072 rec | **PASS** (post-fix) |
| **1,000,000 pairs** | ✅ 1,703,342 rec | ✅ | ✅ 355,576 / 355,576 ln | ✅ 237,736 / 237,736 ln | ✅ 103,110 rec | **PASS** |

**Verdict: ✅ PASS** — byte-identical to Perl v0.25.1 + Bowtie 2 2.5.5 across every PE output at 1M-pair scale.
(The full-scale 10M PE + SE + RRBS gate is **Phase 10**.) oxy scratch `/var/tmp/aligner_pe` cleaned up after.
