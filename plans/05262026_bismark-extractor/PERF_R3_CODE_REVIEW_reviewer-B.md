# Code Review — #884 R3: always-on fixed 2-thread parallel BGZF decode + BAM worker floor (Reviewer B)

**Target:** `git -C /Users/fkrueger/Github/Bismark-extractor diff b2af4e5 6f182f8 -- rust/bismark-extractor/`
(commits `40f3670` always-on threaded decode; `6f182f8` BAM worker floor)
**Files changed:** `rust/bismark-extractor/src/parallel.rs`, `rust/bismark-extractor/tests/parallel_phase_f.rs`
**Reviewer:** B (independent, fresh context — conclusions formed from source, not the diff's comments)

## Verdict: APPROVE

The change is correct, well-scoped, and well-documented. I independently re-verified the
byte-identity linchpin in the noodles 0.47.0 source (the version actually pinned, not the
plan-cited 0.47.0 by guess — confirmed in `Cargo.lock`). The ordering guarantee holds at any
worker count. `cargo test -p bismark-extractor` = 19/19 parallel_phase_f + full suite green;
`cargo test -p bismark-io threaded_bam` green; `cargo clippy --all-targets -D warnings` clean.

No Critical or High issues. A handful of Low/informational notes below — none block merge.

---

## Independent verification of the byte-identity linchpin (concern #1)

`noodles_bgzf::io::MultithreadedReader` (`.../noodles-bgzf-0.47.0/src/io/multithreaded_reader.rs`):

- `spawn_reader` (`:364`) reads BGZF frames **sequentially in file order**. For each frame it
  creates a one-shot channel `(buffered_tx, buffered_rx)`, dispatches `(buffer, buffered_tx)` to the
  shared inflater pool (`inflate_tx`), and **immediately** pushes `buffered_rx` onto `read_tx`
  **in the same frame order** (`:385-386`).
- The consumer `recv_buffer` (`:352`) pops `read_rx` FIFO (frame order) and **blocks** on that
  frame's `buffered_rx.recv()`. So even if inflater workers finish out of order, the consumer
  emits decompressed blocks in strict file order.
- This ordering is **independent of `worker_count`** — 2, 4, or N all emit identically. The R3
  floor-at-2 / fixed-2-decode claims rest on this, and it holds.

**Conclusion:** byte-identity across worker counts is real, not assumed. The "decode parallel,
emit sequential" property is structural in noodles, not incidental.

**Test-strength judgement (the concern raised):**
- The extractor's `legacy_vs_parallel_n1`/`sam_input_matches_*` fixtures are 5 tiny records →
  **a single BGZF data block** (verified: `BamWriter` uses noodles' default single-threaded BGZF
  `Writer`, flushing at ~64 KiB; 5 short records never reach that). With one data block, parallel
  inflate has nothing to reorder, so these tests are **NOT** a genuine multi-block ordering guard
  on their own — they guard dispatch + per-record extraction correctness, not inflate ordering.
- The real ordering guard is `bismark-io::threaded_bam_reader_preserves_record_order`
  (`tests/integration_fixture_bam.rs:261`), which I confirmed runs over a **4-BGZF-block** fixture
  (`test_files/tiny_pe_bismark.bam`, 203 records — I parsed the BGZF BSIZE chain: header block +
  ~2 data blocks + EOF) at **worker_count=4**. Multiple inflaters genuinely process distinct
  blocks here, and the test asserts FIFO qname order. This is a meaningful guard and it passes.
- The plan (rev 1, B-I2) already documents exactly this split. So the coverage is honest. See
  Low-1 for an optional belt-and-suspenders addition.

---

## Issues by area

### Logic / correctness
- **Worker-floor (concern #2):** `n_workers = config.parallel.max(if is_bam {2} else {1})`
  (`parallel.rs:233`). `config.parallel` is `>= 1` always (`cli.rs:422` rejects `--parallel 0`;
  default 1). So BAM always gets `>= 2` workers, SAM/CRAM `>= 1`. Correct. The byte-identity-across-
  worker-count claim that makes this floor *safe* is the `batch_seq` BTreeMap reorder in the
  collector (unchanged by this diff) — verified independent of worker count by the existing
  `legacy_vs_parallel_n1/n4` pair and the new test runs at N=4. Floor changes timing only, not bytes. ✓
- **Dispatch (concern #3):** `AlignmentKind::from_path` (`read.rs:111`) is a **magic-byte sniff**,
  not extension-based — it inflates the first BGZF block and checks `BAM\x01` payload magic. So
  `is_bam` is true **only** for genuine BGZF-wrapped BAM, regardless of extension. This directly
  defuses the "`.bam` extension on a non-BGZF file" worry: such a file would sniff as SAM/CRAM/error,
  NOT route to the threaded reader. CRAM (`C`+`RAM` magic) → `Cram` → `Any`/`open_reader` →
  `CramReader`, unchanged. SAM (`@`) → `Any`. Routing is correct for all three. ✓
- **stdin:** the extractor takes a path argument (`config.files[0]`), not a stream; `from_path`
  opens a real file. No stdin path exists, so no mis-classification risk there. ✓
- **Coord-sort rejection (concern, Validation #4):** `ThreadedBamReader::from_path` (`read.rs:318`)
  calls the **same** `check_not_coordinate_sorted` (`read.rs:326`) as `BamReader::new`
  (`read.rs:237`) used by `open_reader`. `from_path` (not `from_path_without_sort_check`) is used
  at `parallel.rs:247`. Rejection semantics identical → PE adjacent-pairing contract preserved. ✓
- **Legacy reference is genuinely single-threaded:** `extract_se`/`extract_pe` (`pipeline.rs:74,221`)
  use `open_reader` → `AnyReader::Bam(BamReader)` (single-threaded noodles BGZF). So the
  `legacy_vs_parallel_n1` BAM comparison really is single-threaded-decode vs 2-thread-decode through
  the full extraction pipeline — a true (if single-block) cross-check. ✓
- **Production routing:** `main.rs:101-125` routes **every** run (incl. `--parallel 1` default and
  AutoDetect) through `extract_*_parallel` → `run_pipeline`. So the threaded reader + floor are
  exercised in production at the default, matching the plan's "default benefits" goal. ✓

### Errors / edge cases
- **Empty BAM:** `parallel_empty_bam_at_n4_produces_header_only_files` passes; the threaded reader
  + 2 decode threads handle a header-only BAM (sweep of all 12 files works). ✓
- **`ProducerReader::records()` boxing:** returns `Box<dyn Iterator<...> + '_>` to unify the two
  reader types. Allocation is once per run (not per record), negligible. The `BismarkIoError` item
  type matches both arms. Clean. ✓
- **Truncated/corrupt BAM (informational, Low-2):** noodles' `MultithreadedReader` masks a
  *raw-frame* read error as clean EOF — `spawn_reader` (`:380`) returns the `io::Error` into the
  JoinHandle and stops, but `recv_buffer` (`:352`) sees `read_rx` disconnect and returns
  `Ok(None)` (EOF). The error only surfaces on `finish()`, which `ProducerReader`/`records()` never
  calls. *Inflate* errors (corrupt block payload) DO propagate mid-stream. The single-threaded
  `BamReader` surfaces truncated-frame errors directly. So on a truncated tail, the threaded path
  may silently stop at EOF where single-threaded would error. This is a pre-existing noodles
  property shared by `bismark-dedup`'s use of `ThreadedBamReader`, NOT introduced here, and does
  NOT affect byte-identity on well-formed inputs. Flag only; out of scope to fix in this PR.

### Efficiency
- **Always-on 2 decode threads (concern #5):** spawned eagerly in `from_path` (the header read
  triggers `MultithreadedReader::resume()`). Confirmed no test-suite regression — `parallel_phase_f`
  (19 tests, each spawning 2 decode threads on tiny BAMs) finishes in 0.38s; smoke tests in 0.35s.
  The cost is ~+1 core on real workloads (documented), bounded. Acceptable. ✓
- **Redundant sniffs (Low-3):** in AutoDetect mode a BAM is opened+sniffed up to 3× (probe
  `open_reader` in `main.rs:108`, `AlignmentKind::from_path` in `run_pipeline:224`, then
  `ThreadedBamReader::from_path` re-opens as BGZF). Each is ~µs on warm cache and the pattern
  predates this PR (the probe was already there). Not worth changing.

### Structure / style
- `DECODE_THREADS` const (`parallel.rs:113`) with `NonZeroUsize::new(2).unwrap()` compiles as a
  const (rustc accepts the `unwrap` in const context here — clippy clean). Doc-comment is thorough
  and cites the oxy measurement + the `GZIP_COMPRESS_THREADS` precedent. Good.
- `ProducerReader` enum + impl is minimal and idiomatic; both `run_pipeline` and `producer_loop`
  drive it uniformly with no body changes. Clean abstraction. ✓
- `se_directional_records()` refactor (concern #4): extracts the 5 records into a shared helper so
  BAM and SAM fixtures hold byte-identical records. **Behavior-preserving** for the ~15 existing
  callers of `write_se_directional_bam` — the records, tags, positions, and write order are
  identical to the old inline version (same literals, same order); only `finish()` placement is
  unchanged. The full suite (incl. all `se_phase_b` / smoke tests using this fixture) is green. ✓

---

## Recommendations (priority-ranked)

### Critical
None.

### High
None.

### Medium
None.

### Low
1. **(Optional) Add an extractor fixture spanning >= 2 BGZF data blocks for the dispatch/n1 tests.**
   The current 5-record BAM is a single block, so `legacy_vs_parallel_n1` and
   `sam_input_matches_bam_through_r3_dispatch` do not themselves exercise parallel inflate ordering
   (they rely on `bismark-io`'s 4-block fixture for that). The plan already acknowledges this. If
   cheap, writing ~2000 synthetic records (the existing `large` multibatch fixture may already
   cross a block boundary — worth confirming) into a byte-identity test would give the extractor
   its own multi-block ordering guard. Not required: the bismark-io guard covers the mechanism.

2. **(Informational) Document the truncated-BAM EOF-masking difference.** Noodles' threaded reader
   treats a truncated raw frame as EOF (vs single-threaded erroring). Pre-existing, shared with
   dedup, byte-identity-irrelevant on valid input. A one-line note near `DECODE_THREADS` or in the
   bismark-io `ThreadedBamReader` doc would help future debugging of "threaded run silently
   produced fewer records on a corrupt file." No code change needed.

3. **(Informational) The SAM dispatch test compares split files only, not the file set.** It
   asserts every CpG/CHG/CHH `.txt` present in `sam_dir` is byte-equal to BAM and that `compared >
   0`, but does NOT assert SAM produced the *same set* of files as BAM. Since the SAM path is
   unchanged `open_reader` code, this is fine; the test's purpose (guard the new `is_bam ?
   Threaded : Any` else-arm routes SAM away from the threaded reader and yields identical
   methylation calls) is met. Skipping `splitting_report` is correct — its body legitimately
   differs (`Input file: se.bam` vs `se.sam`), and `normalize_report` only strips path lines, not
   the filename embedded in the body, so it could not be reused here.

---

## Validation evidence (run by this reviewer)

- `cargo test --manifest-path rust/Cargo.toml -p bismark-extractor` → all green; `parallel_phase_f`
  19/19 incl. `legacy_vs_parallel_n1_se_default_byte_identical`,
  `sam_input_matches_bam_through_r3_dispatch`, `parallel_empty_bam_at_n4_*`.
- `cargo test --manifest-path rust/Cargo.toml -p bismark-io threaded_bam` → 4/4 incl.
  `threaded_bam_reader_preserves_record_order` (4-block fixture, worker_count=4).
- `cargo clippy --manifest-path rust/Cargo.toml -p bismark-extractor --all-targets -- -D warnings`
  → clean.
- BGZF block count of `tiny_pe_bismark.bam` verified = 4 (header + ~2 data + EOF) via manual
  BSIZE-chain parse.
- noodles-bgzf version verified = `0.47.0` (`Cargo.lock`), and the cited ordering source read
  directly from `~/.cargo/.../noodles-bgzf-0.47.0/src/io/multithreaded_reader.rs`.

**Report path:** `/Users/fkrueger/Github/Bismark-extractor/plans/05262026_bismark-extractor/PERF_R3_CODE_REVIEW_reviewer-B.md`
