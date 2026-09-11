<!-- DRAFT — not posted. Follow-up comment for
     https://github.com/FelixKrueger/Bismark/issues/1120 -->

Implemented on `rust/stream-converted`. Summary of what changed against the proposal above.

## What landed

Streaming is the default for Bowtie 2 and minimap2, with `--no_stream_converted` forcing the
file path. Converted footprint of a default run is zero.

The design is as proposed: one FIFO per aligner instance per mate, one conversion pass per
converted stream, fanned out over a bounded channel with one writer thread per FIFO. The argv
shape is unchanged; only the paths differ.

Runs that cannot stream take the file path and say why on STDERR: `--hisat2`, the
`--combined_index` models, `--rammap`, and a `--temp_dir` that will not host a FIFO.
`--illumina_5base` writes no converted reads at all.

## Two corrections to the proposal

**Non-directional PE is 4 conversion passes, not 2.** The issue says "4 instances = 8 FIFOs,
still 2 passes (the C→T and G→A sets derive from the same two inputs)". They derive from the
same inputs but they are four distinct converted streams — `m1 C→T`, `m2 G→A`, `m1 G→A`,
`m2 C→T` — and the current file path already writes four files for exactly that reason. So
streaming is pass-for-pass identical to today in every mode, which is the claim that matters;
the "2" was simply wrong.

**`--gzip` is inert on the streaming path.** It exists to shrink the converted temp file, and
with no file, compressing bytes that go straight into a pipe is CPU for nothing. The pipe
carries plain bytes and its name carries no `.gz`. Byte-identity under `--gzip` is covered.

## Two things the proposal did not anticipate

**`AlignerStream::spawn` had to be split.** It spawned the child *and* read through the SAM
header to the first alignment record. On a FIFO that read cannot complete until the writer is
released, and the writer is released only when the child opens the pipe — so priming instance
0 before instance 1 exists stalls the shared conversion pass on instance 1's unconsumed
channel, and the run deadlocks. `spawn` is now `spawn_unprimed` + `prime`, and the streaming
drivers spawn every child before reading any of them. The file path is unchanged.

**An aligner that never reads its input hung the run.** Each writer parks in its blocking
open-for-write, so a child that exits without opening its pipe leaves that writer parked
forever and joining it never returns. This is not hypothetical: the repo's own fake `bowtie2`
prints SAM records without reading a byte, and it deadlocked the entire `aligner_cli` suite.
Fixed by having each writer signal once its open has returned, and having the teardown open
and discard any pipe that has not signalled — safe there, because every aligner child has
already been reaped.

## Validation

A/B against the file path — same input, both arms, comparing `samtools view` records and the
report — across 11 shapes: SE and PE × directional / pbat / non-directional, `--gzip`, `-p 4`,
`--multicore 2`, `--skip`/`--upto`, `--prefix`. All BAM- and report-identical, and the
streaming arm's temp dir is empty at the end of every one.

Full-scale validation on real data is still to run.
