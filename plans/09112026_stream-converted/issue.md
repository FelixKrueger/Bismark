## Summary

The aligner writes the in-silico converted reads (`_C_to_T` / `_G_to_A`) to `--temp_dir` on every run, and `--multicore` adds the chunk subsets (`.temp.N`) on top. For users on constrained scratch this is the binding constraint: the run cannot start, however much CPU or RAM is available. `--gzip` takes roughly 5x off but never reaches zero.

Bowtie 2 and minimap2 both accept FIFOs for their read inputs and produce byte-identical alignment records, so the converted reads can be streamed instead of written. This proposes streaming as the default path, with the existing file path retained as a fallback. It measures as slightly faster than the current path, not slower, because it takes conversion off the critical path.

Targets the Rust suite (`upstream/dev` @ `170bfce`), per the Perl maintenance freeze in https://github.com/FelixKrueger/Bismark/issues/1010.

Same problem class as https://github.com/FelixKrueger/Bismark/issues/1019 (BGZF spill compression, shipped in PR https://github.com/FelixKrueger/Bismark/pull/1022), one layer up: that issue made a scratch file smaller, this one removes a scratch file.

## Current disk cost

Directional PE, the converted FastQ is roughly the uncompressed size of the input per converted set. On this repo's `test_files` (5,000 read pairs, 436 KB gzipped across R1+R2) the converted pair is 2.3 MB — 5.3x the input as delivered.

| | `--multicore 1` | `--multicore N` |
|---|---|---|
| `.temp.N` subsets | none | ~1x uncompressed input, always plain |
| converted FastQ | ~1x uncompressed input | ~1x uncompressed input |
| per-chunk BAMs | none | ~1x output |

All N chunks are live at once — workers spawn together under `std::thread::scope` (`rust/bismark/src/aligner/parallel.rs:675`) and each deletes its subset and converted files only after its own alignment finishes (`parallel.rs:440-444`), so per-chunk cleanup does not lower the peak.

`--multicore 1 -p N` is already the right advice for unrelated reasons (the benchmarks page has pure `-p` both faster and 5.7x lighter on RAM than pure `--multicore`) and it does remove the subsets. It still leaves one converted pair on disk, and there is currently no way to avoid that.

## Why streaming is tractable

Three properties of the existing code make this contained rather than a redesign.

**Conversion is a pure per-read map.** `convert_seq_c_to_t` / `convert_seq_g_to_a` (`aligner/convert.rs:103`, `:149`) are stateless — no index, no lookback, no second pass.

**The aligner is the only consumer of the converted bytes.** The methylation caller re-reads the *original* FastQ in lockstep (`aligner/mod.rs:3084`; `convert.rs` header: "The *original* (unconverted) read is deliberately NOT retained here — it is re-read in lockstep during the later methylation-call loop"). Nothing needs the converted file once the aligner has consumed it.

**The temp path cannot reach the output.** Bismark parses only Bowtie 2's alignment records and writes its own header, `@PG ID:Bismark ... CL:"bismark <argv>"` (`aligner/output.rs:107`); Bowtie 2's own `@PG` is not propagated. So byte-identity here holds by construction, not by argument: the bytes the aligner receives are identical, and the one SAM field that records the input path is discarded.

The reason it is a file today is that each converted file has exactly **two** concurrent readers, in every library mode — `se_instance_plan` (`mod.rs:1151`) is `[(Norc, Ct, 0), (Nofw, Ga, 0)]` for directional, both instances reading file 0, and `0,0,1,1` for non-directional; `pe_instance_plan` (`mod.rs:4733`) has directional slots s0 and s3 "both read `-1 C→T_R1 -2 G→A_R2`". Two readers rule out a single pipe. They do not rule out one pipe per reader.

## Proposed design

One FIFO per aligner instance per mate. One conversion pass per mate, fanned out to that mate's FIFOs through a bounded queue, one writer thread per FIFO. The argv shape stays exactly `-1 <path> -2 <path>` (`aligner/align.rs:261`), so nothing about how the aligner is invoked changes.

Directional PE is 2 instances x 2 mates = 4 FIFOs fed by 2 conversion passes. Non-directional is 4 instances = 8 FIFOs, still 2 passes (the C→T and G→A sets derive from the same two inputs).

CPU is one conversion pass per mate, the same as today. Memory is bounded by queue depth, not by read count. No re-read of the input is needed, so non-seekable inputs are unaffected.

Two constraints came out of testing and are not optional:

**One writer per mate is mandatory.** A single writer that fills mate 1's FIFO before starting mate 2 deadlocks — the aligner blocks reading mate 2 while the writer blocks on mate 1's 64 KB pipe buffer. Confirmed (timeout, exit 124).

**HISAT2 cannot be served this way** (see below), so it falls back to the file path under `--hisat2` — never silently.

## Validation

Bowtie 2 2.5.5, HISAT2 2.2.2, minimap2 2.31 — the versions the repo Dockerfile pins for byte-identity. Genome and reads are this repo's `test_files` (E. coli K12 `NC_010473`, 5,000 read pairs) against a C→T-converted index, 51.26% overall alignment rate on the `--norc` instance so the alignment path is genuinely exercised. Comparisons exclude the `@PG` line only, since it necessarily records the input path and Bismark does not propagate it.

| Test | Result |
|---|---|
| Bowtie 2 PE, FIFO per mate | byte-identical |
| Bowtie 2 PE, FIFO fed straight from the converter (zero converted bytes on disk) | byte-identical |
| Bowtie 2 SE, `-U` over a FIFO | byte-identical |
| Both directional instances concurrently, independent FIFO pairs | byte-identical (both) |
| Gzipped bytes over a FIFO | byte-identical (gz is sniffed from the pipe) |
| `-p 4 --reorder` over FIFOs, as Bismark invokes it | byte-identical |
| Tee fan-out, one pass to both instances, queue depth 1 / 4 / 64 blocks | byte-identical at every depth |
| Tee fan-out with asymmetric consumers (`-p 1` vs `-p 4`, depth 1) | byte-identical, no starvation |
| minimap2 SE over a FIFO | byte-identical |
| **HISAT2 PE and SE over a FIFO** | **fails, exit 255** |
| Single writer filling mate 1 before mate 2 | deadlock (exit 124) |

## Performance

Conversion currently runs to completion before the aligner is spawned (`process_se_chunk` calls `convert_se_files` then spawns — `mod.rs:1200`), so it sits on the critical path. Streaming overlaps it with alignment. 300,000 read pairs, both directional instances, `-p 4`, three reps:

| | convert | align | total | converted files on disk |
|---|---|---|---|---|
| Serial (current shape) | 472-605 ms | 4148-4210 ms | 4621-4815 ms | 139 MB |
| Streamed | overlapped | — | 4174-4355 ms | 0 |

About 10% faster here. FIFO transport itself showed no measurable overhead in isolation (106-110 ms vs 107-109 ms, `-p 4`, mean of 5).

Treat the 10% as an upper bound rather than a forecast: the stand-in converter is `awk`, and the real Rust converter is faster, which shrinks the serial phase being hidden; and E. coli makes alignment cheap relative to a human index. The defensible claim is **wall-neutral to modestly faster, never slower** — the change removes a serial phase rather than adding work.

## Suggested scope

Streaming becomes the default path for Bowtie 2 and minimap2, with `--no_stream_converted` (name up for grabs) as an escape hatch that forces the current file path — needed to A/B the two paths in tests and to bisect if something unexpected appears.

The file path is retained, not removed, and is selected automatically when streaming is unavailable: under `--hisat2`, and when `mkfifo` is unavailable or `--temp_dir` cannot host a FIFO (network scratch is exactly where users point `--temp_dir`). That fallback must be never-silent — a silent one would hide the disk cost the feature exists to remove.

Default-on rather than opt-in on the understanding that full-scale byte-identity validation gates the merge; the flag exists so that validation can compare both paths directly. `--combined_index_sequential` is the precedent for a mode that landed opt-in and later became the default with the flag "retained as an explicit selector" (`cli.rs:268`) — this starts one step further along, because the file path never goes away.

Out of scope, deliberately:

**The `.temp.N` subsets.** `--multicore 1 -p N` already avoids them and is the recommended configuration anyway.

**The uBAM / BINSEQ transcode FastQ** (`mod.rs:295-310`), which is consumed twice — by the convert step and the methylation-call re-read — so it genuinely has to be a file.

**Making `--gzip` parallel.** It currently uses single-threaded `flate2::GzEncoder` at `Compression::default()` (`convert.rs:284`, `:469`, `:591`), and `gzp` / `noodles-bgzf` are already dependencies, so a faster writer would be cheap. But it does not help the disk-constrained case — a smaller file is still a file — and if streaming lands its only remaining consumer is HISAT2. Noted, not proposed.

## Two things to record while they are in view

Neither is a work item here.

**HISAT2 has no path to zero.** It rejects a FIFO for both `-1/-2` and `-U`:

```
Warning: Unsupported file format
(ERR): Read file 'h1' doesn't exist
Exiting now ...
```

It appears to stat or seek the input during format sniffing. Since `--hisat2` already reinterprets `--multicore N` as single-instance `-p N` threading (`aligner/cli.rs:57`) and so writes no subsets, HISAT2 users are left with one converted set on disk and `--gzip` as their only lever.

**`--gzip` does not reach the `.temp.N` subsets.** They are written plain by design (`parallel.rs:151`: "NO prefix, NO `.gz` — written plain so the converter's suffix-based gz detection reads them plain"), where Perl compressed them when `--gzip` was set (`bismark:139`). So `--gzip --multicore N` currently costs an extra full uncompressed copy of the input relative to Perl. Flagging it only — that is the chunking path PR https://github.com/FelixKrueger/Bismark/pull/1091's closing comment reserves, and it needs to stay worker-invariant.

<details>
<summary>Reproduction</summary>

Container, using the Dockerfile's pinned versions:

```dockerfile
FROM mambaorg/micromamba:1.5.8-bookworm-slim
USER root
RUN micromamba install -y -n base -c bioconda -c conda-forge \
      bowtie2=2.5.5 hisat2=2.2.2 minimap2=2.31 python=3.12 \
    && micromamba clean --all --yes
ENV PATH=/opt/conda/bin:$PATH
WORKDIR /work
```

Inputs are `test_files/NC_010473.fa.gz`, `test_files/test_R1.fastq.gz` and `test_files/test_R2.fastq.gz` from this repo. The genome is C→T converted; reads are C→T (R1) and G→A (R2) converted; Bowtie 2 gets a Bismark-shaped option set (`-q --phred33 -L 20 --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --dovetail --minins 0 --maxins 500`) plus `--norc` / `--nofw` per instance. The core check:

```bash
mkfifo c1 c2
convert R1.fq.gz c1 ct &   # streams converted records into the FIFO
convert R2.fq.gz c2 ga &
bowtie2 $OPTS --norc -x idx -1 c1 -2 c2 -S fifo.sam
cmp <(grep -v '^@PG' file.sam) <(grep -v '^@PG' fifo.sam)
```

Full scripts (including the tee fan-out with bounded queues, and the timing harness) available on request.
</details>
