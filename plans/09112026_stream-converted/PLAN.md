# Plan — stream the converted temp reads through FIFOs

Read `SPIKE.md` first for the verified facts. Progress and the decisions log live in [`PROGRESS.md`](./PROGRESS.md) (repo convention);
this file is the design and the phase breakdown.

## Goal

Remove the `_C_to_T` / `_G_to_A` temp files from the aligner's disk footprint by handing the
aligner FIFOs instead of files. Byte-identical BAM output, by construction.

## Decisions

Canonical list with attribution and dates is in [`PROGRESS.md`](./PROGRESS.md). Summary:

| Decision | Rationale |
|---|---|
| **Streaming is the DEFAULT**, not opt-in | No performance regression measured; the user will run full validation before merge. Default-off-first is not required within the working effort. |
| Escape hatch: `--no_stream_converted` forces the file path | Needed to A/B the two paths in tests, and to bisect if something odd appears. |
| Fall back to files automatically when streaming is unavailable | `--hisat2` (rejects FIFOs, verified); `mkfifo` unavailable or `--temp_dir` on a filesystem that will not host a FIFO. |
| Fallback must be **never-silent** | Repo convention throughout `cli.rs`. A silent fallback would hide the disk cost the feature exists to remove. |
| **No** "retain converted files" option | Rejected by the user. Never existed in Perl. |
| **No** parallel-`--gzip` work | A smaller file is still a file; does not help the disk-constrained case. See SPIKE.md. |
| **Do not touch** the `.temp.N` subset path (`parallel.rs`) | Reserved by PR 1091's closing comment; must stay worker-invariant. |
| Asymmetric-consumer benchmarking deferred | User will do it once the work is complete. Record the gap, do not block on it. |

## Design

One FIFO per aligner instance per mate. One conversion pass per mate, fanned out to that
mate's FIFOs through a bounded channel, one writer thread per FIFO.

```
R1.fq.gz ──> convert C→T ──> [bounded chan] ──> writer ──> fifo_s0_m1 ──> bowtie2 (--norc)
                         └──> [bounded chan] ──> writer ──> fifo_s3_m1 ──> bowtie2 (--nofw)
R2.fq.gz ──> convert G→A ──> [bounded chan] ──> writer ──> fifo_s0_m2 ──┘ (same 2 children)
                         └──> [bounded chan] ──> writer ──> fifo_s3_m2 ──┘
```

Directional PE: 2 instances x 2 mates = 4 FIFOs, 2 conversion passes.
Non-directional PE: 4 instances = 8 FIFOs, **4** conversion passes — each mate is converted
both ways (`m1 C→T`, `m2 G→A`, `m1 G→A`, `m2 C→T`), which is exactly what the file path
already does (`pe_conv_sources`). (An earlier draft of this plan said 2; that was wrong.)
Directional SE: 2 instances x 1 mate = 2 FIFOs, 1 pass.
Non-directional SE: 4 instances = 4 FIFOs, 2 passes.

CPU is one conversion pass per *converted stream* — the same work as today, unit for unit. Memory is bounded by channel
depth. No input re-read, so non-seekable inputs are unaffected.

### Non-negotiable ordering (deadlock discipline)

1. `mkfifo` all paths.
2. Start the writer threads. Each parks in its blocking open-for-write; because they are
   threads, parking is harmless and the main thread carries on.
3. Spawn **all** aligner children (each opens its FIFOs for read, releasing its writer).
4. Only **then** read from any child.

Opening a FIFO for write blocks until a reader opens, and an aligner emits no records until
it has read some reads — so step 4 is not optional. `AlignerStream::spawn` used to do steps 3
and 4 together, which meant priming instance 0 before instance 1 existed; the shared
conversion pass then stalled on instance 1's unconsumed channel and the run deadlocked. Hence
`spawn_unprimed` + `prime` (D-9).

**One writer thread per FIFO** — verified: a single writer that fills mate 1 before starting
mate 2 deadlocks on the 64 KB pipe buffer (exit 124).

**A child that never opens its pipe must not hang the run.** Each writer signals once its
open has returned; `finish`/`drop` opens and discards any pipe that has not signalled. Safe at
both call sites, because every child has been reaped by then. This is not hypothetical — the
repo's own fake `bowtie2` prints SAM records without reading a byte, and it deadlocked the
entire `aligner_cli` suite before the fix.

### `mkfifo` mechanism

Shell out to `mkfifo` once per run via `Command`. Chosen over adding a `libc` dependency
(libc 0.2.186 is already in `Cargo.lock` transitively, so declaring it is cheap — but it
would still touch `rust/THIRD-PARTY-NOTICES.md`, and shelling out is a few lines). A missing
or failing `mkfifo` is exactly the fallback condition we need anyway, so this makes the
fallback path meaningful rather than dead code.

### The `count` banner

`ConvertedReads.count` feeds **only** the stderr banner "Created … converted version of … (N
sequences)" — all 8 call sites verified (`mod.rs:1206`, `:3563`, `:3862`, `:4287`, `:4520`,
`:4819`, `:5974`, `:6534`). It does **not** feed the report, and `conv_label`'s doc comment
says the banner is "STDERR only (not byte-gated)". So streaming may emit the count after the
stream drains, or reword the banner. Not a byte-identity risk.


## Code map

`aligner/mod.rs` is ~7.7k lines. These are the sites this work touches, so a session does not
have to re-navigate it.

**Conversion + instance plans**

| Site | What |
|---|---|
| `convert.rs:103`, `:149` | `convert_seq_c_to_t` / `convert_seq_g_to_a` — the pure per-read map. **Reuse unchanged; conversion logic must not fork.** |
| `convert.rs:284`, `:469`, `:591` | the three `GzEncoder` write sites (`--gzip`) |
| `mod.rs:1127` | `convert_se_files` — per-mode converted-file set |
| `mod.rs:1150` | `se_instance_plan` — which instance reads which converted file |
| `mod.rs:4733` | `pe_instance_plan` — same, PE |
| `align.rs:261`, `:312` | aligner spawn; argv `-1 <p> -2 <p>`, stdout piped |
| `output.rs:107` | `generate_sam_header` — Bismark's own `@PG`; Bowtie 2's is discarded |

**Chunk drivers — there are 10, not 2.** Phase 3 targets the first two; the eight
combined-index variants are Phase 6.

| Site | Driver | Phase |
|---|---|---|
| `mod.rs:1188` | `process_se_chunk` | 3 |
| `mod.rs:4785` | `process_pe_chunk` | 3 |
| `mod.rs:3544` | `process_se_chunk_combined` | 6 |
| `mod.rs:3844` | `process_se_chunk_combined_nondir` | 6 |
| `mod.rs:4268` | `process_se_chunk_combined_nondir_sequential` | 6 |
| `mod.rs:4501` | `process_se_chunk_combined_nondir_tagged` | 6 |
| `mod.rs:5601` | `process_pe_chunk_combined` | 6 |
| `mod.rs:5944` | `process_pe_chunk_combined_nondir` | 6 |
| `mod.rs:6502` | `process_pe_chunk_combined_nondir_sequential` | 6 |
| `mod.rs:6728` | `process_pe_chunk_combined_nondir_tagged` | 6 |

`run_pe_five_base` (`mod.rs:1766`) aligns to the **unconverted** genome, so it has no converted
temp files and is out of scope entirely.

The count of drivers is the main scope risk in this plan: the common return shape introduced in
Phase 3 has to be one the other eight can adopt without reshaping each of them, or Phase 6
becomes eight bespoke edits.

## Phases

### Phase 0 — environment ✅
- [x] Branch `rust/stream-converted` off `upstream/dev` @ `170bfce`
- [x] `spikes/dev.sh` — cargo in Docker (no local toolchain), registry + target in named volumes
- [x] Debug build green; `bismark --version` → `v3.1.0 (170bfce)`
- [x] Validation scripts carried over from research (`spikes/`)

### Phase 1 — FIFO plumbing ✅
- [x] New module `rust/bismark/src/aligner/stream.rs`
- [x] `FifoSet`: create N FIFOs under `--temp_dir`, hand out paths, unlink on `Drop` and on error
- [x] Probe helper `temp_dir_supports_fifo` (create + unlink one); called from `run`, not `resolve`
- [x] Unit tests: create/unlink round-trip, atomicity on failure, probe true/false, name scoping

### Phase 2 — the streaming converter ✅
- [x] `convert_fastq_impl` / `convert_fasta_impl` split into a **sink-generic core**
      (`convert_fastq_into` / `convert_fasta_into`) plus a thin file wrapper. The streaming
      path reuses the same loop verbatim — the conversion logic is not forked, and the
      existing 46 `convert` unit tests still pass unchanged (the byte-freeze)
- [x] One converter thread per source, `sync_channel(4)` of 256 KiB blocks per consumer
- [x] One writer thread per FIFO
- [x] `--skip` / `--upto` come free: the shared core applies them
- [x] Errors propagate instead of hanging; a `BrokenPipe` write ends the writer quietly so the
      child's exit status is the diagnostic
- [x] Unit tests: streamed bytes == the file the converter would have written (both consumers,
      one pass); PE fan-out (2 passes, 4 pipes); `--gzip` inert; no hang when nothing reads

### Phase 3 — wire into the chunk drivers ✅
- [x] Common return shape: both `process_*_chunk` keep their existing signature and the
      streaming path returns an **empty** cleanup vector — so `run_se`/`run_pe`/`parallel.rs`
      need no change at all
- [x] `process_se_chunk_streamed` / `process_pe_chunk_streamed`, routed on `config.stream_converted`
- [x] Spawn-all-then-prime ordering (needed `AlignerStream::spawn_unprimed` — see D-9)
- [x] Combined-index / 5-base / tagged-interleaved stay on files, via the config gate

### Phase 4 — CLI and config ✅
- [x] `cli.rs`: `--no_stream_converted`
- [x] `config.rs`: `StreamConverted { enabled, reason }`, resolved by the pure
      `resolve_stream_converted` (no I/O), narrowed in `run` by the FIFO probe
- [x] Never-silent notice naming the fallback reason; `config.summary()` states the path taken

### Phase 5 — byte-identity tests ✅
- [x] `tests/aligner_stream_converted.rs` — routing matrix (always runs) + an A/B that prepares
      an E. coli index and compares BAM records (skips where Bowtie 2 is absent, as CI is)
- [x] Container matrix `spikes/ab-run.sh` — 11 shapes, SE+PE × directional/pbat/non-directional,
      `--gzip`, `-p 4`, `--multicore 2`, `--skip`/`--upto`, `--prefix`. All BAM- and
      report-identical, and the streaming arm's temp dir is empty
- [x] Asserts zero `_C_to_T` / `_G_to_A` / FIFO files survive on the streaming path

### Phase 6 — remaining modes ✅ (decided)
- [x] Bowtie 2 + minimap2 stream. HISAT2, combined-index and rammap take files **with a stated
      reason**; 5-Base has no converted temps at all
- [x] Every mode left on files says so (the `run` notice + the summary line)
- Deferred: extending streaming to the combined-index drivers (D-11) and feeding in-process
  rammap directly with no pipe at all (see PROGRESS Open)

### Phase 7 — finish
- [x] `cargo fmt` / `clippy` clean on every touched file
- [x] Docs: `--no_stream_converted` in `docs/.../options/alignment.md`
- [x] CHANGELOG entry
- [ ] `just ci` on the pinned 1.89 toolchain
- [ ] Update issue 1120: default rather than opt-in, retain-files dropped
- [ ] Hand to the user for full-scale + asymmetric-consumer validation

## Open questions

- ~~Non-directional fan-out degree~~ — confirmed by construction: the drivers now derive the
  conversion set from `se_conv_kinds` / `pe_conv_sources`, the same helpers the file path uses,
  so the two cannot disagree. Non-directional PE = 4 sources, 8 pipes; non-directional SE =
  2 sources, 4 pipes.
- ~~Streaming banner wording~~ — `Streaming <kind> converted version of <input> to N aligner
  instance(s)` before the aligners start, and `Streamed … (N sequences, no temp file written)`
  once the stream has drained, so the count still reaches the user.
- ~~`--reorder` vs channel depth~~ — `-p 4` is byte-identical at depth 4 in the container
  matrix (`pe_threads4`).
- **Still open:** wall time under asymmetric consumers (D-6, user-owned). `CHANNEL_DEPTH` in
  `stream.rs` is the knob.
