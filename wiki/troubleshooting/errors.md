---
type: troubleshooting
title: Errors that stop a run
description: The messages that stop a run (including bwa-mem3 and streamed-run errors), the warnings that do not, and streamed runs that stop on Bismark 3.2.0 and 3.3.0.
tags: [troubleshooting, errors, bwa-mem3, streaming]
page_id: errors
subject_status: current
updated: 2026-11-21
sources:
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/error.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/options.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/config.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/aligner.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/mod.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/stream.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/parallel.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/io/error.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/extractor/mod.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/extractor/state.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/genome_prep/bwamem3.rs
    commit: a199ad4
---

# Errors that stop a run

## Summary

Each row gives the start of a message that stops a run, its cause in the code, and the fix. The two warnings below the table do not stop the run. The last section covers streamed runs that stop making progress on Bismark 3.2.0 and 3.3.0.

## Messages that stop a run

| Message starts with | Cause | Fix |
|---|---|---|
| `the Bowtie 2 index of the C->T-converted genome seems to be faulty` | An index file is missing (`FaultyIndex` in `aligner/error.rs`) | Run `bismark prepare` for the same aligner: [usage/prepare-a-genome.md](../usage/prepare-a-genome.md) |
| `failed to execute Bowtie 2 properly` | The aligner is not on `PATH` (`AlignerNotWorking`) | Install it, or pass `--path_to_bowtie2 <dir>` |
| `input BAM is coordinate-sorted` | Paired-end dedup or paired-end extraction got a coordinate-sorted BAM (`UnsortedInput` in `io/error.rs`) | Use the BAM straight from the aligner, or name-sort it |
| `--umi_mismatches needs …`, `--umi_mismatches reads each input twice …`, `input changed while it was being deduplicated`, `the second pass read …` | `bismark dedup --umi_mismatches` errors | See [usage/deduplicate-alignments.md](../usage/deduplicate-alignments.md#errors) |
| `library-mode auto-detection failed` | The extractor could not tell SE from PE (`process_one_file` in `extractor/mod.rs`) | Pass `-s` or `-p` |
| `--include_overlap requires --paired-end` | `--include_overlap` with `-s`, or on a file auto-detected as single-end (`process_one_file` in `extractor/mod.rs`) | Drop `--include_overlap`: single-end reads have no read 2 |
| `Please select a value for -p of 2 or more!` | `-p 1` | Drop `-p`, or use 2 or more |
| `-I/--minins (<i>) is larger than the maximum insert size (<x>)` | A 5-Base Bowtie 2 run with `-I` above `-X`, or above the 800 bp default (`build_five_base_aligner_options` in `aligner/options.rs`) | Lower `-I`, or raise `-X` |
| `bwa-mem3 index file <file> is missing` | A `--bwamem3` index file is missing (`check_bwamem3_index_files` in `aligner/config.rs`) | For `<genome>/Bwamem3_Index/`, rebuild it with `bismark prepare --bwamem3 <genome>`; for a hand-built index, rebuild it with `bwa-mem3 index --meth <genome.fa>` |
| `--illumina_5base --bwamem3 requires --five_base_index <prefix>` (or `--taps requires …`), or `--five_base_index <dir> is a folder; did you mean <dir>/Bwamem3_Index?` | No `--five_base_index` and no `Bwamem3_Index/` in the genome folder, or a genome folder given as the index (`resolve_five_base_index` in `aligner/config.rs`) | Build the index with `bismark prepare --bwamem3 <genome>` ([usage/prepare-a-genome.md](../usage/prepare-a-genome.md)), then leave out `--five_base_index`; or pass the index prefix or its `Bwamem3_Index` folder |
| `the bwa-mem3 index does not match the genome folder` (or `output header`) | The index was built from another FASTA or assembly; contig names or lengths differ (`check_contigs_match` in `aligner/mod.rs`) | Rebuild the index from the FASTA in the `--genome` folder |
| `bwa-mem3 ended before writing its SAM header; bwa-mem3 exited with signal 9` | bwa-mem3 was killed while loading its index, usually for lack of memory (`five_base_align_and_call_pe`, `describe_exit` in `aligner/mod.rs`) | Give the job more memory |
| `<file>.meth.fa in the genome folder is a bwa-mem3 index file` | The bwa-mem3 index sits in the `--genome` folder (`run_pe_five_base` in `aligner/mod.rs`) | Move the index to its own folder |
| `bwa-mem3 <version> is too old or unrecognised` | `--bwamem3` or `--taps` found a bwa-mem3 older than 0.16.0 (`detect_aligner` in `aligner/aligner.rs`) | Install bwa-mem3 0.18.0 (`mamba install -c bioconda -c conda-forge bwa-mem3`), or pass `--path_to_bwamem3 <dir>` |
| `bismark prepare --bwamem3`: `<file>: contig <name>, line <n>: …`, or `bwa-mem3 needs more memory than its budget to index <genome>` | A sequence line that bwa-mem3 reads differently from Bismark, such as whitespace, a non-ASCII byte or an empty contig (`stage_input`, `seq_line_len` in `genome_prep/bwamem3.rs`); or `bwa-mem3 index` exited 3 (`run_index`) | Fix that FASTA line (every rejected input: [usage/prepare-a-genome.md](../usage/prepare-a-genome.md)); for memory, raise `--bwamem3_max_memory` or use a machine with more memory |
| `--taps aligns with bwa-mem3 only` | `--taps` was given another aligner flag (`expand_taps` in `aligner/config.rs`) | Drop the aligner flag |
| `<option> is validated for Illumina 5-Base only, not TAPS` | `--taps` with duplex, consensus, deconvolution, `--five_base_umi_qname` or `--five_base_emit_multiplicity` (`expand_taps`) | Drop the option: [usage/align-taps-reads.md](../usage/align-taps-reads.md) |
| `--five_base_umi_len handles the Illumina 5-Base inline-UMI read structure …` or `--five_base_min_mapq only filters the duplex/consensus pass …` | `--taps` with a non-zero `--five_base_umi_len` or `--five_base_min_mapq` (`expand_taps`) | Drop the option |
| `<R2> has fewer reads than <R1>` (or `more`), `<engine> produced more records than read pairs` | In a 5-Base run, R1, R2 and the engine output differ in length (`five_base_align_and_call_pe`) | Check that the two files are mates from one run, and read the engine's own errors |
| `<file> has Windows (CRLF) line endings` | A read file ends its first line with `\r\n` (`reject_crlf_inputs` in `aligner/mod.rs`) | Convert it, for example with `dos2unix` |
| `the aligner reading <file> returned read names that stop matching the input` | The aligner's read names stopped matching the input (`check_merged` in `aligner/mod.rs`, `finish_checked` in `aligner/stream.rs`) | Check the input for CRLF line endings; in a streamed run, see the next row |
| `… (streamed run; if an aligner wrapper … use --no_stream_converted)` | An aligner instance failed in a streamed run; the suffix is added to such an error unless it already names `--no_stream_converted` (`with_wrapper_hint` in `aligner/stream.rs`) | Read the message before the suffix first. If a wrapper or another process reads the input before the aligner, re-run with `--no_stream_converted`; for gzipped input, also check it with `gzip -t` |
| `<fifo> is still held open by a process that does not read it` | At the end of a streamed run, something kept a converted-read FIFO open without reading it (`finish_checked` in `aligner/stream.rs`) | Re-run with `--no_stream_converted` |
| `the aligner reading <file> returned <n> of <m> reads` (or `pairs`) | An aligner instance returned fewer records than it was sent (`finish_checked`) | Check the aligner's own error output; re-run with `--no_stream_converted` if a wrapper reads the input |
| `mkfifo <path> failed` | A FIFO could not be made in `--temp_dir` during the run (`mkfifo` in `aligner/stream.rs`) | Free or fix the temp folder, or use `--no_stream_converted` |
| `an aligner instance exited (<status>) without reading its converted reads from <fifo>` | In a streamed run, an aligner instance exited with an error before it opened its FIFO (`await_readers` in `aligner/stream.rs`) | Read the aligner's own error output, which names the cause |
| `the conversion passes over <file> disagree` | Each aligner instance reads its own conversion pass over the input; two passes over one file counted different records, so the file changed during the run (`fold_by_source` in `aligner/stream.rs`) | Re-run on a finished file that nothing writes to |

With `--multicore`, an aligner error ends with `(in --multicore chunk <i> of <n> of <input>)` (`in_chunk` in `aligner/parallel.rs`).

`bismark extract --allow_discordant` prints `warning: --allow_discordant: <n> of <m> templates are orphans (>20%)` and keeps running. Check that mates are present and that the BAM is name-grouped (`ExtractState::finalize` in `extractor/state.rs`).

## Streamed runs that stop on Bismark 3.2.0 and 3.3.0

The developer says that in these releases a streamed run can stop making progress, with no CPU use and a BAM that no longer grows (#1268). Two aligner instances that share one conversion pass can wait on each other under CPU contention, and streamed `--minimap2` stalls at start-up when the genome's SAM header is larger than a pipe buffer. On those releases, re-run with `--no_stream_converted`. From 3.4.0, each instance has its own conversion pass, and every aligner child is read before the FIFOs are checked (`StreamedConversion::start`, `await_readers` in `aligner/stream.rs`).

## Links

- [usage/align-reads.md](../usage/align-reads.md): how the converted reads reach the aligner, and `--no_stream_converted`.
- [usage/deduplicate-alignments.md](../usage/deduplicate-alignments.md#errors): the `--umi_mismatches` errors.
- [troubleshooting/data-quality.md](data-quality.md): mapping efficiency, library type, coverage and M-bias.

## Open questions

- None.
