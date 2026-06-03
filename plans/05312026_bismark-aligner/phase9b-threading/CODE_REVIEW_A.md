# CODE REVIEW A — Phase 9b: order-preserving file-level threading (`--multicore`/`--parallel N`)

**Reviewer:** A (independent, fresh context)
**Scope:** uncommitted working-tree diff on `rust/aligner-v1` in `~/Github/Bismark-aligner`
— `config.rs`, `lib.rs`, `merge.rs`, `parallel.rs` (NEW), `Cargo.toml`, `tests/cli.rs`
(+ `EPIC.md`/`PROGRESS.md`/`Cargo.lock` doc/lock).
**Gate:** worker-count invariance — `bismark_rs --parallel N` == `--parallel 1` == Perl
single-core, byte-for-byte (decompressed BAM content, report, aux); N==1 byte-frozen.

## Verdict

**APPROVE with minor follow-ups (no Critical, no High).** The three worker-invariance
invariants are correctly implemented and I re-derived each from source. The build is
clean and the suite is green:

- `cargo test -p bismark-aligner` → **201 lib + 37 integration** green (incl. 5 new 9b
  worker-invariance tests + 6 new `parallel` unit tests).
- `cargo clippy -p bismark-aligner --all-targets -- -D warnings` → clean.
- `cargo fmt -p bismark-aligner -- --check` → clean.

Findings below are all **Medium / Low** — test-coverage gaps relative to the plan's §9
and one repo-hygiene item. None blocks the implementation; they should be closed before
(or alongside) the oxy gate.

---

## Invariant verification (re-derived from source)

### (a) Contiguous partition covering exactly `(skip, upto]` — HOLDS
- `quotas` (`parallel.rs:118`) is balanced-contiguous: `base = eff/n`, first `eff%n`
  chunks get +1; trailing chunks get 0 when `eff < n`. Σquota == eff. Unit-tested
  (`quotas_balanced_contiguous`, incl. `eff<n` and `eff==0`).
- `in_window`/`past_upto` (`parallel.rs:98`/`110`) implement `count<=skip`/`count>upto`
  with the Perl-falsy-0 guard. I diffed this against the converter (`convert.rs:317–327`)
  and against `drive_merge` (`lib.rs:612–622`) and `drive_merge_pe` (`lib.rs:1169–1180`):
  **all four agree** — 1-based ordinal, `Some(0)`/`None` both disable, stop at `upto`.
- The splitter (`split_contiguous:187`) advances chunks only on effective reads and writes
  the verbatim record bytes; concatenating chunks in order reproduces the converter's
  effective input. Verified by `split_concatenation_equals_input_no_skip` and the
  skip+upto boundary-straddle test (`split_skip_and_upto_both_set_straddles_boundary`,
  reads 3..=8 over n=4).
- SE `read_record` (arity=4) requires all 4 lines non-zero — **bit-identical** to
  `drive_merge`'s SE incomplete check (`lib.rs:607`) and the converter (`convert.rs:304`).

### (b) In-order single-writer merges — HOLD
- `merge_bams` (`parallel.rs:521`) opens ONE `BamWriter` with the shared header, then for
  each part **in `parts` order** reads via raw `noodles_bam::io::Reader::record_bufs`,
  skips the per-chunk header, and `write_raw_record`s every record through the single
  writer; one `finish()`. Header handling is correct: each `@SQ` is identical (all built
  from the same shared `Genome`/`generate_sam_header`), so the per-chunk ref-id indices
  are valid against the final header. `parts` are assembled in chunk order
  (`outcomes.iter().map(..bam..)`, `:673`/`:815`), so record order == single-core.
- `merge_aux_gz` (`parallel.rs:545`) streams each plain part through ONE
  `GzEncoder::new(.., Compression::default())` with **no mid-stream flush** and one
  `finish()` — matches the single-core encoder (`open_sinks` `lib.rs:489–497`,
  `Compression::default()`). The `AuxWriter::finish` (`lib.rs`) deliberately avoids a
  mid-stream `flush()` on the plain variant (only flushes the BufWriter once at the end),
  preserving the deflate-block layout = raw-byte identity Rust-vs-Rust. The N==1 path
  keeps inline `AuxWriter::Gz`, byte-frozen.

### (c) `Counters::merge` is a complete 22-field sum — HOLDS
I mechanically diffed the `Counters` struct fields against the `merge` body:
**22 struct fields, 22 summed, zero dropped** (verified with a field-by-field `comm`).
Every field is a monotone `u64` count → commutative/associative → report identical
regardless of worker count. Unit-tested (`counters_merge_is_field_wise_sum`).

### skip/upto double-application — CORRECTLY AVOIDED
`run_se_multicore`/`run_pe_multicore` (`parallel.rs:625–628`/`765–768`) snapshot
`orig_skip`/`orig_upto`, then clone `RunConfig` and set `read_processing.skip/upto = None`.
`split_contiguous` is called with the **original** values (applied once at the split);
the cloned `cfg` (skip/upto cleared) is what the chunk jobs see, so **both** the converter
(`ConvertOptions::from_config(&cfg)`, `:629`) AND `drive_merge` (reads
`config.read_processing`, `lib.rs:584`) get them cleared. This is the load-bearing detail
and it is correct.

### N==1 byte-frozen — HOLDS
`process_se_chunk`/`process_pe_chunk` are a faithful extraction of the convert→spawn→
drive_merge→finish-streams body; `run_se`/`run_pe` keep the sink-opening, report-header,
final-analysis, completion-line, and temp-cleanup code verbatim. The only behavioural
shift is STDERR ordering (open-sinks now precede convert) — non-gated and documented.
The 32 pre-existing integration tests (full BAM record assertions + report substring
assertions) stay green, and the new worker-invariance tests now pin the **full** SE/PE
report body (modulo wall-clock) with N==1 as the baseline.

### Raw-BAM merge / noodles pin — CORRECT
`merge_bams` reads raw `RecordBuf` (no Bismark-tag validation) — required because
`--ambig_bam` holds tag-less aligner records (`write_raw_record` bypasses validation,
`bismark-io/src/write.rs:86`). `noodles-bam = "=0.89.0"` matches bismark-io's transitive
pin; `Cargo.lock` shows a single shared `noodles-bam 0.89.0`. Round-trips decompressed
content faithfully (`record_bufs` → `write_raw_record` → `finish` writes the BGZF EOF).

### PE lockstep split / common-min — CORRECT
`count_effective_pe`/`split_contiguous_pe` (`parallel.rs:213`/`245`) read both mates in
lockstep, break on the first mate that lacks a complete record, and partition the COMMON
(min) pair count — mirroring `drive_merge_pe`. Per-chunk R1==R2 unit-tested
(`pe_split_partitions_common_min_count_in_lockstep`, R1=7/R2=5 → 5 pairs).

### `std::thread::scope` / orphan safety — CORRECT
Genome/refid/header/cfg/opts are borrowed read-only (`&` refs captured into `s.spawn`
closures). A worker `Err` propagates as a returned `Result`; `collect_in_order`
(`parallel.rs:600`) iterates in chunk order and returns the **lowest-chunk-index** error
via `?`. The scope joins every worker before the orchestrator surfaces the error, and each
worker's `AlignerStream`/`PairedAlignerStream` `Drop` kills+reaps its Bowtie 2 child
(`align.rs:255/461`) → no orphan on the `Err` path. (See M2: the panic path is *handled
differently from the plan* and is untested.)

### Temp naming / cleanup / `--prefix` — CORRECT
Subsets are named off the ORIGINAL basename `{base_name}.temp.{i}`, plain (no `.gz`), **no
prefix** (`split_contiguous:175`). The converter prepends `--prefix` once
(`convert.rs:264`) → `p.reads.fq.temp.0_C_to_T.fastq`, no `p.p.` doubling. The **final**
BAM/report/aux paths use the original `read_file` via `derive_output_path`/`aux_filename`
(prefix applied once). Per-chunk converted+subset cleaned in `se_chunk_job`/`pe_chunk_job`;
per-chunk BAM/aux cleaned in `cleanup_se`/`cleanup_pe`. No leak or collision found (distinct
chunk-index suffix guarantees uniqueness; converted names also distinct per chunk).

---

## Findings

### M1 (Medium) — §9 #10 not tested: worker error/panic propagation + no-orphan
The plan's §9 #10 calls for a test that a chunk worker error/panic propagates as the
**lowest-chunk-index** error with **no orphaned Bowtie 2** on BOTH the `Err` and panic
paths. There is **no such test** — `collect_in_order` has no unit test, and no integration
test forces a chunk to fail (e.g. a fake bowtie2 that exits non-zero for one chunk) and
asserts the deterministic error + reaped children. The logic is correct on inspection, but
this is the one validation item with zero coverage. *Recommend:* add a unit test for
`collect_in_order` (lowest-index `Err` wins) and an integration test with a fake bowtie2
that `exit 2`s on a specific read-id so exactly one chunk fails.

### M2 (Medium) — panic handling deviates from the plan and is undocumented as a deviation
Plan §3.8 says "a worker *panic* re-panics on join (acceptable loud abort)". The
implementation instead **catches** the panic (`h.join().unwrap_or_else(|_| Err(..))`,
`parallel.rs:658`/`800`) and converts it to a `Validation("a Phase-9b chunk worker
panicked")` error. This is arguably *better* (deterministic, still joins all siblings, no
orphan), but it contradicts the plan and is not listed in §13's "Deviations". Functionally
fine — flagging for the plan-manager to reconcile (either update the plan or note the
deviation). No code change strictly required.

### M3 (Medium) — `--ambig_bam` merged-record invariance not directly asserted (esp. PE)
The SE worker-invariance tests run with `--ambig_bam`, so the `merge_bams` raw-record path
is *exercised* (and the content-addressed fake's `a` class earns its keep — it's what
caught the original missing-XR bug). But the test compares only the **main** `_bismark_bt2.bam`,
the report, and the `_unmapped`/`_ambiguous` aux — it never reads back the `.ambig.bam` to
assert its records are byte-identical across N. The **PE** path is weaker still: the PE
test uses `--unmapped` only (no `--ambiguous`/`--ambig_bam`), so PE `merge_bams`-for-ambig
is entirely untested. Risk is low (PE ambig uses the identical `merge_bams` already proven
on the main BAM), but it's a real gap vs the plan's intent. *Recommend:* assert `canon_bam`
on the `.ambig.bam` in the SE cells, and add `--ambig_bam` (and ideally an `a`/ambiguous
class) to the PE cell.

### L1 (Low) — no explicit §9 #7b unit test (same-read → same fake alignment)
Plan §9 #7b wants a standalone assertion that a read yields the same fake SAM regardless of
chunk/ordinal. The property is enforced *by construction* (the fake keys on the read-id
first char, `tests/cli.rs` `make_fake_bowtie2_content_addressed`) and validated
*end-to-end* by the N-invariance diff, so the false-pass hole is effectively closed — but
there is no dedicated #7b unit test as the plan specifies. Cosmetic; the construction +
end-to-end gate cover it.

### L2 (Low) — PE splitter `read_record` vs `drive_merge_pe` differ on malformed `+` lines
The PE splitter's `read_record` (arity=4) requires all 4 lines non-empty, whereas
`drive_merge_pe`'s incomplete check guards only 6 of 8 lines (the two `+` lines are NOT
guarded — replicating a Perl quirk, `lib.rs:1151–1164`). On **valid** FastQ these agree
exactly; they diverge only on malformed input where a `+` line is empty but qual is present.
Per plan §3.8, malformed input is explicitly error-path/non-gated, so this is inert on the
happy path. Noting only for completeness. (The SE driver, by contrast, guards all 4 lines
and matches the SE splitter bit-for-bit.)

### L3 (Low) — repo-hygiene: stray output artifacts in the crate dir, not gitignored
Two untracked files sit in `rust/bismark-aligner/`: `reads_bismark_bt2.bam` and
`reads_bismark_bt2_SE_report.txt` (a manual `--pbat` run with `--output_dir` omitted →
output landed in CWD). They are **not** gitignored and would be swept up by `git add .`
into the 9b commit. *Recommend:* delete them before committing (and/or add a gitignore
entry). Not produced by the test suite (all tests use `TempDir` `--output_dir`).

### L4 (Low) — doc drift on test count
Plan §5.3/§9 #8 repeatedly cite "227 tests" as the byte-frozen guard, and §13 mentions
"201 lib + 37 integration". The actual current count is **201 lib + 37 integration = 238**
(of which 5 integration + 6 unit are new in 9b). Harmless doc inconsistency; the regression
guard (prior 32 integration tests unchanged) is intact regardless.

---

## Efficiency / structure notes (no action required)
- Genome/refid/header loaded **once** before the fan-out and shared by `&` — no `Arc`, no
  per-worker reload. Correct and matches the plan.
- 2-pass split (count then partition) re-decodes a gz input once more — negligible vs
  alignment, as the plan notes.
- `AuxWriter` `large_enum_variant` and `process_pe_chunk`/`open_chunk_pe_sinks`
  `too_many_arguments` are justified `#[allow]`s consistent with sibling code.
- `emit_memory_warning` (`parallel.rs:578`) is STDERR-only, "bounded by, not equal to"
  wording present, degrades gracefully to no-detail if the index dir can't be stat'd.
  The `{stem}.`-prefix sum may over-count unrelated sibling files, but it's an estimate by
  design — acceptable.

## Bottom line
The implementation is sound and the gate-relevant invariants are correct and tested at the
machinery level (fake-bt2 N-invariance for SE dir/non-dir/pbat + empty-chunk + PE dir). The
remaining work before the oxy gate is **test coverage**, not logic: close §9 #10 (M1),
strengthen `--ambig_bam`/PE-ambiguous assertions (M3), and reconcile the panic-handling
deviation (M2). The real-data oxy gate (§9 #11) remains the authoritative proof of the
Bowtie-2-per-read-independence assumption, as the plan states.
