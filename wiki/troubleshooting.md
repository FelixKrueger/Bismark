---
type: troubleshooting
title: Troubleshooting
description: Low mapping efficiency, choosing the library type, coverage problems, low 5-Base coverage at methylated CpG islands, M-bias, the errors that stop a run (including bwa-mem3 errors), and the warnings that do not.
tags: [troubleshooting, mapping-efficiency, library-type, coverage, m-bias, 5-base]
page_id: troubleshooting
subject_status: current
updated: 2026-11-20
sources:
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/report.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/options.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/error.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/extractor/cli.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/extractor/mod.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/io/error.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/extractor/error.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/coverage2cytosine/report.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/mod.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/stream.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/config.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/aligner.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/parallel.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/extractor/state.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/genome_prep/bwamem3.rs
    commit: a0059f4
---

# Troubleshooting

## Summary

The developer says users ask most often about low mapping efficiency, which library type to choose, coverage, and M-bias. This page shows where each one appears in Bismark's output and which options change it. It ends with the errors that stop a run.

## Low mapping efficiency

1. Open the alignment report and find where the reads went ([reference/run-reports.md](reference/run-reports.md)): `no alignments under any condition`, `did not map uniquely`, or `could not be extracted`.
2. Check the library type line in the report header. A directional run aligns only OT and OB; reads from CTOT and CTOB find no alignment (`library_line` in `aligner/report.rs`). See the next section.
3. The default score limit is `--score_min L,0,-0.2`, end-to-end; 5-Base runs with Bowtie 2 or HISAT2 default to `L,0,-0.6`, or `G,20,8` with Bowtie 2 `--local` (`default_linear_score` in `aligner/options.rs`). `--score_min L,0,-0.6` allows more mismatches; `--local` allows soft-clipped ends with Bowtie 2, and with HISAT2 outside 5-Base.
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
- Paired-end extraction counts an overlapping region once: read 2 calls inside read 1's span are dropped unless `--include_overlap` is given (paired-end input only; `process_one_file` in `extractor/mod.rs`).
- `--cutoff` (bedGraph and `.cov`, default 1) and `--coverage_threshold` (cytosine report) drop low-coverage positions.
- The cytosine report lists every cytosine, including uncovered ones with counts `0 0`, unless `--coverage_threshold` is set.

## 5-Base: low coverage at methylated CpG islands

- **Cause in the code.** 5-Base runs without `--bwamem3` give the raw reads to minimap2 or Bowtie 2 against the unconverted genome, with the engine's normal scoring (`five_base_build_argv` in `aligner/mod.rs`, `minimap2_options` in `aligner/options.rs`). Each methylated CpG is then a mismatch. Bowtie 2 aligns end-to-end with a fixed score limit, `--score_min L,0,-0.6` by default (`default_linear_score` in `aligner/options.rs`), so a read with many methylated CpGs falls below it. The same read unmethylated stays above it.
- **What the developer measured** on the NA12878 and HG002 demo data:
  - CpGs up to about 9 per 151 bp are unaffected.
  - Partly methylated CpGs at 15 or more per 151 bp read 6–11 points low.
  - The densest CpGs that are ≥80% methylated keep 40–50% of the expected coverage.
  - Both engines are affected.
- **What helps.** Align with `--bwamem3`, which runs bwa-mem3's methylation-aware mode (`bwamem3_options` in `aligner/options.rs`): see [usage/align-5-base-reads.md](usage/align-5-base-reads.md). The developer measured that it keeps 1.01–1.02× DRAGEN's depth in the densest methylated CpGs, against 0.48–0.50× for minimap2. With minimap2 or Bowtie 2, expect methylated CpG islands (imprinted regions, the inactive X, hypermethylated islands) to be under-covered. The developer found that filtering 5-Base BAMs by MAPQ makes the coverage loss worse.

## M-bias

1. Open `<name>.M-bias.txt` from `bismark extract` ([reference/methylation-call-files.md](reference/methylation-call-files.md)).
2. Find the read positions at each end where `% methylation` departs from the middle of the read. Paired-end files show read 1 and read 2 separately.
3. Run `bismark extract` again with `--ignore`, `--ignore_r2`, `--ignore_3prime` and `--ignore_3prime_r2` set to cover those positions. The alignment does not need to be repeated.

## Errors that stop a run

| Message starts with | Cause | Fix |
|---|---|---|
| `the Bowtie 2 index of the C->T-converted genome seems to be faulty` | An index file is missing (`FaultyIndex` in `aligner/error.rs`) | Run `bismark prepare` for the same aligner: [usage/prepare-a-genome.md](usage/prepare-a-genome.md) |
| `failed to execute Bowtie 2 properly` | The aligner is not on `PATH` (`AlignerNotWorking`) | Install it, or pass `--path_to_bowtie2 <dir>` |
| `input BAM is coordinate-sorted` | Paired-end dedup or paired-end extraction got a coordinate-sorted BAM (`UnsortedInput` in `io/error.rs`) | Use the BAM straight from the aligner, or name-sort it |
| `--umi_mismatches needs …`, `--umi_mismatches reads each input twice …`, `input changed while it was being deduplicated`, `the second pass read …` | `bismark dedup --umi_mismatches` errors | See [usage/deduplicate-alignments.md](usage/deduplicate-alignments.md#errors) |
| `library-mode auto-detection failed` | The extractor could not tell SE from PE (`process_one_file` in `extractor/mod.rs`) | Pass `-s` or `-p` |
| `--include_overlap requires --paired-end` | `--include_overlap` with `-s`, or on a file auto-detected as single-end (`process_one_file` in `extractor/mod.rs`) | Drop `--include_overlap`: single-end reads have no read 2 |
| `Please select a value for -p of 2 or more!` | `-p 1` | Drop `-p`, or use 2 or more |
| `-I/--minins (<i>) is larger than the maximum insert size (<x>)` | A 5-Base Bowtie 2 run with `-I` above `-X`, or above the 800 bp default (`build_five_base_aligner_options` in `aligner/options.rs`) | Lower `-I`, or raise `-X` |
| `bwa-mem3 index file <file> is missing` | A `--bwamem3` index file is missing (`check_bwamem3_index_files` in `aligner/config.rs`) | For `<genome>/Bwamem3_Index/`, rebuild it with `bismark prepare --bwamem3 <genome>`; for a hand-built index, rebuild it with `bwa-mem3 index --meth <genome.fa>` |
| `--illumina_5base --bwamem3 requires --five_base_index <prefix>` (or `--taps requires …`), or `--five_base_index <dir> is a folder; did you mean <dir>/Bwamem3_Index?` | No `--five_base_index` and no `Bwamem3_Index/` in the genome folder, or a genome folder given as the index (`resolve_five_base_index` in `aligner/config.rs`) | Build the index with `bismark prepare --bwamem3 <genome>` ([usage/prepare-a-genome.md](usage/prepare-a-genome.md)), then leave out `--five_base_index`; or pass the index prefix or its `Bwamem3_Index` folder |
| `the bwa-mem3 index does not match the genome folder` (or `output header`) | The index was built from another FASTA or assembly; contig names or lengths differ (`check_contigs_match` in `aligner/mod.rs`) | Rebuild the index from the FASTA in the `--genome` folder |
| `bwa-mem3 ended before writing its SAM header; bwa-mem3 exited with signal 9` | bwa-mem3 was killed while loading its index, usually for lack of memory (`five_base_align_and_call_pe`, `describe_exit` in `aligner/mod.rs`) | Give the job more memory |
| `<file>.meth.fa in the genome folder is a bwa-mem3 index file` | The bwa-mem3 index sits in the `--genome` folder (`run_pe_five_base` in `aligner/mod.rs`) | Move the index to its own folder |
| `bwa-mem3 <version> is too old or unrecognised` | `--bwamem3` or `--taps` found a bwa-mem3 older than 0.16.0 (`detect_aligner` in `aligner/aligner.rs`) | Install bwa-mem3 0.18.0 (`mamba install -c bioconda -c conda-forge bwa-mem3`), or pass `--path_to_bwamem3 <dir>` |
| `bismark prepare --bwamem3`: `<file>: contig <name>, line <n>: …`, or `bwa-mem3 needs more memory than its budget to index <genome>` | A sequence line that bwa-mem3 reads differently from Bismark, such as whitespace, a non-ASCII byte or an empty contig (`stage_input`, `seq_line_len` in `genome_prep/bwamem3.rs`); or `bwa-mem3 index` exited 3 (`run_index`) | Fix that FASTA line (every rejected input: [usage/prepare-a-genome.md](usage/prepare-a-genome.md)); for memory, raise `--bwamem3_max_memory` or use a machine with more memory |
| `--taps aligns with bwa-mem3 only` | `--taps` was given another aligner flag (`expand_taps` in `aligner/config.rs`) | Drop the aligner flag |
| `<option> is validated for Illumina 5-Base only, not TAPS` | `--taps` with duplex, consensus, deconvolution, `--five_base_umi_qname` or `--five_base_emit_multiplicity` (`expand_taps`) | Drop the option: [usage/align-taps-reads.md](usage/align-taps-reads.md) |
| `--five_base_umi_len handles the Illumina 5-Base inline-UMI read structure …` or `--five_base_min_mapq only filters the duplex/consensus pass …` | `--taps` with a non-zero `--five_base_umi_len` or `--five_base_min_mapq` (`expand_taps`) | Drop the option |
| `<R2> has fewer reads than <R1>` (or `more`), `<engine> produced more records than read pairs` | In a 5-Base run, R1, R2 and the engine output differ in length (`five_base_align_and_call_pe`) | Check that the two files are mates from one run, and read the engine's own errors |
| `<file> has Windows (CRLF) line endings` | A read file ends its first line with `\r\n` (`reject_crlf_inputs` in `aligner/mod.rs`) | Convert it, for example with `dos2unix` |
| `the aligner reading <file> returned read names that stop matching the input` | The aligner's read names stopped matching the input (`check_merged` in `aligner/mod.rs`, `finish_checked` in `aligner/stream.rs`) | Check the input for CRLF line endings; in a streamed run, see the next row |
| `… (streamed run; if an aligner wrapper … use --no_stream_converted)` | An aligner instance failed in a streamed run; the suffix is added to such an error unless it already names `--no_stream_converted` (`with_wrapper_hint` in `aligner/stream.rs`) | Read the message before the suffix first. If a wrapper or another process reads the input before the aligner, re-run with `--no_stream_converted`; for gzipped input, also check it with `gzip -t` |
| `<fifo> is still held open by a process that does not read it` | At the end of a streamed run, something kept a converted-read FIFO open without reading it (`finish_checked` in `aligner/stream.rs`) | Re-run with `--no_stream_converted` |
| `the aligner reading <file> returned <n> of <m> reads` (or `pairs`) | An aligner instance returned fewer records than it was sent (`finish_checked`) | Check the aligner's own error output; re-run with `--no_stream_converted` if a wrapper reads the input |
| `mkfifo <path> failed` | A FIFO could not be made in `--temp_dir` during the run (`mkfifo` in `aligner/stream.rs`) | Free or fix the temp folder, or use `--no_stream_converted` |

With `--multicore`, an aligner error ends with `(in --multicore chunk <i> of <n> of <input>)` (`in_chunk` in `aligner/parallel.rs`).

`bismark extract --allow_discordant` prints `warning: --allow_discordant: <n> of <m> templates are orphans (>20%)` and keeps running. Check that mates are present and that the BAM is name-grouped (`ExtractState::finalize` in `extractor/state.rs`).

## Links

- [usage/align-reads.md](usage/align-reads.md): the aligner options.
- [usage/extract-methylation-calls.md](usage/extract-methylation-calls.md): trimming with `--ignore`.

## Open questions

- Typical mapping efficiencies per data type, and how many bases to ignore for common kits, come from experience, not from the code. The developer has not given them yet.
