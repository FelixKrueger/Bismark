# PROGRESS — stream the converted temp reads through FIFOs (#1120)

**Plan:** [`PLAN.md`](./PLAN.md) · **Spike:** [`SPIKE.md`](./SPIKE.md) · **Benchmarks:** [`BENCHMARKS.md`](./BENCHMARKS.md) · **Issue:** [#1120](https://github.com/FelixKrueger/Bismark/issues/1120)
**Last updated:** 2026-09-11 (implementation session) · **Base:** `dev` `170bfce` · **Branch:** `rust/stream-converted`

**Status legend:** 📋 Planned · 🔨 In progress · ✅ Done · ⛔ Blocked · ⏸ Awaiting user

| # | Pipeline step | Status | Notes |
|---|---|---|---|
| 1 | Triage / research | ✅ Done | `SPIKE.md`. FIFOs validated byte-identical for Bowtie 2 (SE+PE, gz, `-p 4 --reorder`, concurrent instances) and minimap2. **HISAT2 rejects FIFOs** (exit 255) — file path must stay. Single-writer-per-mate-pair deadlocks (exit 124) |
| 2 | Benchmark the premise | ✅ Done | `BENCHMARKS.md`. Streaming is ~10% **faster**, not slower: conversion currently completes before the aligner is spawned (`mod.rs:1200`), so streaming takes a serial phase off the critical path |
| 3 | Issue filed | ✅ Done | [#1120](https://github.com/FelixKrueger/Bismark/issues/1120). Edited 2026-09-11 to default-on framing per D-1/D-2 (filed as opt-in) |
| 4 | Plan written (rev 0) | ✅ Done | `PLAN.md` |
| 5 | Environment | ✅ Done | Local cargo 1.98.1 (installed 2026-09-11); debug build green in ~11 s, `bismark --version` → `v3.1.0 (170bfce)`. `spikes/dev.sh` keeps a pinned-1.89 Docker path for the reproducibility gate |
| 6 | Manual review (user) | ⏸ Awaiting user | Plan rev 0 not yet read |
| 7 | Agent review (dual `plan-reviewer`) | ⏸ Awaiting user | Repo convention for this workflow; **not run — ask before spawning agents** |
| 8 | Implement — Phase 1 FIFO plumbing | ✅ Done | `stream.rs`: `FifoSet` create/unlink/probe + `fifo_name`. 5 unit tests |
| 9 | Implement — Phase 2 streaming converter | ✅ Done | `convert_fastq_impl`/`convert_fasta_impl` split into a sink-generic core (`convert_fastq_into` / `convert_fasta_into`) reused verbatim by both paths — the conversion loop is NOT forked. 1 converter thread per source, 1 writer thread per consumer, bounded channel |
| 10 | Implement — Phase 3 chunk drivers | ✅ Done | `process_se_chunk_streamed` / `process_pe_chunk_streamed`; both `process_*_chunk` route on `config.stream_converted.enabled` and return an empty cleanup vec |
| 11 | Implement — Phase 4 CLI + config | ✅ Done | `--no_stream_converted`; `config::StreamConverted { enabled, reason }` via the pure `resolve_stream_converted`; FIFO probe + never-silent notice in `run` |
| 12 | Implement — Phase 5 byte-identity tests | ✅ Done | `tests/aligner_stream_converted.rs` (8 tests: routing matrix + an A/B that prepares an E. coli index and compares BAM records). Container matrix in `spikes/ab-run.sh`: **14 shapes, all BAM- and report-identical**, FastQ + FastA |
| 13 | Implement — Phase 6 remaining modes | ✅ Decided | Combined-index, rammap and HISAT2 stay on files with a stated reason; 5-Base has no converted temps. Extending streaming to combined-index is deliberately deferred (see Open) |
| 14 | `just ci` green | ✅ Done | On the pinned 1.89 in Docker: fmt ✅ · clippy ✅ · test 1513 pass / 2 fail · test-release 1512 pass / 2 fail. **Both failures are pre-existing** on the unmodified base (verified by stashing): `bam2nuc::freqs::…readonly` (the container runs as root, so a read-only dir is still writable) and `inprocess::…rejects_pre_clipped_core_in_debug` (a `should_panic` `debug_assert`, which `--release` disables). macOS debug run: 60/60 binaries green |
| 15 | Verify (code review) | ✅ Done | `plan-reviewer`/`code-reviewer` do not exist in a fresh clone (no `.claude/` in the repo — raised on the PR), so the `/code-review` skill ran against PR #1121 at high effort instead. **5 findings: 1 HIGH (silent hang), 4 LOW.** All fixed bar the per-chunk `mkfifo` fallback (diagnostic improved instead — see Open) |
| 16 | Full-scale + asymmetric-consumer validation | 📋 User | User owns this (D-6) |
| 18 | Measured cost/benefit | ✅ Done | `spikes/bench-ab-run.sh`. 200,000 pairs (93 MB uncompressed), `-p 4`: peak converted bytes on disk **4 KB streamed vs 96 MB files**; wall time a wash (see `BENCHMARKS.md`) |
| 17 | Commit + PR → `dev` | ✅ Done | [PR #1121](https://github.com/FelixKrueger/Bismark/pull/1121) → `dev`, from `ewels:rust/stream-converted` (no push access to upstream). CI fully green incl. `perl-oracle byte-identity`. Follow-up posted on [#1120](https://github.com/FelixKrueger/Bismark/issues/1120) |

## Resolved decisions

- **D-1** (user, 2026-09-11) — **streaming is the DEFAULT**, not opt-in. The user runs full validation before merge, so default-off-first is not required within the working effort. `--no_stream_converted` is the escape hatch (needed to A/B the paths in tests and to bisect).
- **D-2** (user, 2026-09-11) — automatic **fallback to files** when streaming is unavailable: `--hisat2`, or `mkfifo` unavailable / `--temp_dir` cannot host a FIFO. Must be **never-silent** (repo convention, and a silent fallback would hide the disk cost the feature exists to remove).
- **D-3** (user, 2026-09-11) — **no** parallel-`--gzip` work. A smaller file is still a file, so it does not help the disk-constrained case; and if streaming lands its only remaining consumer is HISAT2. Proposed and withdrawn before filing.
- **D-4** (user, 2026-09-11) — **no** "retain converted files" option. Never existed in Perl.
- **D-5** (research, 2026-09-11) — **do not touch** `parallel.rs` (the `--multicore` chunking path). PR [#1091](https://github.com/FelixKrueger/Bismark/pull/1091)'s closing comment reserves it and it must stay worker-invariant. This leaves the `--gzip`-on-subsets gap unfixed, deliberately.
- **D-6** (user, 2026-09-11) — asymmetric-consumer benchmarking deferred to the user, once the work is complete. Record the gap; do not block on it.
- **D-7** (2026-09-11) — `mkfifo` via `Command` rather than declaring `libc`. libc 0.2.186 is already in `Cargo.lock` transitively, but declaring it would touch `rust/THIRD-PARTY-NOTICES.md`, and a missing `mkfifo` is exactly the fallback condition D-2 needs — so shelling out makes the fallback meaningful instead of dead code.

- **D-8** (2026-09-11, implementation) — **`--gzip` is inert on the streaming path.** It exists to shrink the converted temp *file*; with no file, compressing bytes that go straight into a pipe is CPU for nothing. The FIFO therefore carries plain bytes and its name carries no `.gz`. Proven byte-identical under `--gzip` in the container matrix.
- **D-9** (2026-09-11, implementation) — **`AlignerStream::spawn` had to be split into `spawn_unprimed` + `prime`.** `spawn` read through the header to the first alignment record, which on a FIFO cannot complete until the writer is released, which cannot happen until the child opens the pipe. Priming instance 0 before instance 1 exists stalls the shared conversion pass on instance 1's unconsumed channel and deadlocks. Both halves are newtype-separated (`UnprimedAlignerStream`), because an unprimed stream's `current: None` is indistinguishable from EOF and would silently drop every alignment. The file path is unchanged (`spawn` = the two chained).
- **D-10** (2026-09-11, implementation) — **the FIFO name keeps the format extension last** (`…_C_to_T.s0.4242.fifo.fastq`). Bowtie 2 and minimap2 sniff content, but the repo's own test stubs (and other tooling) pick the input argument off its `.fastq`/`.fq` suffix. Free to get right.
- **D-11** (2026-09-11, implementation) — **combined-index streaming is deferred, not rejected.** The eight combined-index drivers would each need the spawn/prime split threading through them, for an opt-in mode; the default path is what the disk constraint bites on. `resolve_stream_converted` sends them to files with a stated reason.

- **D-12** (2026-09-11, code review) — **only a NON-ZERO exit before opening is fatal** in `await_readers`. A child that exits cleanly without reading has produced whatever it was going to produce, so the run continues. Treating every early exit as fatal broke eight PE tests: the repo's fake aligners print SAM records without consuming a byte. The container A/B cannot catch this — real aligners always open their inputs — so the unit tests are the only guard.
- **D-13** (2026-09-11, code review) — **the writer-open signal is an `AtomicU8`, not a channel.** Two readers need it (the pre-prime handshake and the teardown) and a one-shot channel gives the answer to whichever asks first. Paired with `JoinHandle::is_finished()`, which also covers a writer that panicked before publishing state — something the channel version silently did not.

## Open (non-blocking)

- **Asymmetric-consumer wall time is unmeasured.** Files fully decouple the two aligner instances; a tee couples them, so a fast instance can be throttled by the bounded channel behind a slow one. Proven: no deadlock, no starvation, byte-identical at depth 1 under `-p 1` vs `-p 4`. Not proven: wall time under that asymmetry. Channel depth is the mitigation knob. (D-6)
- **`--gzip` does not reach the `.temp.N` subsets** (`parallel.rs:151`), where Perl compressed them (`bismark:139`) — so `--gzip --multicore N` costs an extra uncompressed copy of the input vs Perl. Out of scope per D-5; worth its own issue once `parallel.rs` is free.
- **HISAT2 has no path to zero temp bytes.** Its only lever stays `--gzip`. Known ceiling, not a work item.
- **A per-chunk `mkfifo` failure aborts instead of falling back to files.** Reachable after a successful start-up probe: the probe uses a short fixed name, but the real FIFO name adds `.s<slot>.<pid>.fifo` (and `.temp.<i>` again under `--multicore`), so a long sample basename that worked as a file can exceed 255 bytes as a pipe. A temp dir that fills or goes read-only mid-run does the same. The error now names `--no_stream_converted`, but the documented promise is automatic fallback. The proper fix means restructuring `process_*_chunk` so the file-path body is callable after a streaming start fails.
- **Combined-index modes still write converted files** (D-11). Worth a follow-up once someone needs it; the return shape (`process_*_chunk_streamed`) is ready to be copied.
- **`--rammap` in-process could skip the converted bytes entirely**, not merely stream them: `inprocess.rs` consumes converted reads one at a time from a `BufRead`, so the converter could feed it directly with no pipe at all. A better optimisation than streaming, and a separate one.
- **A bounded fan-out couples its consumers.** An aligner instance that stops reading fills its channel and stalls the shared conversion pass, starving its siblings. Harmless for real aligners (they all read their input) and resolved after the children are reaped by `drain_unopened`, but it is the mechanism behind the unmeasured asymmetric wall-time question above.

## Key facts established in triage

- **2026-09-11** — `ConvertedReads.count` feeds **only** the stderr banner; all 8 call sites verified (`mod.rs:1206`, `:3563`, `:3862`, `:4287`, `:4520`, `:4819`, `:5974`, `:6534`). `conv_label`'s doc says the banner is "STDERR only (not byte-gated)". So not knowing the count up front is not a byte-identity risk.
- **2026-09-11** — the converted bytes have exactly **one** consumer, the aligner. The methylation caller re-reads the *original* FastQ (`mod.rs:3084`).
- **2026-09-11** — each converted file has exactly **two** concurrent readers in every library mode (`se_instance_plan` `mod.rs:1151`, `pe_instance_plan` `mod.rs:4733`). Two readers rule out a single pipe, not one pipe per reader.
- **2026-09-11** — Bismark writes its own `@PG` (`output.rs:107`) and discards Bowtie 2's, so the temp path never reaches output. Byte-identity here holds **by construction**, not by argument.
- **2026-09-11** — the uBAM/BINSEQ transcode FastQ (`mod.rs:295-310`) is consumed **twice**, by the convert step and the methylation-call re-read, so it genuinely has to be a file. Out of scope.

## Environment

Local toolchain: cargo 1.98.1 / rustc 1.98.1 at `~/.cargo/bin` (**not on the default PATH in
non-interactive shells** — export `PATH="$HOME/.cargo/bin:$PATH"` first). Project floor is
`rust-version = "1.89"`, edition 2024. Clean debug build of `-p bismark --bin bismark`: ~11 s.

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo build  --locked --manifest-path rust/Cargo.toml -p bismark --bin bismark
cargo test   --locked --manifest-path rust/Cargo.toml -p bismark aligner_
cargo clippy --locked --manifest-path rust/Cargo.toml --all-targets
```

Project gate is `just ci` (`rust/justfile`: fmt, clippy, test, test-release) — run it before handover.

The repo pins **rust 1.89** for reproducibility (`Dockerfile`, `just reproduce`), which the local
1.98 toolchain does not match. `spikes/dev.sh` runs cargo in Docker on the pinned 1.89 with the
registry and `target/` in named volumes; use it to confirm a result holds on the pinned
toolchain, not for day-to-day iteration.

## Implementation notes (2026-09-11)

- **A stub aligner that never opens its FIFO deadlocked the whole `aligner_cli` suite.** `aligner_cli.rs` drives a fake `bowtie2`/`minimap2` shell script that prints SAM records without reading a byte, so the writer thread stayed parked in open-for-write and `finish()` waited on it for ever. Fixed at the root: each writer signals once its blocking open has returned, and `StreamedConversion::finish`/`drop` opens and discards any pipe that has not signalled. That is safe at both call sites because every aligner child has already been reaped, so there is nobody left to steal bytes from. Regression test: `stream.rs::finish_does_not_hang_when_nothing_reads_the_pipe`.
- **`temp_dir_supports_fifo` is probed in `run`, not `resolve`.** `resolve` runs in ~100 unit tests whose `--temp_dir` defaults to the CWD; probing there would have littered the repo with FIFOs.
- **The A/B harness is `spikes/ab-run.sh`** (`Dockerfile.ab` = rust 1.89 + Debian bowtie2/samtools). It runs 11 shapes twice each and compares `samtools view` output, so it also covers the unmapped records the in-repo test's `BamReader` filters. Bowtie 2's exact version is irrelevant there — both arms use the same one.

## Traps

- **One writer thread per FIFO, always.** A single writer filling mate 1 before mate 2 deadlocks on the 64 KB pipe buffer (exit 124).
- **Spawn aligner children before writers open for write.** Opening a FIFO for write blocks until a reader opens it. If a child fails to spawn, writers must be unblocked or they hang.
- **HISAT2 cannot be streamed** (exit 255, both `-1/-2` and `-U`).
- **Compare BAMs, or SAM excluding `@PG`.** Bowtie 2's `@PG` records the input path, which necessarily differs between the two paths.
- **Docker cannot bind-mount the session scratchpad** (`/private/tmp/...`) on this machine — the
  mount silently comes up empty. The spike harness therefore `COPY`s fixtures into a throwaway
  image (`Dockerfile.run`) rather than mounting them. The repo itself under `~/GitHub` mounts fine.
- **Use the E. coli fixture, not lambda.** An early spike against an unconverted lambda index gave a 0.06% alignment rate, making byte-identity meaningless. E. coli gives 51.26%.
