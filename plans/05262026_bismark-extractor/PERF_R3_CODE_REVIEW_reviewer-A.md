# Code Review — #884 R3 (fixed 2-thread parallel-BGZF decode + BAM worker floor)

**Reviewer:** A (independent, fresh context)
**Target:** worktree `/Users/fkrueger/Github/Bismark-extractor`, branch `perf-r3-fixed2-decode` @ `6f182f8`
**Diff base:** `b2af4e5..6f182f8` (two commits: `40f3670` always-on threaded decode; `6f182f8` worker floor)
**Files reviewed:** `rust/bismark-extractor/src/parallel.rs`, `rust/bismark-extractor/tests/parallel_phase_f.rs`, plus `bismark-io` `read.rs` (`ThreadedBamReader`, `AlignmentKind::from_path`, `open_reader`, `AnyReader`), `cli.rs` (`--parallel` validation), and the vendored `noodles-bgzf 0.47.0` `MultithreadedReader`.

---

## Verdict: APPROVE — no blocking findings

The R3 change is correct, byte-identity-preserving, and well-scoped. Build, tests (parallel_phase_f 19/19), clippy `-D warnings`, and `cargo fmt --check` all pass locally on my own runs (not just the main session's report). The single Low-priority item below is a pre-existing micro-redundancy, not introduced by this change.

---

## Summary

R3 makes two coupled changes, both BAM-only:
1. **Always-on threaded decode** (`40f3670`): BAM now reads via `ThreadedBamReader` with a fixed `DECODE_THREADS = 2`, decoupled from `--parallel`. SAM/CRAM keep `open_reader` (single-threaded). Dispatched through a new `ProducerReader` enum so `run_pipeline`/`producer_loop` bodies are otherwise unchanged.
2. **BAM worker floor** (`6f182f8`): `n_workers = config.parallel.max(if is_bam { 2 } else { 1 })` so a single extract worker can't bottleneck the 2-thread decode.

Both axes (decode-thread count, extract-worker count) are byte-orthogonal to output, verified below.

---

## Issues by area

### 1. Byte-identity (the central concern) — PASS

**Decode-thread axis (single → threaded-2).** Byte-identity here requires `ThreadedBamReader::records()` to yield records in the *exact* order of the single-threaded `BamReader`. I verified the noodles `MultithreadedReader` (`noodles-bgzf-0.47.0/src/io/multithreaded_reader.rs`, which is the version `bismark-io` resolves to) is **FIFO block-ordered**:
- `spawn_reader` reads frames *sequentially in file order* and, per frame, pushes a dedicated 1-capacity `buffered_rx` onto `read_tx` **in read order** (`multithreaded_reader.rs:375–391`).
- `recv_buffer` consumes `read_rx.recv()` then `buffered_rx.recv()` (`:352–360`), so blocks are surfaced in the order they were read, *regardless of which inflater thread finished first*. Each block has its own private `buffered_tx`/`buffered_rx` pair, so out-of-order inflation cannot reorder the output stream.
- Therefore the decompressed byte stream is identical to single-threaded decode → identical `record_bufs` → identical `BismarkRecord` sequence. Worker-count-invariant by construction.

This is now *directly exercised*: `legacy_vs_parallel_n1_se_default_byte_identical` (`tests/parallel_phase_f.rs:423`) compares legacy `extract_se` (uses `open_reader` → single-threaded `BamReader::from_path`, confirmed at `pipeline.rs:74`) against `extract_se_parallel --parallel 1` — which post-R3 uses `ThreadedBamReader` (2 decode threads) AND `n_workers = max(1,2) = 2`. So this test is no longer "N=1 vs N=1": it is genuinely single-threaded-decode vs threaded-2-decode + 2-worker, byte-for-byte. It passes.

**Extract-worker axis (floor-at-2).** Output is byte-identical across extract-worker counts by the `(batch_seq, within_idx)` collector reorder (`BTreeMap`, module docs `parallel.rs:24–44`). `legacy_vs_parallel_n4_*` already validate 4-worker vs single byte-identity and still pass. The floor changes only how many workers spin up; it cannot move bytes.

Conclusion: floor-at-2 and threaded-2-decode change timing only. No byte-identity risk.

### 2. Floor-at-2 logic — CORRECT

`config.parallel.max(if is_bam { 2 } else { 1 })` (`parallel.rs:233`):
- BAM, `--parallel 1` → `1.max(2) = 2`. ✓
- BAM, `--parallel 4` → `4.max(2) = 4` (floor doesn't cap). ✓
- SAM/CRAM, `--parallel 1` → `1.max(1) = 1`. ✓
- `--parallel 0` is rejected in CLI validation (`cli.rs:421–424`, `InvalidParallelValue`), so `config.parallel >= 1` always holds here; no `max(0,...)` surprise. ✓

No off-by-one. The downstream channel capacities `bounded(n_workers * 4)` (`parallel.rs:273–274`) stay `>= 1` (n_workers ≥ 2 for BAM, ≥ 1 otherwise), preserving the no-deadlock property.

### 3. `is_bam` reuse / no double-sniff — CORRECT

`is_bam` is computed once via `AlignmentKind::from_path(input)?` (`parallel.rs:224`) and reused for both the worker floor (`:233`) and reader selection (`:246`).
- BAM branch: `ThreadedBamReader::from_path` re-opens the file but does **not** re-sniff via `AlignmentKind` — it just `File::open` + `MultithreadedReader` + `read_header` (`read.rs:318–328`). So R3 itself adds no extra `AlignmentKind` sniff.
- SAM/CRAM branch: `open_reader(input, None)` *does* internally re-sniff via `AlignmentKind::from_path` (`read.rs:566`). That is a pre-existing double-sniff (~100–700 µs once, per the BGZF-block-inflate cost noted in `read.rs:93`), **not introduced by R3** — `open_reader` always sniffed. See Low-1.
- SAM and CRAM both correctly take the `Any` path: `AlignmentKind::from_path` returns `Sam`/`Cram` (never `Bam`) for them (`read.rs:124–138`), so `is_bam == false`. ✓

### 4. Coord-sort rejection — PRESERVED

`ThreadedBamReader::from_path` (not `_without_sort_check`) calls `check_not_coordinate_sorted(&header)` (`read.rs:326`), exactly as `BamReader::from_path` does (`read.rs:237`). So the parallel BAM path retains the same `UnsortedInput` rejection it had under `open_reader`. The PE adjacent-pairing read-order contract is unchanged. ✓ (The comment at `parallel.rs:243–245` correctly states this.)

### 5. The SAM dispatch test — SOUND

`sam_input_matches_bam_through_r3_dispatch` (`tests/parallel_phase_f.rs`):
- The `se_directional_records()` refactor is behavior-preserving: the 5 records are byte-for-byte the same literals previously inlined in `write_se_directional_bam` (verified field-by-field against the diff). Both `write_se_directional_bam` and the new `write_se_directional_sam` iterate the same `Vec`, so the BAM and SAM containers hold identical records. All existing tests that call `write_se_directional_bam` still pass (19/19).
- The test runs both inputs at `--parallel 4`: the BAM goes Threaded (2-decode) + 4 workers; the SAM goes `Any` (single-threaded) + 4 workers — so it genuinely contrasts the two reader paths. Identical input records ⇒ identical CpG/CHG/CHH split-file bytes. It asserts `compared > 0` to guard against a vacuous all-empty pass. ✓
- **Skipping `_splitting_report.txt` is correct and necessary.** That file's *content* embeds the input path/filename (the report header includes the input file), and the basename is `se` for both (`se.bam`/`se.sam`) so the *filename* matches — but the body would differ on the `.bam` vs `.sam` path text. The split-data files (CpG/CHG/CHH) carry no filename, share the `se` basename in both dirs, and are the right byte-identity surface. The `bam_dir.join(&name)` lookup is valid because both dirs use the same `se` basename. ✓

One minor note (not a defect): the test proves "same records → same output across reader paths," which is the dispatch guard it claims. It does *not* independently re-prove threaded-vs-single decode ordering — but that is covered by `legacy_vs_parallel_n1` (see area 1). The two tests together fully cover R3.

### 6. CRAM — UNCHANGED, not newly broken

CRAM resolves `is_bam == false` → `open_reader(input, /*cram_ref=*/ None)`. With `None`, the CRAM arm returns `MissingCramReference` (`read.rs:569–572`) — identical to pre-R3 behavior (the old code also called `open_reader(input, None)`). So CRAM remains end-to-end-unsupported exactly as before; R3 does not touch it. ✓

### 7. Efficiency / structure — CLEAN

- `ProducerReader::records` boxes once per call (`parallel.rs:410–415`) — `producer_loop` calls `records()` once and drives the iterator, so it's a single allocation for the whole run, mirroring `AnyReader::records`' existing boxing (`read.rs:527`). No per-record vtable regression beyond what already existed.
- `DECODE_THREADS` const-asserts via `NonZeroUsize::new(2).unwrap()` in const context (`parallel.rs:113`) — fine, evaluates at compile time.
- No dead code: the `ProducerReader::Any` arm is reached by SAM (and would be by CRAM), `Threaded` by BAM. Both `header()` arms used (`:251`, `:260`, and producer).
- `clippy --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean (my runs).
- `SamWriter` import in the test is used (`write_se_directional_sam`), no unused-import warning.

---

## Recommendations

### Critical
None.

### High
None.

### Medium
None.

### Low

**Low-1 — pre-existing double-sniff on the SAM/CRAM path (informational, do not block).**
For SAM/CRAM, R3 computes `is_bam` via `AlignmentKind::from_path` (`parallel.rs:224`), then `open_reader` re-sniffs via `AlignmentKind::from_path` again (`read.rs:566`). This is a redundant magic-byte sniff (one extra `open(2)` + first-byte read; for SAM it's trivial, no BGZF inflate). It is **not introduced by R3** — `open_reader` has always self-sniffed — and the path is rarely hot (SAM/CRAM are not the BGZF perf target). If ever optimized, a `ThreadedBamReader`-style direct `SamReader::from_path` in the else-arm (using the already-known `AlignmentKind`) would remove it, but that's gold-plating and out of scope here. No action recommended for this PR.

**Low-2 — comment-vs-measurement drift (cosmetic).**
The `DECODE_THREADS` doc comment (`parallel.rs:107–112`) cites `--mbias_only 18.3→13.0 s` while the task summary cites `18.8→12.3 s`, and the floor comment (`:226–232`) gives yet another set (`~18.5 / ~16` → `~17.8 / ~13`). These are illustrative trial numbers, not asserted invariants, so this is purely cosmetic. Consider normalizing to one measured figure to avoid future confusion. Non-blocking.

---

## Verification performed
- `cargo test -p bismark-extractor --test parallel_phase_f` → **19 passed, 0 failed**.
- `cargo test ... sam_input_matches_bam_through_r3_dispatch` → **1 passed**.
- `cargo clippy -p bismark-extractor --all-targets -- -D warnings` → clean.
- `cargo fmt --check -p bismark-extractor` → clean (exit 0).
- Read noodles `MultithreadedReader` source to confirm FIFO block ordering (byte-identity root cause).
- Traced `is_bam` / `open_reader` / `ThreadedBamReader::from_path` / `AlignmentKind::from_path` / `--parallel` validation to confirm dispatch, sort-check parity, and floor arithmetic.
