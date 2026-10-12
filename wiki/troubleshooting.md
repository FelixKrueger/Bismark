---
type: troubleshooting
title: Troubleshooting
description: Low mapping efficiency, choosing the library type, coverage problems, M-bias, and the errors that stop a run.
tags: [troubleshooting, mapping-efficiency, library-type, coverage, m-bias]
page_id: troubleshooting
subject_status: current
updated: 2026-10-12
sources:
  - repo: Bismark
    path: rust/bismark/src/aligner/report.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/aligner/options.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/aligner/error.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/extractor/cli.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/extractor/mod.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/io/error.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/extractor/error.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/coverage2cytosine/report.rs
    commit: 65b183a
---

# Troubleshooting

## Summary

The developer says users ask most often about low mapping efficiency, which library type to choose, coverage, and M-bias. This page shows where each one appears in Bismark's output and which options change it. It ends with the errors that stop a run.

## Low mapping efficiency

1. Open the alignment report and find where the reads went ([reference/run-reports.md](reference/run-reports.md)): `no alignments under any condition`, `did not map uniquely`, or `could not be extracted`.
2. Check the library type line in the report header. A directional run aligns only OT and OB; reads from CTOT and CTOB find no alignment (`library_line` in `aligner/report.rs`). See the next section.
3. The default score limit is `--score_min L,0,-0.2`, end-to-end (`build_aligner_options` in `aligner/options.rs`). `--score_min L,0,-0.6` allows more mismatches; `--local` (Bowtie 2 or HISAT2) allows soft-clipped ends.
4. Reads that map equally well to several places are counted under `did not map uniquely` and are not in the BAM. `--ambiguous` writes them to a file so you can inspect them.

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

- The deduplication report gives the share removed ([reference/run-reports.md](reference/run-reports.md)). For RRBS, amplicon and implicon data the developer says to skip deduplication.
- Paired-end extraction counts an overlapping region once: read 2 calls inside read 1's span are dropped unless `--include_overlap` (with `-p`) is given (`Cli::validate` in `extractor/cli.rs`).
- `--cutoff` (bedGraph and `.cov`, default 1) and `--coverage_threshold` (cytosine report, default 0) drop low-coverage positions.
- The cytosine report lists every cytosine, including uncovered ones with counts `0 0`, unless `--coverage_threshold` is set.

## M-bias

1. Open `<name>.M-bias.txt` from `bismark extract` ([reference/methylation-call-files.md](reference/methylation-call-files.md)).
2. Find the read positions at each end where `% methylation` departs from the middle of the read. Paired-end files show read 1 and read 2 separately.
3. Run `bismark extract` again with `--ignore`, `--ignore_r2`, `--ignore_3prime` and `--ignore_3prime_r2` set to cover those positions. The alignment does not need to be repeated.

## Errors that stop a run

| Message starts with | Cause | Fix |
|---|---|---|
| `the Bowtie 2 index of the C->T-converted genome seems to be faulty` | An index file is missing (`FaultyIndex` in `aligner/error.rs`) | Run `bismark prepare` for the same aligner: [usage/prepare-a-genome.md](usage/prepare-a-genome.md) |
| `failed to execute Bowtie 2 properly` | The aligner is not on `PATH` (`AlignerNotWorking`) | Install it, or pass `--path_to_bowtie2 <dir>` |
| `input BAM is coordinate-sorted` | Dedup or paired-end extraction got a sorted BAM (`UnsortedInput` in `io/error.rs`) | Use the BAM straight from the aligner, or name-sort it |
| `library-mode auto-detection failed` | The extractor could not tell SE from PE (`process_one_file` in `extractor/mod.rs`) | Pass `-s` or `-p` |
| `--include_overlap requires --paired-end` | Overlap option without `-p` | Add `-p` |
| `Please select a value for -p of 2 or more!` | `-p 1` | Drop `-p`, or use 2 or more |

## Links

- [usage/align-reads.md](usage/align-reads.md): the aligner options.
- [usage/extract-methylation-calls.md](usage/extract-methylation-calls.md): trimming with `--ignore`.

## Open questions

- Typical mapping efficiencies per data type, and how many bases to ignore for common kits, come from experience, not from the code. The developer has not given them yet.
