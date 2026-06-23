# GATE 00 — baseline profile (Apple M4 Max)

`sample` of the `bismark_rs` process (profiling build, mimalloc), 90 s in the
steady state of a `-p 6` RRBS-10M / GRCm39 run. `sample` profiles `bismark_rs`
only; the bowtie2 children are separate PIDs. "Top of stack" = where threads
actually were.

## The dominant fact: under `-p`, the aligner waits on bowtie2

| Top-of-stack frame | samples | meaning |
|---|---|---|
| `read` (libsystem_kernel) | **73 702** | **blocked waiting on bowtie2 output (+ input gz)** |
| _all Rust-CPU frames combined_ | **~7 000** | the optimizable surface |

→ ~90 % of `bismark_rs` wall is **blocked on the external bowtie2 process**;
only ~10 % is Rust CPU. This is why mimalloc (and any Rust-side optimization) is
**neutral under `-p`** — the aligner is bowtie2-bound in that regime. The Rust
side only becomes the bottleneck under `--multicore` (N concurrent pipelines).

## Breakdown of the ~10 % Rust CPU (top-of-stack, ≥ samples)

| Area | ~samples | notes |
|---|---|---|
| **zlib_rs** (deflate+inflate+crc32) | ~2670 | gzip input read + BGZF output write — **already optimal** (maintainer's domain) |
| alloc + memmove/memcpy/bzero | ~950 | `mi_*` + `_platform_memmove` — Phase 2/4 target |
| BAM encode / output (`build_pe_mate`, noodles encoders) | ~570 | record serialization |
| **UTF-8 validation of SAM lines** (`core::str`, CharSearcher, Utf8Chunks) | ~424 | `SamRecord::parse` takes `&str` → validates every bowtie2 line; parsing `&[u8]` would skip it |
| methylation (`methylation_call`, `reverse_complement`, `parse_cigar`, `walk_mate`, `make_mismatch_string`) | ~410 | Phase 4 buffer-reuse target |
| `SamRecord::parse` + `clone` + `from_lines` | ~150 | Phase 2/4 (raw_line gate, borrow) |
| **bisulfite transform** (`fix_id` + `convert_seq_*`) | **~120** | **COLD — Phase 3 (vectorization) not worth it** |

## Decisions driven by this profile

- **Phase 3 (vectorize the C→T/G→A transform): SKIP.** The transform is ~120
  samples (~0.2 % of wall). Vectorizing it would save a fraction of a percent.
  This is the profile-gated non-action the plan reserved.
- **`-p` regime has ~no Rust headroom** (bowtie2-bound). Do not claim a `-p`
  speedup from Rust-side work; the honest scope is the `--multicore` path.
- **Phase 2 (alloc reduction) + an `&[u8]` SAM parse (drop UTF-8 validation):**
  ~1500 combined samples, but the payoff is on the **`--multicore`** path (fewer
  allocations → less contention, compounding with mimalloc). Worth ONE measured
  attempt under `--multicore`; include only if it adds meaningfully.
- **Phase 4 (full `SamRecord` borrow refactor): defer** unless the Phase 2
  `--multicore` measurement shows the parse path is worth the lifetime churn.

## Net

The headline win is **mimalloc (3.13× on `--multicore`)**. The profile says the
remaining Rust headroom is small and lives entirely on the `--multicore` path;
the `-p` realistic regime is bowtie2-bound. Phase 3 is dead on arrival per the
data. Phase 2 is the only further Rust lever worth a measured try.
