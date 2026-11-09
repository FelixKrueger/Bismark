# SPIKE — can the converted temp reads be streamed to the aligner instead of written?

**Issue:** [#1120](https://github.com/FelixKrueger/Bismark/issues/1120) · **Plan:** [`PLAN.md`](./PLAN.md) · **Progress:** [`PROGRESS.md`](./PROGRESS.md) · **Benchmarks:** [`BENCHMARKS.md`](./BENCHMARKS.md)
**Date:** 2026-09-11 · **Base:** `dev` `170bfce`
**Verdict:** **YES for Bowtie 2 and minimap2** — FIFOs are accepted for `-1/-2` and `-U`, and the alignment records are byte-identical, including under `-p 4 --reorder`, gzipped bytes, and two concurrent instances off independent FIFO pairs. **NO for HISAT2**, which rejects a FIFO outright (exit 255) and must keep the file path.

Everything below was verified against code or measured in a container. Citations are
`upstream/dev` @ `170bfce` unless stated. Nothing here is inference; where something is
unverified it says so.

## The problem

The aligner writes the in-silico converted reads (`_C_to_T` / `_G_to_A`) to `--temp_dir`
on every run. `--multicore N` adds chunk subsets (`.temp.N`). For disk-constrained users
this is the binding constraint — the run cannot start. `--gzip` takes ~5x off, never zero.

On `test_files` (5,000 read pairs, 436 KB gzipped across R1+R2) the converted pair is
2.3 MB — 5.3x the input as delivered.

## Why streaming works — the three load-bearing facts

1. **Conversion is a pure per-read map.** `convert_seq_c_to_t` / `convert_seq_g_to_a`
   (`rust/bismark/src/aligner/convert.rs:103`, `:149`) — stateless, no index, no lookback.

2. **The aligner is the only consumer of the converted bytes.** The methylation caller
   re-reads the *original* FastQ in lockstep (`aligner/mod.rs:3084`). `convert.rs` header:
   "The *original* (unconverted) read is deliberately NOT retained here — it is re-read in
   lockstep during the later methylation-call loop."

3. **The temp path cannot reach the output.** Bismark parses only the aligner's alignment
   records and writes its own header, `@PG ID:Bismark ... CL:"bismark <argv>"`
   (`aligner/output.rs:107`); Bowtie 2's own `@PG` is not propagated. So byte-identity holds
   **by construction**: identical bytes in, and the one SAM field recording the input path
   is discarded.

## Why it is a file today

Each converted file has exactly **two** concurrent readers, in every library mode.

- `se_instance_plan` (`mod.rs:1151`): directional `[(Norc, Ct, 0), (Nofw, Ga, 0)]` — both
  instances read file 0. Non-directional: `0,0,1,1`.
- `pe_instance_plan` (`mod.rs:4733`): directional slots s0 and s3, comment says "both read
  `-1 C→T_R1 -2 G→A_R2`".

Two readers rule out a single pipe. They do not rule out one pipe per reader.

## Aligner invocation

`align.rs:261` — argv is `<opts…> <orient> -x <index> -1 <input1> -2 <input2>`, spawned
with `Stdio::piped()` stdout (`align.rs:312`). Streaming keeps this argv shape exactly;
only the paths become FIFOs.

Conversion currently runs to completion **before** the aligner is spawned:
`process_se_chunk` (`mod.rs:1200`) calls `convert_se_files(...)`, prints the "Created …
converted version" banner, then spawns. This is why streaming is a wall-time *win* — it
takes a serial phase off the critical path.

## Validation (container, pinned versions)

Bowtie 2 2.5.5, HISAT2 2.2.2, minimap2 2.31 — the versions the repo `Dockerfile` pins for
byte-identity. Genome/reads are this repo's `test_files` (E. coli K12 `NC_010473`, 5,000
read pairs) against a C→T-converted index. **51.26%** overall alignment rate on the `--norc`
instance, so the alignment path is genuinely exercised (an earlier pass against an
unconverted lambda index gave 0.06% and proved nothing — do not repeat that mistake).

Comparisons exclude the `@PG` line only, per fact 3 above.

| Test | Result | Script |
|---|---|---|
| Bowtie 2 PE, FIFO per mate | byte-identical | `spikes/run.sh` B |
| Bowtie 2 PE, FIFO fed straight from the converter | byte-identical | `run.sh` C |
| Both directional instances concurrently, independent FIFO pairs | byte-identical (both) | `run.sh` D |
| Gzipped bytes over a FIFO | byte-identical (gz sniffed from pipe) | `run.sh` E |
| `-p 4 --reorder` over FIFOs | byte-identical | `run.sh` F |
| Bowtie 2 SE, `-U` over a FIFO | byte-identical | `spikes/run3.sh` |
| minimap2 SE over a FIFO | byte-identical | `run3.sh` |
| **HISAT2 PE and SE over a FIFO** | **fails, exit 255** | `run3.sh` |
| Tee fan-out, 1 pass to both instances, queue depth 1 / 4 / 64 | byte-identical at every depth | `spikes/run4.sh` |
| Tee fan-out, asymmetric consumers (`-p 1` vs `-p 4`, depth 1) | byte-identical, no starvation | `run4.sh` |
| Single writer filling mate 1 before mate 2 | **deadlock** (exit 124) | `run.sh` G |

### HISAT2 cannot be streamed

Rejects a FIFO for both `-1/-2` and `-U`:

```
Warning: Unsupported file format
(ERR): Read file 'h1' doesn't exist
Exiting now ...
```

Appears to stat or seek during format sniffing. `--hisat2` already reinterprets
`--multicore N` as single-instance `-p N` (`aligner/cli.rs:57`) so it writes no subsets;
its only disk lever stays `--gzip`. **The file path must be retained for HISAT2.**

## Scope boundaries

**uBAM / BINSEQ transcode FastQ** (`mod.rs:295-310`) is consumed **twice** — by the convert
step and the methylation-call re-read — so it genuinely has to be a file. Out of scope.

**`.temp.N` subsets** — out of scope. `--multicore 1 -p N` already avoids them and is the
recommended configuration (benchmarks page: pure `-p` is faster *and* 5.7x lighter on RAM
than pure `--multicore`).

**`--gzip` does not reach the subsets.** Written plain by design (`parallel.rs:151`: "NO
prefix, NO `.gz` — written plain so the converter's suffix-based gz detection reads them
plain"), where Perl compressed them when `--gzip` was set (`bismark:139`). So
`--gzip --multicore N` costs an extra uncompressed copy of the input vs Perl. **Do not
touch** — PR 1091's closing comment reserves the chunking path ("I'm going to take the
`--parallel` splitter double-inflate myself… please don't pick it up on my account") and it
must stay worker-invariant.

**Making `--gzip` parallel** — rejected. `convert.rs:284`, `:469`, `:591` use
single-threaded `flate2::GzEncoder` at `Compression::default()`, and `gzp` 0.11.3 +
`noodles-bgzf` 0.47.0 are already dependencies (`rust/Cargo.toml:32`, `:35`), so it would be
cheap. But a smaller file is still a file — it does not help the disk-constrained case — and
if streaming lands, its only remaining consumer is HISAT2.

**Retaining converted files for debugging** — rejected by the user. Never existed in Perl.

## Prior art

- Issue 1019 → PR 1022 (merged, shipped `v2.0.0-beta.4`): BGZF-compress the
  `--combined_index_sequential` spill, ~7.4x less scratch, wall-neutral. Same problem class
  one layer down: that made a scratch file smaller, this removes one. Sets the expected
  framing — measured table, "wall-neutral", byte-identical.
- PR 1091 (closed): parallel gzip *input* decode via `rapidgzip-core`. Touched `convert.rs`.
  Closed pending `rapidgzip-core` stabilising "or a genuinely decompression-bound path
  appearing". The tee design is 1 pass per mate, so it does **not** create one.
- Issue 1010: Perl is in maintenance freeze; new work targets the Rust suite.
- Issue 1025: added the uBAM/BINSEQ transcode FastQ (the scope boundary above).
- `--combined_index_sequential` is the precedent for the ship trajectory: landed
  experimental/opt-in, later became the default with the flag "retained as an explicit
  selector" (`cli.rs:268`).

## Upstream issue

https://github.com/FelixKrueger/Bismark/issues/1120 — filed from this research. Local copy:
`issue.md`. **Note:** it was written proposing opt-in and mentions retaining converted files;
both since superseded (see `PLAN.md` decisions). Needs an edit or a follow-up comment.
