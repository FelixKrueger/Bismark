# Parallel gzip input decode via `rapidgzip-core`

Branch: `feat/rapidgzip-input` (off `origin/master`, `ec5cb58`).

Upstream: [COMBINE-lab/rapidgzip-rust](https://github.com/COMBINE-lab/rapidgzip-rust),
published as [`rapidgzip-core` 0.1.0](https://crates.io/crates/rapidgzip-core).

## Goal and constraint

Reading a gzipped FastQ is currently serial: every `.gz` input in the suite goes
through a single-threaded `flate2::read::MultiGzDecoder`, so the aligner is fed
by one core's worth of inflate no matter how many cores the machine has. Replace
that with `rapidgzip-core`, a pure-Rust implementation of the rapidgzip
marker/window algorithm, on every `.gz` **input** path.

The constraint is the suite's standing invariant: **byte-identical to Perl
Bismark v0.25.1**. This change satisfies it trivially rather than by argument,
because the decoded byte stream is fixed by the gzip format and validated by
CRC32 + ISIZE. Nothing downstream can observe which decoder produced it. The
change is throughput-only.

## Scope

**In:** all 21 `.gz` input call sites, via one shared module.

| Area | Sites |
|------|-------|
| `aligner/mod.rs` | 6 |
| `aligner/convert.rs` | 4 |
| `aligner/genome.rs` | 1 |
| `genome_prep/convert.rs` | 1 |
| `io/genome.rs` | 1 |
| `bam2nuc/genome.rs` | 1 |
| `coverage2cytosine/{cov,genome}.rs` | 2 |
| `nome_filtering/nome.rs` | 1 |

**Out, deliberately:**

- **Gzip output.** `GzEncoder` / `gzp::ParCompress` are untouched. `rapidgzip-core`
  is a decoder only.
- **`bedgraph/input.rs`.** It uses `GzDecoder` (single-member), not
  `MultiGzDecoder`, matching Perl's behaviour at that specific site. Routing it
  through the multi-member decoder would change what a concatenated `.gz` yields
  there. Left alone on purpose.

## Design

One module, `bismark::io::gzread`, is the single entry point:

```rust
pub fn open_read(path: &Path) -> io::Result<Box<dyn BufRead + Send>>;
pub fn open_read_with_threads(path: &Path, threads: usize) -> io::Result<Box<dyn BufRead + Send>>;
```

Dispatch ladder:

1. no `.gz` suffix, plain `BufReader<File>`;
2. `.gz` + regular file + budget >= 4 + framing validates, `rapidgzip_core::DecoderReader`;
3. otherwise, `MultiGzDecoder` (the previous behaviour).

Step 3 is not a nicety. `rapidgzip_core::ReadAt` needs positional reads over a
stable-length source, so a FIFO, a process substitution (`<(zcat x.gz)`) or
`/dev/stdin` cannot use the parallel path. Falling back also preserves the
pre-existing error text for a file named `.gz` that is not gzip, since that error
is then raised by `MultiGzDecoder` exactly as before.

### Thread budget

`rapidgzip-core` defaults to `available_parallelism()` **per reader**. That is
wrong here: `--parallel N` runs N in-process workers (`aligner::parallel`), each
holding open 1 (SE) or 2 (PE) read streams, so the naive default would request
`N * streams * cores` decoder threads and starve the aligner it is feeding.

The budget is therefore process-global and installed exactly once, in
`aligner::pipeline` (the single funnel where both the worker count and the layout
are known, before any stream opens). Resolution order:

1. `BISMARK_GUNZIP_THREADS`;
2. the installed budget;
3. `min(8, available_parallelism())`.

Threading a parameter through 21 call sites was rejected: it would have touched
every signature for a value that is a property of the process, not of the call.

## Measurements

639 MB FastQ decoded, 16-core M4 Max, best of 3, `BISMARK_GUNZIP_THREADS`
forcing the budget. `threads=1` is the sequential `MultiGzDecoder` baseline.

Measured with the `MIN_PARALLEL_THREADS` gate **removed**, which is what
motivated adding it:

| threads | `gzip -6` | vs seq | `gzip -1` | vs seq |
|---------|-----------|--------|-----------|--------|
| 1 (seq) | 1048 MB/s | 1.00x  | 675 MB/s  | 1.00x  |
| 2       | 743 MB/s  | **0.71x** | 618 MB/s | **0.92x** |
| 3       | 1111 MB/s | 1.06x  | 911 MB/s  | 1.35x  |
| 4       | 1451 MB/s | 1.38x  | 1213 MB/s | 1.80x  |
| 8       | 1888 MB/s | 1.80x  | 2321 MB/s | 3.44x  |

The speculative decode does redundant work that only amortises once enough
workers run, so at 2 threads it is a **29 % regression** on ordinary `gzip -6`
input and 3 is break-even within noise. Hence `MIN_PARALLEL_THREADS = 4`: below
it, the sequential decoder is selected and this change can never be slower than
what it replaced.

With the gate in place (separate run, warm cache, so absolute numbers differ;
compare within the run):

| threads | `gzip -6` | vs seq |
|---------|-----------|--------|
| 1       | 1063 MB/s | 1.00x  |
| 2       | 1069 MB/s | 1.01x  |
| 3       | 1069 MB/s | 1.01x  |
| 4       | 1472 MB/s | 1.39x  |
| 8       | 2783 MB/s | 2.62x  |
| 12      | 3850 MB/s | 3.62x  |

1/2/3 now collapse onto the sequential baseline, confirming the gate routes them
away from the parallel decoder. `MAX_STREAM_THREADS = 8` is a budget choice, not
a plateau (scaling continues to 12): decoding faster than the aligner subprocess
can consume buys nothing and the threads are taken from the aligner.

## Tests

15 unit tests in `io::gzread`, covering the parity-critical cases:

- concatenated gzip members are **all** read (the `MultiGzDecoder`-vs-`GzDecoder`
  distinction the suite depends on);
- BGZF and single-member gzip round-trip;
- truncated gzip raises an `io::Error` on read rather than returning short
  output, which would silently drop reads;
- non-gzip bytes under a `.gz` name fail at read time, not open time (the
  pre-migration behaviour);
- an empty member yields no bytes; a missing file errors at open;
- a FIFO takes the sequential fallback;
- **decode is thread-budget invariant** across 1/2/3/8 threads, which is what
  keeps `--parallel` worker-invariance and byte-identity intact.

Full suite: 1439 lib tests + all integration tests pass in debug and release.

## Dependency

`rapidgzip-core = "=0.1.0"`, exact-pinned per the crate's convention. Its only
dependencies are `crossbeam-deque` and `libz-rs-sys`, i.e. the same zlib-rs
inflate backend `flate2` already uses here, so no new system dependency and no C
toolchain. MSRV 1.87, below the workspace's 1.89. Licensed BSD-3-Clause AND MIT,
recorded in `rust/THIRD-PARTY-NOTICES.md` (BSD-3-Clause was not previously
represented there).

## Known risk, stated plainly

`rapidgzip-core` 0.1.0 was published on 2026-08-01, the same day as this branch,
with a double-digit download count. Enabling it by default on every input path
puts a very new dependency in the critical path of the whole suite with no
compile-time opt-out.

The mitigations actually in the code are: the sequential fallback survives and is
reachable at runtime via `BISMARK_GUNZIP_THREADS=1`; any construction failure
falls back rather than aborting; and the decode is byte-checked by CRC32 + ISIZE.
A maintainer who would rather have a compile-time switch should ask for the
`[features]` gate (the pattern `rammap-inprocess` / `binseq-input` already use in
this crate); it is a small change and was not done here only because
default-on was the requested shape.
