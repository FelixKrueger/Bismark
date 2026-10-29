<!-- Posted as a comment on https://github.com/FelixKrueger/Bismark/issues/1120 -->

Implemented in #1121. Summary of what changed against the proposal above.

## What landed

Streaming is the default for Bowtie 2 and minimap2, with `--no_stream_converted` forcing the file path. The converted footprint of a default run is zero.

The design is as proposed: one FIFO per aligner instance per mate, one conversion pass per converted stream, fanned out over a bounded channel with one writer thread per FIFO. The argv shape is unchanged; only the paths differ.

Runs that cannot stream take the file path and say why on STDERR: `--hisat2`, the `--combined_index` models, `--rammap`, and a `--temp_dir` that will not host a FIFO. `--illumina_5base` writes no converted reads at all.

## Three corrections to the issue above

**The ~10% speedup in the Performance section did not reproduce.** That measurement used an `awk` stand-in converter; the real Rust converter is fast enough that the serial phase being hidden is nearly free, so there is almost nothing left to win. Measured on the shipped code — 200,000 pairs, 93 MB uncompressed, `-p 4`, 7 reps with the arms interleaved — wall time is a wash:

| | wall, median of 7 | peak converted bytes in `--temp_dir` |
|---|---|---|
| Files (`--no_stream_converted`) | 6.35 s | **96 MB** |
| Streamed (default) | 6.33 s | **4 KB** |

The disk result is the point and it is unambiguous. The 4 KB is one FIFO inode per aligner instance and does not grow with read count; the 96 MB does. The honest performance claim is **wall-neutral**, not faster, and the PR says so.

**Non-directional paired-end is 4 conversion passes, not 2.** The issue says "4 instances = 8 FIFOs, still 2 passes (the C→T and G→A sets derive from the same two inputs)". They derive from the same inputs but they are four distinct converted streams — `m1 C→T`, `m2 G→A`, `m1 G→A`, `m2 C→T` — and the current file path already writes four files for exactly that reason. So streaming is pass-for-pass identical to today in every mode, which is the claim that matters; the "2" was simply wrong.

**`--gzip` is inert on the streaming path.** It exists to shrink the converted temp file, and with no file, compressing bytes that go straight into a pipe is CPU for nothing. This costs nothing, because `--gzip` has never affected an output file in either implementation: in the Rust aligner it feeds only `ConvertOptions`, and in Perl its only other use is the `.temp.N` subset *names* (`bismark:619-768`). The BAM is BGZF regardless, and the `--unmapped`/`--ambiguous` FastQ are gzipped unconditionally in both (`bismark:1293`, `:1368`). Byte-identity under `--gzip` is covered by the A/B.

## Two things the proposal did not anticipate

**`AlignerStream::spawn` had to be split.** It spawned the child *and* read through the SAM header to the first alignment record. On a FIFO that read cannot complete until the writer is released, and the writer is released only when the child opens the pipe — so priming instance 0 before instance 1 exists stalls the shared conversion pass on instance 1's unconsumed channel, and the run deadlocks. `spawn` is now `spawn_unprimed` + `prime`, and the streaming drivers spawn every child before reading any of them. The file path is unchanged.

**An aligner that never reads its input hung the run.** Each writer parks in its blocking open-for-write, so a child that exits without opening its pipe leaves that writer parked forever and joining it never returns. This is not hypothetical: the repo's own fake `bowtie2` prints SAM records without reading a byte, and it deadlocked the entire `aligner_cli` suite. Fixed by having each writer signal once its open has returned, and having teardown open and discard any pipe that has not signalled — safe there, because every aligner child has already been reaped.

## Validation

A 14-shape A/B against the file path — same input, both arms, comparing `samtools view` records and the report — across SE and PE × directional / pbat / non-directional, FastQ **and** FastA, `--gzip`, `-p 4`, `--multicore 2`, `--skip`/`--upto`, `--prefix`. All BAM- and report-identical, and the streaming arm's temp dir is empty at the end of every one.

Still to do: full-scale validation on real data, and wall time under asymmetric consumers (a `-p 1` instance beside a `-p 4` one), since a bounded fan-out couples its consumers. Proven deadlock-free and byte-identical; the wall-time behaviour under that asymmetry is unmeasured.

## Two scope notes

**Combined-index streaming is deferred, not blocked.** I checked all eight combined-index drivers for the thing that would prevent it — a converted file read more than once, which a pipe cannot serve — and none of them do it. It is deferred on cost and risk: eight more drivers for an opt-in mode, two further conversion cores, and `--combined_index_sequential`'s whole purpose is minimising peak RSS by making pass 1's aligner exit before pass 2 spawns, which deserves its own design rather than a copy-paste.

**In-process `--rammap` could skip the converted bytes entirely** rather than stream them: `inprocess.rs` consumes converted reads one at a time from a `BufRead`, so the converter could feed it with no pipe at all. A better optimisation, and a separate one.

Finally: this issue's **title still says "opt-in"**, which the body no longer does. Worth retitling if you agree with the default-on framing.
