---
type: troubleshooting
title: Mapping efficiency, library type, coverage and M-bias
description: Low mapping efficiency, choosing the library type, coverage problems, 5-Base coverage at methylated CpG islands, and M-bias trimming.
tags: [troubleshooting, mapping-efficiency, library-type, coverage, m-bias, 5-base]
page_id: troubleshooting
subject_status: current
updated: 2026-11-25
sources:
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/report.rs
    commit: d48c830
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/config.rs
    commit: d48c830
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/options.rs
    commit: d48c830
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/mod.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/extractor/mod.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/extractor/cli.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/coverage2cytosine/report.rs
    commit: a199ad4
---

# Mapping efficiency, library type, coverage and M-bias

## Summary

The developer says users ask most often about low mapping efficiency, which library type to choose, coverage, and M-bias. This page shows where each one appears in Bismark's output and which options change it. The errors that stop a run are in [troubleshooting/errors.md](errors.md).

## Low mapping efficiency

1. Open the alignment report and find where the reads went ([reference/run-reports.md](../reference/run-reports.md)): `no alignments under any condition`, `did not map uniquely`, or `could not be extracted`.
2. Check the library type line in the report header. A directional run aligns only OT and OB; reads from CTOT and CTOB find no alignment (`library_line` in `aligner/report.rs`). See the next section.
3. The default score limit is `--score_min L,0,-0.2`, end-to-end (`default_linear_score` in `aligner/options.rs`). `--score_min L,0,-0.6` allows more mismatches; `--local` allows soft-clipped ends with Bowtie 2 and HISAT2.
4. Reads that map equally well to several places are counted under `did not map uniquely` and are not in the BAM. `--ambiguous` writes them to a file so you can inspect them. 5-Base and TAPS runs keep every proper pair at any MAPQ, so they reject `--ambiguous` (`resolve` in `aligner/config.rs`).

## Which library type?

| If the protocol makes | Use |
|---|---|
| Reads from OT and OB only | default (directional) |
| Reads from all four strands | `--non_directional` |
| Reads from CTOT and CTOB only (PBAT) | `--pbat` |

To check a library, align a subset with all four strands and read the strand counts:

```
bismark --non_directional -u 200000 --genome /data/genomes/GRCh38/ sample.fq.gz
```

The `CT/CT`, `CT/GA`, `GA/CT` and `GA/GA` lines give the unique hits on OT, OB, CTOT and CTOB. They show which strands the reads came from.

## Coverage

- The deduplication report gives the share removed ([reference/run-reports.md](../reference/run-reports.md)). For RRBS, amplicon and implicon data the developer says to skip deduplication.
- Paired-end extraction counts an overlapping region once: read 2 calls inside read 1's span are dropped unless `--include_overlap` is given (paired-end input only; `process_one_file` in `extractor/mod.rs`).
- `--cutoff` (bedGraph and `.cov`, default 1) and `--coverage_threshold` (cytosine report) drop low-coverage positions.
- The cytosine report lists every cytosine, including uncovered ones with counts `0 0`, unless `--coverage_threshold` is set.

## 5-Base: coverage at methylated CpG islands

- **What the code does.** 5-Base runs align with bwa-mem3's methylation-aware mode, `--meth=5base --meth-scoring neutral` (`bwamem3_options` in `aligner/options.rs`); no other engine is accepted (`reject_non_bwamem3_flags` in `aligner/config.rs`). See [usage/align-5-base-reads.md](../usage/align-5-base-reads.md).
- **What the developer measured** on the NA12878 and HG002 demo data: bwa-mem3 keeps 1.01–1.02× DRAGEN's depth in the densest methylated CpGs, against 0.48–0.50× for minimap2. With minimap2 and Bowtie 2, which count each methylated CpG as a mismatch, CpGs up to about 9 per 151 bp were unaffected, the densest CpGs that are ≥80% methylated kept 40–50% of the expected coverage, and partly methylated CpGs at 15 or more per 151 bp read 6–11 points low. The developer found that filtering 5-Base BAMs by MAPQ makes the coverage loss worse. The developer gives these results as the reason 5-Base aligns with bwa-mem3 only.

## M-bias

1. Open `<name>.M-bias.txt` from `bismark extract` ([reference/methylation-call-files.md](../reference/methylation-call-files.md)).
2. Find the read positions at each end where `% methylation` departs from the middle of the read. Paired-end files show read 1 and read 2 separately.
3. Run `bismark extract` again with `--ignore`, `--ignore_r2`, `--ignore_3prime` and `--ignore_3prime_r2` set to cover those positions. The alignment does not need to be repeated.

## Links

- [usage/align-reads.md](../usage/align-reads.md): the aligner options.
- [usage/extract-methylation-calls.md](../usage/extract-methylation-calls.md): trimming with `--ignore`.
- [troubleshooting/errors.md](errors.md): the errors that stop a run.

## Open questions

- Typical mapping efficiencies per data type, and how many bases to ignore for common kits, come from experience, not from the code. The developer has not given them yet.
