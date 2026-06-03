# PLAN — Phase 9b: Order-preserving file-level threading (`--multicore`/`--parallel`) 🎯

> **Epic:** `05312026_bismark-aligner/EPIC.md`, Phase 9 — *FastA input + order-preserving threading*, split into **9a (FastA, DONE)** and **9b (threading, this plan)** per Felix. 9a is squash-merged into `iron-chancellor` (`9650cbf`).

## 1. Goal

Wire `--multicore N` / `--parallel N` (file-level parallelism) into the Rust `bismark`
aligner so that one input read file (SE) or mate-pair (PE) is split into **N contiguous
chunks**, each chunk runs the full convert → 2–4 Bowtie 2 instances → lockstep merge →
per-chunk BAM pipeline on its own worker (single-threaded *per Bowtie 2 instance*), and the
per-chunk outputs are **merged back in chunk order** into the single Bismark BAM + report +
`--unmapped`/`--ambiguous`/`--ambig_bam` outputs.

🎯 **Acceptance gate = worker-count invariance.** For any `N ≥ 1`:

```
bismark_rs --parallel N  ==  bismark_rs --parallel 1  ==  Perl bismark (single-core)
```

byte-for-byte across **every** output (BAM decompressed-content, `_SE_report.txt`/
`_PE_report.txt`, and all aux files). This is the **stronger** guarantee — it deliberately
does **NOT** reproduce Perl's *own* `--multicore N` byte layout (Perl fork+modulo striping
concatenates per-offset temp files → a different merged read order; see §2.4), and it is
**NOT** Bowtie 2 `-p` intra-instance threading (which reorders within an instance → v2).

## 2. Context

### 2.1 Placement / dependencies
- **Depends on Phases 1–9a** (SE + PE, FastQ + FastA, all library types — on `iron-chancellor`).
- 🔴 **Re-base prerequisite (NOT part of implementation; surfaced for Felix).** `rust/aligner-v1`
  carries the now-redundant per-phase commit `3272ecf` (Phase 9a), whose *content* is already
  on `origin/rust/iron-chancellor` via the squash `9650cbf`. Verified: `git diff
  origin/rust/iron-chancellor -- rust/bismark-aligner/` is **empty** (subtree-equivalent).
  Before opening the 9b PR, re-base/branch-fresh so the PR shows only the 9b diff. Force-push
  is **blocked** by Felix's `git push --force *` deny rule, and `git reset --hard
  origin/rust/iron-chancellor` is destructive → **do this on Felix's explicit ask**, not as a
  planning step. (Exactly the dance done for 9a.) cargo + git in `~/Github/Bismark-aligner`
  need `dangerouslyDisableSandbox`; never `git checkout` in the shared `~/Github/Bismark`.
- Worktree `~/Github/Bismark-aligner`, branch `rust/aligner-v1`; crate `rust/bismark-aligner`
  (bin `bismark_rs`).

### 2.2 The seam (already in place — verify-only)
| Component | Status | Evidence |
|---|---|---|
| `--multicore` / alias `--parallel` parsed as `Option<u32>` | ✅ | `cli.rs:149–152` |
| `validate_multicore` (≥ 1, Perl 8244) | ✅ | `config.rs:306–317` |
| `--multicore` currently listed in `deferred_flags` (the STDERR "not yet active" notice) | ✅ | `config.rs:322–338` — 🔴 **drop it once wired** (a stale notice lies) |
| `pipeline()` dispatch on `config.layout` | ✅ | `lib.rs:109–114` — the insertion point for the multicore branch |

### 2.3 What 9b wraps (`rust/bismark-aligner/src/lib.rs`)
- `run_se` (`lib.rs:228–336`) and `run_pe` (`lib.rs:715–845`) are the **per-read-file**
  drivers. Each: loads the genome once (`read_genome_into_memory`, `build_refid`,
  `generate_sam_header`), then per read file → `convert_se_files`/PE conversion →
  spawn instances per `se_instance_plan`/`pe_instance_plan` → `open_sinks`/`open_pe_sinks`
  (writing the **final** BAM/aux paths) → write report header → `drive_merge`/`drive_merge_pe`
  (re-read the original input in lockstep, route each read to its sink) → finish streams →
  `print_final_analysis_report_*` + `write_completion_line` → finish sinks → temp cleanup →
  STDERR summary.
- These helpers are reused **unchanged** by the chunk worker: `convert_se_files`,
  `convert_pe_kind`, `se_instance_plan`/`pe_instance_plan`, `AlignerStream`/
  `PairedAlignerStream::spawn`, `drive_merge`/`drive_merge_pe`, `open_sinks`/`open_pe_sinks`,
  `derive_output_path`, `aux_out::*`, `report::*`, `Counters`.
- **Shared, read-only across workers:** the `Genome`, the `refid` (`HashMap<String,usize>`),
  the `noodles_sam::Header`, and `&RunConfig`. Loaded **once** before the chunk fan-out.

### 2.4 Perl source of truth (`~/Github/Bismark-aligner/bismark`, v0.25.1) — what we do NOT replicate
- `multi_process_handling` fork loop **61–118**: `fork()`s `$multicore-1` children; each child
  gets a fixed `$offset` (1..N). The genome is loaded **once** (line ~276) before the fork →
  children inherit `%chromosomes` via copy-on-write.
- `subset_input_file_FastQ`/`_FastA` **~123–207**: the **modulo stripe** `($line_count -
  $offset) % $multicore == 0` (line ~169) sends read `i` → worker `i mod N` (round-robin per
  read, NOT contiguous), writing per-offset subset files `<input>.temp.<offset>`.
- `merge_individual_BAM_files` **~1390–1482**: the parent `waitpid`s all children, then
  concatenates the per-offset BAMs **in offset order 1→2→3→…** (`foreach my $temp_bam
  (@$tempbam)` piping `samtools view -h` records). 🔴 **Because the assignment was striped,
  this merged order ≠ single-core order** — Perl's `--multicore N` output is NOT byte-identical
  to its own single-core output. We mirror the merge-in-order idea but with **contiguous**
  assignment, so our merged order **does** equal single-core.
- `read_alignment_report` / counter aggregation **~1049–1237**: the parent re-reads each
  child's text report and `+=`-sums every count into `%counting` (order-independent). We sum
  per-chunk `Counters` the same way (commutative).

### 2.5 The precedent — `bismark-extractor` worker-invariance (#884)
Studied for the *principle*, not the mechanism (the shapes differ — see Insight below):
- **Contiguous partition + in-order single-writer merge + commutative counter sum** = the
  three load-bearing invariants for worker-invariant byte-identity (`parallel.rs`).
- **std::thread, NOT rayon** — rayon's `ThreadPool::scope()` consumes a pool thread to run the
  closure, deadlocking at low worker counts; the extractor switched to std threads (rev-1
  finding, `parallel.rs` module docs). 9b uses **`std::thread::scope`** (workers borrow the
  shared immutable `&genome`/`&refid`/`&header`/`&config` — no `Arc` needed for a fixed N-chunk
  fan-out).
- **`feedback_extractor_parallel_cpu_messaging`:** never publish a per-core Rust-p1-vs-Perl-mc1
  speedup; the fair figure is wall-clock at comparable cores.
- **`feedback_gzp_finish_drop_panic`:** the extractor uses `gzp` (parallel gz) and hit a
  finish/Drop double-panic. 9b's aux merge uses the existing **single-thread `flate2`
  `GzEncoder`** (`lib.rs:395`), so this hazard does not apply — but do not switch aux to `gzp`.

> **Why the extractor's per-record reorder buffer does NOT fit here:** the extractor
> parallelizes *per record* (one input, many workers, one collector reordering by `batch_seq`
> and writing all files). The aligner's unit of work is a **Bowtie 2 subprocess** — you cannot
> parallelize within one instance without `-p` (→ reorders → v2). So 9b parallelizes at the
> **chunk** level (each chunk = a full mini-alignment spawning its own 2–4 instances + per-chunk
> outputs), exactly Perl's coarse model, minus the striping. Extractor lends the principle;
> Perl lends the mechanism.

### 2.6 🔴 The central correctness assumption — Bowtie 2 per-read seeding
Worker-invariance is only achievable because **a read's alignment is independent of which other
reads share its input file**. Bowtie 2 re-initialises its pseudo-random generator **per read**
from the read's *name + sequence + quality + `--seed`* (not file position or read ordinal), so
read R aligns identically whether it is record #5 of the whole file or record #1 of a chunk.
The Phase-0 determinism spike already relied on this (run-to-run byte-identical, no `-p`/
`--reorder`/`--seed`). **The oxy gate (`--parallel 4` vs `--parallel 1` vs Perl single-core) is
the authoritative proof; if this assumption were false, 9b would fail there, not silently.**
This is the #1 risk (§11).

🔴 **rev 1 (both reviewers, A/B-I4): the fake-bowtie2 unit gate CANNOT disprove this assumption** —
the fake aligner is deterministic per converted-read by construction, so `§9 #7` validates only the
split/merge/counter **machinery**. The Bowtie-2-per-read-independence assumption is provable **only**
by the real-data oxy gate `§9 #11` — the two are **not** redundant, and #11 is load-bearing, not a
formality (run it early per §11). The gate's loudness further depends on `--parallel 1` being a
**true** single-core Bowtie 2 invocation: each per-chunk `AlignerStream::spawn` must pass the SAME
`config.aligner_options` as single-core (it does — `process_*_chunk` reuses the instance plans +
`config.aligner_options`), so a chunk's Bowtie 2 is invoked identically to single-core's, just on
fewer reads — which is exactly what makes the assumption *testable* by the diff.

## 3. Behavior (numbered)

### 3.1 Dispatch (`lib.rs::pipeline`)
1. Read `n = config.multicore.unwrap_or(1)`.
2. `n == 1` (the **default**, and the proven single-core path) → call the existing
   `run_se`/`run_pe` **unchanged**. No splitting, no per-chunk temp, no merge. This keeps the
   default byte-frozen and is the baseline the gate compares against.
3. `n > 1` → call `run_se_multicore`/`run_pe_multicore` (new, `parallel.rs`).
4. Drop `--multicore` from `deferred_flags` (`config.rs`); the STDERR "not yet active" notice
   no longer mentions it.

### 3.2 Splitting into N contiguous chunks (`parallel.rs`)
Per read file (SE) / mate-pair (PE), and **per read file in the `reads`/`mates` list**
(the file loop stays sequential — each file produces its own BAM, matching single-core):
1. Determine the **effective read set** by applying `skip`/`upto` here, ONCE, at the split, with
   the **exact** arithmetic the single-core Rust converter uses (NOT Perl's subset counter — see
   §3.6): 1-based raw record ordinal `count` (`+=1` per record read), in the effective set iff
   `!(skip>0 && count<=skip) && !(upto>0 && count>upto)` with the Perl-falsy-0 guard, and **stop
   reading at the `upto` break** (don't scan the whole file). Window = reads `(skip, upto]`.
2. Partition the effective reads into **N contiguous ranges** (chunk 0 = the first
   `⌈eff/N⌉`, chunk 1 = the next, …), writing each range to a per-chunk subset file
   `<temp_dir>/<basename>.temp.<chunk>[.<ext>]` (SE) or two lockstep subset files for PE
   (`_1`/`_2`). Preserve the original record bytes verbatim (no uppercasing/conversion — the chunk
   worker re-converts and re-reads exactly like the single-core driver). Use the input's own record
   arity (FastQ 4-line / FastA 2-line).
   - 🔴 **rev 1 (A-Imp7): the subset suffix `<ext>` must track the subset's ACTUAL on-disk
     encoding, not the original's gz-ness.** The converter detects gz from the path suffix
     (`convert.rs:275`), so if the subset bytes are written **plain** the name must NOT carry
     `.gz` (else `MultiGzDecoder` is applied to plain bytes and fails). Simplest: write subsets
     plain, name `<basename>.temp.<chunk>` with no `.gz`.
   - 🔴 **rev 1 (B-I5): the subset is named off the ORIGINAL `basename` with NO `--prefix`
     applied.** The chunk index is the only discriminator; the converter/output helpers prepend
     the prefix as usual (`p.<basename>.temp.<chunk>_C_to_T.fastq`). Do NOT also prepend the
     prefix to the subset name, or you get `p.p.<basename>…`. (`--prefix` + pbat + multicore is a
     live combination — `--prefix` is orthogonal to the pbat dies, `config.rs:287–300`.)
   - 🔴 **rev 1 (B-I6): PE splitting reads BOTH mates in lockstep and partitions the COMMON (min)
     record count** — mirroring `drive_merge_pe`'s break-on-first-incomplete-record (`lib.rs:1037–
     1049`), which truncates a ragged pair at the shorter mate. Compute chunk boundaries over the
     common count so a chunk never gets a trailing R1 with no R2 mate; the per-chunk `R1==R2`
     assertion (§3.8) is then a sanity net that should never fire.
3. Empty / short input: if `eff < N`, the trailing chunks are empty subset files → empty
   converted files → Bowtie 2 over empty input → header-only per-chunk BAM → contributes
   nothing to the merge. (Matches single-core's empty/short output.)

> **Chunk count = exactly N** (one subset per worker) is the recommended sizing — see §10 Q1
> for the 2-pass-count-then-split vs single-pass-fixed-size trade-off. **Byte-identity holds for
> any contiguous partition** (the merge concatenates in chunk order regardless of chunk sizes);
> sizing is a performance/structure choice, not a gate concern.

### 3.3 Per-chunk processing (the worker — reuses the existing pipeline)
Each chunk worker, given its subset file(s) + per-chunk output paths + the shared
`&genome`/`&refid`/`&header`/`&config`, runs the **existing** per-read-file body against the
subset, with these differences from `run_se`/`run_pe`:
1. Convert the subset (`convert_se_files`/PE conversion) — the converter derives names from the
   subset basename (`<basename>.temp.<chunk>_C_to_T.fastq`…), so per-chunk converted files are
   **collision-free** by construction.
2. Spawn the instances (`se_instance_plan`/`pe_instance_plan`) on the subset's converted files.
3. Open per-chunk sinks at **temp** paths: `<temp_dir>/<basename>.temp.<chunk>_bismark_bt2.bam`
   (+ `.ambig.bam`, + plain `--unmapped`/`--ambiguous` — see §3.5).
4. `drive_merge`/`drive_merge_pe` with **`skip`/`upto` cleared** (already applied at the split,
   §3.6) — re-read the subset in lockstep, route each read to the per-chunk sink.
5. Finish streams + finish per-chunk sinks. **Do NOT** write a per-chunk report header/footer —
   the report is merged once (§3.4). Return the per-chunk `Counters` (and the per-chunk output
   paths) to the orchestrator.
6. Genome/header/refid are **borrowed read-only** (`std::thread::scope`); each worker spawns its
   own OS subprocesses (Bowtie 2). With N chunks × 2–4 instances there are up to **4N concurrent
   Bowtie 2 processes**, each loading the bisulfite index — inherent to the file-level model
   (Perl does the same); memory/CPU caveat in §6, no cap beyond N (the user chose N).

### 3.4 Ordered merge (orchestrator, after all workers join, chunks 0..N in order)
1. **BAM:** open the **final** BAM (`derive_output_path`, the original-basename name) with the
   shared header; for each chunk **in order**, open its per-chunk BAM, **skip the header**, copy
   every record to the final writer (noodles record-stream copy — a single writer, matching the
   decompressed-content gate). Same for `--ambig_bam` (`.ambig.bam`). (See §10 Q3 for
   record-copy vs raw-BGZF-block-concat.)
2. **Report:** element-wise **sum** the per-chunk `Counters` (commutative — every field is a
   monotone count), then write the single report: header (`write_report_header`, from the
   original file names) + `print_final_analysis_report_single_end`/`_paired_ends` + the **one**
   `write_completion_line` for the whole wall-clock run.
3. **Aux (`--unmapped`/`--ambiguous`):** open **one** final `flate2::GzEncoder` per aux file; for
   each chunk in order, copy that chunk's **plain** aux bytes through the single encoder →
   the gz stream is byte-identical to `--parallel 1` (both are one flate2 pass over the same
   in-order byte sequence). (See §10 Q4.)
4. **Cleanup:** delete every per-chunk subset / converted / temp-BAM / temp-aux file
   (best-effort, like the existing per-mode cleanup — byte-invisible).

### 3.5 Why aux is written **plain** per chunk then gz-merged
The worker-invariance gate is byte-for-byte, including aux. Per-chunk *gz* files concatenated
would be a multi-member gzip (decompressed-identical but raw-byte-different from `--parallel 1`'s
single-member stream). Writing each chunk's aux **plain** and re-emitting through one final
`GzEncoder` at merge gives a single-member stream **raw-identical** to `--parallel 1`. (The
per-chunk BAM stays a real BAM because the merge reads it via noodles; only aux changes from the
single-core inline-gz to plain-temp + merge-gz.)

🔴 **rev 1 (both reviewers, A-Q4/Imp4, B-I2) — the raw-byte identity rests on three invariants the
implementation MUST hold, one a foot-gun:**
1. **Same compression level** — the merge encoder uses `GzEncoder::new(.., Compression::default())`,
   identical to the single-core aux encoder (`lib.rs:392–399`, `:909–915`). flate2/miniz_oxide
   deflate output is a pure function of (input bytes, level, strategy).
2. **Single `GzEncoder`, NO mid-stream `flush()`** — a `flush()` forces a deflate block boundary
   and would change the bytes. The single-core path only `finish()`es at the end (`Sinks::finish`,
   `lib.rs:358–363`); the merge path must stream all chunks' plain bytes through ONE encoder and
   `finish()` once. (Independent of write-call chunking, which the encoder buffers internally.)
3. **Raw-byte identity is Rust-vs-Rust ONLY.** Against the **Perl** oracle (`§9 #11`), Perl writes
   aux via an external `| gzip -c` (a different encoder), so `--parallel 1` aux will NOT match
   Perl's aux *raw* bytes — only the **decompressed content** matches. The gate must compare aux
   **raw** bytes Rust-vs-Rust (`--parallel N` vs `--parallel 1`) and **decompressed content**
   Rust-vs-Perl. `§9 #6`'s raw-byte assertion is the **canary** for invariants 1–2 (fails loudly
   if a stray `flush()` or a wrong level slips in).

### 3.6 `skip` / `upto` interaction (load-bearing)
Single-core applies `skip`/`upto` inside the converter (`ConvertOptions`, `convert.rs:307–327`)
**and** inside `drive_merge` (`lib.rs:513–525`) — both with identical 1-based-raw-ordinal,
Perl-falsy-0, `count<=skip`/`count>upto` logic. In the chunked path these must be applied
**exactly once, at the split** (§3.2 step 1), and the per-chunk converter + driver must then run
with `skip=None`/`upto=None` (use `None`, not `Some(0)` — both are falsy but `None` is
unambiguous). Otherwise they re-skip within each chunk and drop the wrong reads. **Double-
application is a *live* risk:** both `convert.rs:307` and `lib.rs:513` still carry the skip/upto
code that the per-chunk pipeline must be passed zeroed — pinned by a dedicated unit test (`§9 #2`).

🔴 **rev 1 (both reviewers, A-Imp2 / B-I1) — copy the Rust *converter's* arithmetic, NOT Perl's
subset arithmetic.** Perl's `subset_input_file_FastQ` applies `--upto` via a **per-worker**
`$seqs_processed == $upto` counter (`bismark:164`) and applies **no `--skip`** at the subset stage
— so Perl's own `--multicore N --upto U` keeps up to **N×U** reads (a genuine Perl multicore quirk
that already makes Perl mc-N ≠ its single-core). We deliberately diverge: applying skip+upto once,
globally, at the split yields exactly **single-core** semantics (the gate target) and avoids the
N×U quirk. The splitter's window must equal the converter's `(skip, upto]` over the raw 1-based
ordinal — cite `convert.rs:307–327` as the spec, not the Perl subset.

### 3.7 STDERR messaging (not byte-gated)
Per-chunk `eprintln!` banners ("Created C->T converted version…", ">>> Writing…", the mapping
summary) will interleave across workers and repeat per chunk. This is acceptable (STDERR is not
gated); optionally prefix with the chunk index for readability. Per
`feedback_extractor_parallel_cpu_messaging`: the resolved-config / progress messaging must not
imply a per-core speedup.

**Memory-estimate warning (Q5 resolved → option 2: no cap + warn).** When `n > 1`, emit **one**
STDERR line up front estimating peak Bowtie 2 index memory =
`n × (instances-per-chunk) × (index-file size)` — e.g. `--parallel 8` non-directional ≈ 8 × 4 ×
~3.5 GB. Instances-per-chunk is known from the library/layout (2 for directional/pbat, 4 for
non-directional); the index size is the on-disk `BS_CT`/`BS_GA` `.bt2(l)` total (cheap to `stat`;
the two indexes are typically equal-size, so `stat` one and multiply). We do **NOT** cap
concurrency (matches Perl's `$multicore` — the user chose `N`); the warning just flags the
multiplication so a memory-limited user can lower `N`. The message must not imply a per-core
speedup. **rev 1 (B-O4) wording:** say peak resident is **"bounded by, not equal to"** the estimate
— the OS may page-share read-only index pages across instances of the *same* index. (Worker-
invariance is unaffected — this is STDERR only.)

### 3.8 Edge cases
- **Empty input** → ≥1 empty chunk → header-only final BAM, empty report counts, empty aux.
- **`eff < N`** → trailing empty chunks. 🔴 **rev 1 (B-O1): verify Bowtie 2 2.5.5 exits 0 on an
  empty input file** — `AlignerStream::finish` errors on a non-zero child exit (`align.rs:241–251`),
  so if Bowtie 2 *errored* on empty input a trailing empty chunk would fail the whole run where
  single-core (no chunks) would not. `§9 #9` must run the empty chunk through the **real spawn
  path** (not just the splitter) to confirm exit 0.
- **Multiple input files** → the file loop is sequential; each file is split+merged into its own
  BAM (matches single-core per-file output).
- **PE lockstep split** → both mates split over the **COMMON (min) record count** (§3.2 step 2,
  B-I6), truncating exactly as `drive_merge_pe` does. The per-chunk `R1==R2` count assertion is an
  early **tripwire** only — it catches a *size* desync but not a *content* desync; the real proof
  is the `§9 #7` PE byte-diff (the id-keyed merge surfaces a content desync as wrong pairing).
- **`--gzip`** (converted-temp compression) → orthogonal; the per-chunk converter honours it for
  its converted files. PE FastA still forces converted-gz off (Phase 9a) — unchanged.
- **A chunk worker errors** → 🔴 **rev 1 (both reviewers): orphan-safety is already backstopped.**
  `AlignerStream`/`PairedAlignerStream` `Drop` does `kill()`+`wait()` on the child if not finished
  (`align.rs:255–261`, `:461–466`); a worker that hits `?` drops its owned streams → its Bowtie 2
  children are killed+reaped, and `std::thread::scope` joins all siblings before the orchestrator
  surfaces the error. The scope contract: convert a worker `Err` into a **returned `Result`** (not
  a panic) so the orchestrator can collect all workers' results and return the **lowest-chunk-index
  error** deterministically (B-O3); a worker *panic* re-panics on join (acceptable loud abort, but
  the `§9 #10` test asserts no orphan on BOTH the `Err` and the panic path).
- **Malformed input (error-path, not gated)** → **rev 1 (A-Opt8):** `convert_fastq_impl`'s
  record-1 FastQ sanity check (`convert.rs:344–349`, hard-coded `"sequence 1"`) fires on **each
  chunk's** first read, so a malformed read that is record #5000 of the file but record #1 of
  chunk 2 is reported as `"sequence 1"` by a different chunk than single-core's global order. This
  is **inert on valid input** (byte-invisible happy path) and is an error-path/STDERR-class
  divergence only — acknowledged, not fixed (the gate is happy-path).
- **`--temp_dir` empty** (CWD) → per-chunk temp names are still unique (chunk-index suffix).

## 4. Signatures (proposed)
```rust
// parallel.rs — the multicore orchestrator (new module).

/// SE: split each read file into `n` contiguous chunks, process in parallel,
/// merge per-chunk BAM/aux/counters in chunk order into the final outputs.
pub(crate) fn run_se_multicore(config: &RunConfig, reads: &[String], n: u32) -> Result<()>;
pub(crate) fn run_pe_multicore(
    config: &RunConfig, mates1: &[String], mates2: &[String], n: u32,
) -> Result<()>;

/// Split an input into `n` contiguous subset files under `temp_dir`, applying
/// skip/upto to the effective read set. Returns the per-chunk subset paths.
/// `arity` = 4 (FastQ) | 2 (FastA); handles gz input transparently.
fn split_contiguous(
    input: &Path, temp_dir: &Path, n: u32, arity: usize,
    skip: Option<u64>, upto: Option<u64>,
) -> Result<Vec<PathBuf>>;

/// Process one chunk → per-chunk BAM (+ optional ambig/aux temp paths) + Counters.
/// Reuses convert_se_files / se_instance_plan / drive_merge / open_sinks (to temp
/// paths) — i.e. the run_se body minus the report. Borrowed shared state is
/// read-only; safe under std::thread::scope.
struct ChunkOutcome { bam: PathBuf, ambig_bam: Option<PathBuf>,
                      unmapped: Option<PathBuf>, ambiguous: Option<PathBuf>,
                      counters: Counters }
fn process_se_chunk(config: &RunConfig, genome: &Genome, refid: &HashMap<String,usize>,
                    header: &noodles_sam::Header, subset: &Path, chunk: usize) -> Result<ChunkOutcome>;
// process_pe_chunk analogous (two subset files).

/// In-order merges (single writer each).
fn merge_bams(final_path: &Path, header: &noodles_sam::Header, parts: &[PathBuf]) -> Result<()>;
fn merge_aux_gz(final_path: &Path, plain_parts: &[PathBuf]) -> Result<()>; // one GzEncoder
fn sum_counters(parts: &[Counters]) -> Counters;                          // element-wise +
```

## 5. Implementation outline (TDD)
1. **`config.rs`** — remove `--multicore` from `deferred_flags`. (Keep `validate_multicore`.)
   Verify the resolved-config summary is unaffected (STDERR-only). Add `config.multicore`
   accessor use in `pipeline`.
2. **`lib.rs::pipeline`** — branch: `n = multicore.unwrap_or(1)`; `n==1` → existing `run_se`/
   `run_pe`; `n>1` → `parallel::run_se_multicore`/`run_pe_multicore`.
3. **Refactor (Q2 → (a), resolved):** extract the per-read-file body of `run_se`/`run_pe` into a
   `process_*_chunk` that writes to **given** paths and **returns `Counters`** (no report); `run_se`/
   `run_pe` (N==1) delegate to it writing to the *final* paths, then the **caller** writes the whole
   report (header BEFORE the merge, final-analysis + completion AFTER — preserving the current
   `lib.rs:291`/`:319–321` sequence). 🔴 **rev 1 prerequisite (A-Imp3 / B-I3): BEFORE this refactor,
   confirm at least one existing SE and one PE test asserts the FULL `_SE_report.txt`/`_PE_report.txt`
   body (modulo the wall-clock line) AND the aux files — not just the BAM.** If they only diff the
   BAM, add that assertion first; otherwise the delegation could silently reorder a `write!` in the
   report path with nothing to catch it. Existing test count is **227** (not 226), the byte-frozen
   regression guard (`§9 #8`).
4. **`parallel.rs`** — `split_contiguous` (SE) + PE lockstep splitter; `run_se_multicore`/
   `run_pe_multicore` orchestration via `std::thread::scope` (N workers borrowing shared state);
   `merge_bams` (noodles record copy, skip per-part header); `merge_aux_gz` (one `GzEncoder`);
   `sum_counters`. Per-chunk temp naming `<basename>.temp.<chunk>…`; cleanup at the end.
5. **`open_sinks`/`open_pe_sinks`** — parameterize the aux writers to emit **plain** (not gz)
   when used by a chunk worker (the gz happens at merge), or add per-chunk plain-aux openers in
   `parallel.rs`. The N==1 path keeps the existing inline-gz aux (byte-frozen).
6. **Tests (§9)** — 🔴 **the dominant risk is a test that false-passes** (the Phase-8 `*BS_CT*`-only
   and Phase-9a `NR%4`/`^@` traps). The fake-bowtie2 harness (`tests/cli.rs`) aligns the converted
   file per chunk — but each chunk has a *different* converted file with reads at *different*
   ordinals, so a fake that keys on a line-ordinal / `NR%4` pattern would align DIFFERENTLY per
   chunk and either spuriously fail or (worse) spuriously pass. 🔴 **rev 1 (both reviewers,
   A-Imp1 / B-O2): the fake-bt2 MUST be content-addressed** (decision determined by the read
   **sequence/name**, not ordinal), with an explicit assertion that **the same read yields the same
   fake alignment regardless of which chunk/ordinal it lands in** — that single assertion closes the
   false-pass hole for the unit gate. The worker-invariance test must: pick one count **coprime-ish
   to {2,4,8}** (e.g. 13, or 1000003) so a boundary is straddled at every N with one fixture; force
   a `UniqueBest`, an `Ambiguous`, and a `NoAlignment` read on **both sides** of a chunk boundary;
   include a chunk that ends up **EMPTY** at high N (so the empty-chunk merge path is on the
   byte-identity gate, not just the no-crash test); and assert `--parallel {2,4,8}` == `--parallel
   1` byte-for-byte — BAM (decompressed records), report (filter ONLY the single wall-clock line),
   aux **both** decompressed content AND raw gz bytes. Cover SE + PE × directional + non-dir (+ pbat
   SE/PE FastQ). **FastQ + FastA single-core paths stay byte-frozen** (227-test regression guard).
7. **`scripts/`** — a `phase9b_worker_invariance_gate.sh` (mirror `phase9a_fasta_gate.sh`): on
   oxy, real GRCh38, SE + PE, at 10k + 1M + a non-divisible count. 🔴 **rev 1 (A-Imp6): Rust gets
   `--parallel N`; the Perl oracle gets the identical argv MINUS `--multicore`/`--parallel`.** The
   phase9a harness passes the SAME `"${ARGS[@]}"` to both binaries — if `--parallel` leaks into the
   Perl argv, Perl runs mc-N → striped-reorder → the diff fails for the *wrong* reason (or someone
   "fixes" it by over-filtering order and it silently passes). Compare: BAM (samtools `@PG`-filtered,
   decompressed) + report (wall-clock-filtered) byte-identical across all three; aux **raw bytes**
   Rust `--parallel N` vs Rust `--parallel 1`, **decompressed content** vs Perl (Perl uses external
   `gzip`, §3.5).

## 6. Efficiency
- **Parallel speedup:** N chunks align concurrently; wall-clock ≈ single-core / N (bounded by
  decode + the N-way Bowtie 2 contention). The decode/align is the bottleneck (CLAUDE.md
  profiling), so the file-level model is where the win is.
- **Memory:** the genome is loaded **once** (shared), but each chunk's Bowtie 2 instances load
  the bisulfite index per process → up to 4N index loads concurrently (≈ Perl's fork model).
  This is the inherent cost of file-level parallelism; document it, do not cap below N.
- **Extra I/O:** the split writes N subset files (one extra pass over the input; two with the
  2-pass count, §10 Q1) and the merge reads N per-chunk BAMs — negligible vs alignment.
- mimalloc already global.

## 7. Integration
- **Reads:** the original input (split into subset temps); writes per-chunk converted + BAM +
  plain aux temps; the **final** BAM/report/aux are the existing writers fed by the merge.
- **Order vs other phases:** independent of 9a (format is a per-record concern inside the chunk
  worker; threading wraps the per-file loop). Phase 10's full-scale gate will include a multicore
  cell.
- **Downstream:** the emitted BAM is byte-identical to single-core → consumable by the ported
  extractor/dedup exactly as before.

## 8. Assumptions
**From epic:** Perl v0.25.1 + Bowtie 2 2.5.5 + samtools 1.23.1 oracle; byte-identity on
decompressed SAM content (samtools `@PG` normalised, wall-clock line normalised); adjudicate on
Linux/oxy; identical argv; output is fully Bismark-generated. **Phase-specific:**
- 🔴 Bowtie 2 aligns each read independently of its file-mates (per-read content seeding) →
  contiguous chunking + ordered merge reproduces single-core byte-for-byte (§2.6).
- `--parallel 1` (default) uses the existing direct path, unchanged.
- Any contiguous partition + in-chunk-order merge is byte-identical (chunk *sizing* is a perf
  choice, not a gate concern).
- `skip`/`upto` applied once at the split; per-chunk pipeline runs with them cleared.
- Aux gz reproduced as a single-member stream via one merge-time encoder.
- BAM merge via noodles record copy (single writer), not raw byte concat.
- Genome/refid/header shared read-only via `std::thread::scope` (no `Arc` needed).

## 9. Validation
| # | Verify | How | Expected |
|---|--------|-----|----------|
| 1 | `--multicore` dropped from the deferred-flags notice | unit | notice no longer lists it |
| 2 | `split_contiguous` covers exactly the effective `(skip,upto]` set, arity preserved — incl. **skip AND upto BOTH set with the window straddling a chunk boundary** (rev1 A-Imp2/B-I1) | unit | concatenated subsets == the single-core *converter's* effective input (`convert.rs:307–327` arithmetic); per-chunk pipeline invoked with `skip=None`/`upto=None` |
| 3 | PE split partitions the **COMMON (min)** record count in lockstep (rev1 B-I6) | unit | per-chunk R1==R2 (sanity net, never fires); global truncation point == single-core |
| 4 | `sum_counters` == single-core counters | unit | every field equal to the N==1 run |
| 5 | `merge_bams` concatenates per-chunk records in chunk order under one shared header (per-chunk headers skipped) | unit/integration | record order == single-core; one valid BAM |
| 6 | `merge_aux_gz` aux worker-invariance: **decompressed content** == `--parallel 1` (gz framing is an impl detail — raw bytes differ at scale between the N==1 inline encoder and the N>1 bulk-merge encoder; corrected after the oxy gate) | integration | decompressed content identical across N |
| 7 | 🎯 **worker-invariance (machinery gate):** `--parallel {2,4,8}` == `--parallel 1`, SE + PE × {dir, non-dir, pbat-FastQ}; **count coprime-ish to {2,4,8}** (e.g. 13/1000003); each decision class (`UniqueBest`/`Ambiguous`/`NoAlignment`) on **both sides** of a boundary; ≥1 **EMPTY** chunk at high N (rev1 A-Imp1/B-O2) | integration (**content-addressed** fake bt2) | byte-identical BAM (decompressed) + report (filter ONLY wall-clock) + aux (raw AND decompressed) |
| 7b | **content-addressed fake invariance** (rev1 A-Imp1): the same read → the same fake alignment regardless of chunk/ordinal | unit | identical fake SAM for a read whether record #1 of chunk 2 or record #k of chunk 0 — closes the false-pass hole |
| 8 | **single-core byte-frozen:** the existing **227** tests pass unchanged; ≥1 SE + 1 PE assert the FULL report body (modulo wall-clock) + aux (rev1 A-Imp3/B-I3, prerequisite to the Q2 refactor) | existing suite | zero diff (regression guard) |
| 9 | empty input + `eff < N` (trailing empty chunks) **run through the REAL spawn path** (rev1 B-O1) | integration | Bowtie 2 2.5.5 exits 0 on empty input; header-only BAM; no crash |
| 10 | a chunk worker error/panic propagates, all workers joined, **no orphan Bowtie 2** (rev1 A-Imp5/B-O3) | unit | **lowest-chunk-index** error returned (deterministic); no orphan on the `Err` path AND the panic path |
| 11 | 🎯 **oxy gate (the assumption gate):** real GRCh38, Rust `--parallel 4` vs Rust `--parallel 1` vs **Perl WITHOUT `--multicore`** (rev1 A-Imp6), SE + PE, 10k + 1M + a non-divisible count; only #11 validates §2.6 | `phase9b_worker_invariance_gate.sh` | ✅ **PASSED 2026-06-03** — all 6 cells byte-identical p1==p4==Perl (BAM @PG-block-filtered decompressed + report wall-clock-filtered + aux decompressed) at N=10,000 AND N=1,000,003 (`GATE_OXY.md`) |

## 10. Questions or ambiguities
**(Resolved by the kickoff / SPEC fork #4 — do NOT re-litigate):** the model = a thread-pool over
**contiguous** chunks + order-preserving merge = byte-identical to single-core (NOT Perl's
fork+modulo layout; NOT Bowtie 2 `-p`). The gate = `--parallel N` == `--parallel 1` == Perl
single-core.

**Critical (none).** Scope/goal/behavior are fixed by the kickoff; nothing below changes the
output bytes — each is a structure/performance choice surfaced for the manual review.

- **(Resolved — Q1, chunk sizing, Felix 2026-06-03 → recommendation)** Exactly-N balanced
  contiguous chunks via a **2-pass** (count records, then split) — mirrors Perl's N-worker model,
  fewest Bowtie 2 index loads, simplest merge. (Rejected: single-pass fixed-size + work-queue.)
  The count pass decompresses gz once more; negligible vs alignment.
- **(Resolved — Q2, refactor shape, Felix 2026-06-03 → recommendation)** `run_se`/`run_pe`
  (N==1) **delegate** to the shared `process_*_chunk` (single code path), with the 227 tests as
  the byte-frozen guard (prerequisite: confirm they assert the full report body + aux — §5.3). (Rejected: leave untouched + duplicate the body.)
- **(Resolved — Q3, BAM merge, Felix 2026-06-03 → recommendation)** noodles **record-stream
  copy** (single writer; matches the decompressed-content gate). (Rejected: raw-BGZF-block
  concat.) Confirm at implementation whether `bismark-io` exposes a record reader/copier or we
  use `noodles_bam::io::Reader` directly — a possible additive `bismark-io` helper, no version
  bump per the beta no-bump convention.
- **(Resolved — Q4, aux gz, Felix 2026-06-03 → recommendation)** per-chunk **plain** temp + one
  merge-time `GzEncoder` (raw-identical to `--parallel 1`). (Rejected: per-chunk gz + member
  concat — decompressed-identical only, fails a raw-byte gate.)
- **(Resolved — Q5, concurrency, Felix 2026-06-03 → option 2)** N chunk workers = N concurrent
  (matches Perl's `$multicore`); up to 4N concurrent Bowtie 2 processes. **No cap below N** (the
  user chose N), **plus a one-line memory-estimate warning** at multicore startup (§3.7). A
  RAM-aware auto-cap was rejected (would diverge from Perl's scheduling + add a cross-platform RAM
  dependency); a bare no-warning parity was rejected in favour of the friendlier warning. Output
  bytes are identical under any of these (chunk count and merge order are fixed; only scheduling/
  messaging differ). No new dependency needed — instance count is known from the config, index
  size is a `stat` on the `BS_CT`/`BS_GA` index files.

## 11. Self-Review
- **Logic:** the three invariants (contiguous partition, in-chunk-order single-writer merge,
  commutative counter sum) reproduce single-core byte-for-byte; traced to Perl (fork/modulo/merge
  61–118/169/1390–1482) for what we deliberately diverge from, and to the extractor (#884) for
  the invariants. The chunk worker reuses the proven per-read-file pipeline verbatim. ✓
- **Edge cases:** empty input; `eff < N` (empty chunks); PE lockstep split; multiple input files
  (sequential, per-file BAM); `skip`/`upto` once at the split; `--gzip` orthogonal; worker error
  propagation/join. ✓
- **Integration:** reuses `convert_*`, `*_instance_plan`, `drive_merge*`, `open_*sinks`,
  `derive_output_path`, `report::*`, `Counters`, `aux_out::*`; new = the splitter, the scoped
  fan-out, the three merges, `parallel.rs`. N==1 + FastQ/FastA byte-frozen. ✓
- **Efficiency:** genome loaded once (shared); per-chunk Bowtie 2 index loads (≈ Perl); split/
  merge I/O negligible vs alignment. ✓
- **Risks (rev 1 — both reviewers APPROVE, no Critical; the items below are the residual risk):**
  1. 🔴 **The Bowtie 2 per-read-independence assumption** (§2.6) — if alignment depended on file
     position, worker-invariance fails. *Mitigation:* Phase-0 spike implies independence; **only the
     oxy gate `§9 #11` (real Bowtie 2) can prove it** — `§9 #7` (fake bt2) validates machinery only
     and is deterministic-per-read by construction. Run `§9 #11` **early** (A/B-I4).
  2. 🔴 **A false-passing test** (the extractor/Phase-8/9a recurring trap) — *retired* by the
     **content-addressed** fake-bt2 (decision from read seq/name, not ordinal) + the `§9 #7b`
     same-read→same-alignment assertion + each decision class on both sides of a boundary + an empty
     chunk at high N (A-Imp1/B-O2).
  3. **Aux raw-byte identity** — solved by plain-temp + a single merge `GzEncoder` (Q4), conditional
     on same `Compression::default()` + **no mid-stream flush**; `§9 #6` is the canary. Raw equality
     is Rust-vs-Rust only; vs Perl it is decompressed-content (A-Imp4/B-I2).
  4. **`skip`/`upto` double-application** — solved by applying once at the split with the *converter's*
     `(skip,upto]` arithmetic (NOT Perl's subset counter, §3.6); pinned by the skip+upto-both-set
     boundary-straddle test (`§9 #2`, A-Imp2/B-I1).
  5. **Subprocess orphaning on error** — *already backstopped* by `AlignerStream`/`PairedAlignerStream`
     `Drop` kill+reap (`align.rs:255/461`); scope returns the lowest-chunk-index `Err`; pinned by
     `§9 #10` on both the `Err` and panic paths (A-Imp5/B-O3).
  6. **N==1 delegation refactor** (Q2) silently changing report bytes — *prerequisite*: confirm the
     227-suite asserts the full report body + aux for SE+PE before the refactor (`§9 #8`, A-Imp3/B-I3).
  7. **Trailing empty chunk failing the run** if Bowtie 2 errored on empty input — pinned by running
     the empty chunk through the real spawn path (`§9 #9`, B-O1).
  8. **Gate false-failure** if `--parallel` leaks into the Perl oracle's argv (would striped-reorder
     Perl) — the gate script strips it (`§9 #11`/§5.7, A-Imp6).
  All pinnable before the oxy gate.

## 12. Revision History
- **rev 1 (2026-06-03)** — folded the dual plan-review (`PLAN_REVIEW_A.md` APPROVE-with-conditions,
  `PLAN_REVIEW_B.md` sound/implementation-ready; **both no Critical, no contradictions**; both
  source-verified every load-bearing claim — the three invariants, `Counters` sum-mergeability
  `merge.rs:63–122`, the Perl stripe `:169` + offset-order merge `:1458`, the N==1-doesn't-subset
  guard `:308`/`:478`, the `flate2`-pinned aux encoder, the `Drop` kill+reap `align.rs:255/461`).
  All findings are spec-tightenings / targeted tests / gate-script fixes — **zero design changes.**
  Folded: **(both)** pin the exact skip/upto arithmetic = the converter's `(skip,upto]` NOT Perl's
  subset counter + a skip+upto-both-set boundary test (§3.2/§3.6/§9 #2); aux raw-byte identity rests
  on same-level + single-encoder + **no mid-stream flush**, and is Rust-vs-Rust only (vs Perl =
  decompressed) (§3.5/§9 #6/#11); confirm the 227-suite asserts the full report body + aux before
  the N==1 delegation (§5.3/§9 #8); §9 #7 (fake) = machinery, §9 #11 (real oxy) = the §2.6
  assumption, run #11 early (§2.6/§9/§11); **content-addressed** fake-bt2 + same-read→same-alignment
  + each decision class both-sides + empty chunk at high N to retire the false-pass trap (§5.6/§9 #7/
  #7b); PE split over the COMMON (min) count (§3.2/§3.8/§9 #3); orphan-safety already backstopped by
  `Drop`, scope returns lowest-chunk-index `Err` (§3.8/§9 #10); verify Bowtie 2 exits 0 on empty
  input via the real spawn path (§3.8/§9 #9). **(A-unique)** the oxy gate must run **Perl WITHOUT
  `--multicore`** (strip `--parallel` from the Perl argv) (§5.7/§9 #11); subset suffix tracks actual
  on-disk encoding, plain⇒no `.gz` (§3.2); record-1 FastQ sanity fires per-chunk = error-path/STDERR
  divergence (§3.8); 226→227 tests. **(B-unique)** subset named off ORIGINAL basename, no prefix
  applied, + a `--prefix`+N>1 test (§3.2); memory warning "bounded by, not equal to" + OS page-share
  (§3.7). Ready for the implement trigger.
- **rev 0 (2026-06-03)** — initial plan, after orienting on: the kickoff (`PHASE9B_KICKOFF_PROMPT.md`,
  SPEC fork #4 resolved), the current `lib.rs` `run_se`/`run_pe` drivers + the `--multicore` seam
  (`cli.rs`/`config.rs`), the Perl fork+modulo+merge model (61–118/169/1390–1482 — what we do NOT
  replicate), and the `bismark-extractor` #884 worker-invariance precedent (contiguous partition +
  ordered single-writer merge + commutative counters; std-threads-not-rayon). Scope fixed by the
  kickoff (contiguous chunks + order-preserving merge; worker-invariance gate). Surfaced 5
  structure/perf choices (chunk sizing, refactor shape, BAM merge, aux gz, concurrency cap) — none
  goal/scope/behavior-changing. **All 5 resolved by Felix at manual review (2026-06-03):** Q1
  2-pass exactly-N · Q2 N==1 delegates to a shared `process_chunk` · Q3 noodles record-copy · Q4
  plain-temp + single merge-encoder aux · Q5 no cap + a memory-estimate warning (§3.7). Ready for
  dual plan-review on Felix's word → implement trigger.

## 13. Implementation Notes (2026-06-03)

**Status: IMPLEMENTED + dual-code-reviewed (×2) + plan-manager COMPLETE + 240 local tests green
(clippy `-D warnings` + fmt clean) + 🎯 oxy worker-invariance gate (§9 #11) PASSED. NOT committed
(commit/push/PR + re-base on Felix's ask).**

🎯 **oxy gate (§9 #11) ✅ PASSED 2026-06-03** (`GATE_OXY.md`): `--parallel 4` == `--parallel 1` ==
Perl single-core, byte-identical (decompressed SAM records + reports + aux decompressed content),
all **6 cells** (SE/PE × {dir, non-dir, pbat}) on real GRCh38 at **N=10,000 AND N=1,000,003**
(non-divisible → a chunk boundary straddled at every worker count). pe_dir = 1,703,348 main +
103,110 ambig records. Proves the §2.6 Bowtie 2 per-read-independence assumption. Harness note: the
first run "failed" purely on the `@PG CL` argv line (records were byte-identical) + raw-gz framing —
fixed by filtering the `@PG` block + gating aux on decompressed content (no production change).

### What was built
- **`config.rs`** — `RunConfig` gained `pub multicore: u32` (resolved `cli.multicore.unwrap_or(1)`;
  🔴 **deviation:** the plan assumed `config.multicore` existed — it did not; `--multicore` lived
  only on `Cli`. Added the additive field). Dropped `--multicore` from `deferred_flags`.
- **`lib.rs`** — `pipeline` branches on `config.multicore` (N>1 → `parallel::run_*_multicore`,
  N==1 → direct). New `AuxWriter` enum `{Gz(GzEncoder), Plain(BufWriter)}` (impl `Write` + `finish`);
  `Sinks`/`PeSinks` aux fields + `open_sinks`/`open_pe_sinks` + `write_pe_aux` now use it (N==1 = Gz,
  byte-frozen). Extracted **`process_se_chunk`/`process_pe_chunk`** (convert+spawn+drive_merge+finish-
  streams, sinks/genome/refid passed in, returns the converted temps); `run_se`/`run_pe` (N==1)
  delegate to them, keeping the report/sink-opening code verbatim (Q2a single code path → N==1
  byte-frozen, proven by the 32 unchanged integration tests).
- **`merge.rs`** — `Counters::merge` (field-wise `+`, all 22 monotone `u64` counts).
- **`parallel.rs`** (new) — `split_contiguous`/`split_contiguous_pe` (2-pass count-then-split, the
  converter's `(skip,upto]` arithmetic, plain subsets named off the ORIGINAL basename with NO `.gz`,
  PE over the COMMON-min count); `run_se_multicore`/`run_pe_multicore` (genome/header/refid loaded
  once + shared via `std::thread::scope`, **skip/upto cleared on a `RunConfig` clone** so neither the
  converter nor `drive_merge` re-applies them, memory-estimate warning, lowest-chunk-index error);
  `merge_bams` (RAW `noodles_bam::io::Reader` record copy — see the gate-found bug below);
  `merge_aux_gz` (one `flate2::GzEncoder`); per-chunk plain sinks + temp cleanup. 7 unit tests.
- **`Cargo.toml`** — added `noodles-bam = "=0.89.0"` (bismark-io's transitive pin) for the raw merge.
- **`tests/cli.rs`** — content-addressed fakes (SE id-keyed `m`/`a`/`u`; PE `m`/`u`) + 5
  worker-invariance tests asserting `--parallel {2,4,8}` == `--parallel 1` byte-for-byte (BAM
  decompressed records, report modulo wall-clock, aux RAW gz bytes): SE dir/non-dir/pbat, the
  empty-chunk-at-high-N case, and PE dir.
- **Gate script** — `phase9b_worker_invariance_gate.sh` (this dir; **deviation:** the plan said
  `scripts/`, but the phase-dir convention matches phase8/9a). Runs Perl single-core vs Rust
  `--parallel 1` vs `--parallel PAR`, all 6 SE/PE × {dir,non-dir,pbat-FastQ} cells; Perl gets **no**
  `--multicore` (A-Imp6).

### 🔴 Gate-found bug (the content-addressed fake earned its keep)
The first run of the worker-invariance tests **failed**: `merge_bams` read per-chunk BAMs via
`bismark_io::BamReader::records()`, which yields a validating `BismarkRecord` and **requires
`XR`/`XG`/`XM`**. The `--ambig_bam` holds **raw passthrough** aligner records with no Bismark tags →
`missing required Bismark tag: XR`. Fixed by reading per-chunk BAMs as **raw `RecordBuf`** via
`noodles_bam::io::Reader` (no validation, no unmapped-filter) + `write_raw_record`. An ordinal-keyed
(non-content-addressed) fake might never have routed an ambiguous read through a non-first chunk and
would have false-passed — exactly the trap rev1 A-Imp1/B-O2 warned about.

### Deviations from the plan
1. `RunConfig` had no `multicore` field (added it — §3.1 assumed it existed).
2. `merge_bams` uses a raw `noodles_bam` reader (not `bismark_io::BamReader`) — required because the
   `--ambig_bam` records lack Bismark tags (Q3's "noodles record copier" → noodles directly).
3. Gate script in the phase dir (not `scripts/`), matching the phase8/9a convention.
4. Non-gated STDERR ordering shifts in the N==1 path (open-sinks now precede convert) — byte-invisible
   (outputs unchanged); the 32 integration tests stay green.

### Iteration log
- **#1** config + AuxWriter refactor → built; 32 integration green (N==1 byte-frozen).
- **#2** process_*_chunk extraction; run_se/run_pe delegate → 32 integration + 201 lib green.
- **#3** parallel.rs + Counters::merge + pipeline branch → 201 lib green incl. 7 new unit tests.
- **#4** worker-invariance integration tests → **FAILED**: `merge_bams` rejected the raw `--ambig_bam`
  (missing XR). Switched to raw `noodles_bam::io::Reader` + `noodles-bam` dep → 5/5 green.
- **#5** clippy (`large_enum_variant` on AuxWriter → justified `#[allow]`; `too_many_arguments` on
  `process_pe_chunk` → `#[allow]`, like its `drive_merge_pe` sibling) + `cargo fmt` + doc refresh →
  clippy/fmt clean, 201 lib + 37 integration green.

### Post-review fix-pass (dual `/code-reviewer` both APPROVE — no Critical/High — + `/plan-manager` COMPLETE)
Reviews: `CODE_REVIEW_A.md` (APPROVE w/ minor follow-ups), `CODE_REVIEW_B.md` (APPROVE w/
recommendations), `COVERAGE.md` (verdict COMPLETE). All three re-derived the invariants from source
(contiguous partition, single-writer ordered merges, the 22-field `Counters::merge`, skip/upto via
the config clone, N==1 byte-frozen, the noodles-bam pin). Convergent Medium findings folded:
- **(A-M3/B-M1) `--ambig_bam` cross-N coverage** — `run_se_parallel`/`run_pe_parallel` now also read
  back and assert the `.ambig.bam` records across N (the raw-merge path the gate-found bug lived in).
  🔴 Surfaced a **second** instance of the same validation trap, this time in the TEST helper:
  `canon_bam` used `bismark_io::BamReader` (validates `XR`) → panicked on the tagless ambig BAM.
  Fixed `canon_bam` to read RAW `RecordBuf`s via `noodles_bam::io::Reader` (uniform with the prod merge).
- **(B-M2) PE matrix** — added `worker_invariance_pe_non_directional` (4-instance PE fan-out under
  chunking). (PE ambiguous/`--ambig_bam` merge path is format-agnostic → covered by the SE `a` cells.)
- **(A-M1/B-M3, §9 #10) worker-error test** — `worker_error_propagates_no_hang`: a fake Bowtie 2 that
  exits 1 on alignment → the run exits 1 cleanly (no hang/deadlock, no panic abort).
- **(B-L2) panic payload** — the two `scope` join sites now surface the panic message
  (`panic_message`) instead of a generic string; **deviation documented**: a worker panic is mapped
  to a clean `Err` (CLI exit 1) rather than re-panicking (PLAN §3.8 said "re-panics") — the cleaner
  exit was preferred and orphan-safety still holds via the streams' `Drop` reaping.
- **(A-L4) test count** corrected (227 → **240**: 201 lib + 39 integration).
- **Final verification:** clippy `-D warnings` clean, `cargo fmt --check` clean, **240 tests green**
  (run with `--test-threads=2` — the worker-invariance tests each spawn up to 8 chunks × 2–4
  fake-Bowtie 2 subprocesses, so the default high parallelism can exhaust process/memory limits;
  not a correctness issue).

### Round-2 delta re-review (the fix-pass itself)
On Felix's request, the post-review fix-pass delta (the `parallel.rs` panic-payload change + the
`cli.rs` test additions/`canon_bam` rewrite) got its own focused dual re-review:
`CODE_REVIEW_A2.md` + `CODE_REVIEW_B2.md` — **both APPROVE, no Critical/High/Medium.** Both
re-confirmed: panic→`Err` loses no orphan-safety (worker unwinds + `Drop` reaps + `scope` joins
before the payload is read); the downcast is correct/exhaustive; `canon_bam`'s raw reader is
equivalent for the main BAM (no unmapped there) and required for the tagless ambig BAM;
`worker_error_propagates_no_hang` can't false-pass (detection succeeds, the error is the per-worker
alignment exit 1). Three convergent **Low** notes, all accepted as non-blocking: (L-1) the panic
*branch* itself is inspection-only (optional unit test); (L-2) the PE `.ambig.bam` assertion is
currently vacuous (PE fake has no `a` class — the SE `a` cells + the format-agnostic `merge_bams` +
the oxy gate cover the real ambig merge); (L-3) the two stray untracked test-output files.

### Post-gate aux correction (folded after the oxy gate's first run)
The oxy gate's first run surfaced that the **raw-byte** aux worker-invariance (the original §3.5/§9 #6
claim) does NOT hold at scale: the N==1 inline-incremental `GzEncoder` and the N>1 bulk-merge
`GzEncoder` produce equivalent gz with **different deflate block boundaries** once the data spans
multiple blocks (the local tests passed only because 13 reads fit one block). The decompressed
content IS invariant. Correction (no production change — `merge_aux_gz` already emits a valid
single-member gz with the right content): the gate + the worker-invariance tests now compare aux on
**decompressed content**, consistent with how the BAM is gated on decompressed SAM content (gz/BGZF
framing is an impl detail). §3.5/§9 #6 updated; local tests' aux assertions switched to `read_gz`.

### Open (non-blocking, surfaced for Felix)
- ✅ **§9 #11 oxy gate — PASSED** (`GATE_OXY.md`; see the status block above). The §2.6 assumption is
  proven. Per §11 this was the pre-merge requirement — now satisfied.
- **Stray untracked files** `rust/bismark-aligner/reads_bismark_bt2.bam` + `_SE_report.txt` (present
  since session start, NOT 9b artifacts — leaked test output to CWD). Both reviewers (A-L3/B-L5) flag
  they'd be swept into a `git add .`; `rm` them before committing. Left in place (not mine to delete).
- **Re-base** `rust/aligner-v1` onto `origin/rust/iron-chancellor` (drops the redundant `3272ecf`)
  before the fresh-branch PR — destructive/force-push-blocked, so on Felix's explicit ask.
