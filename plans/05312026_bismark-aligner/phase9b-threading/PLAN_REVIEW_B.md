# PLAN_REVIEW_B — Phase 9b: order-preserving file-level threading

**Reviewer:** B (independent, fresh context)
**Plan reviewed:** `phase9b-threading/PLAN.md` (rev 0, 2026-06-03)
**Verdict:** Sound and well-grounded. The core design (contiguous chunks + in-order single-writer merges + commutative counter sum, std::thread::scope, N==1 byte-frozen) is correct and re-derives cleanly against the actual `lib.rs`/`config.rs`/`convert.rs`/`align.rs`/`merge.rs` source and against the Perl `bismark` v0.25.1 oracle. No **Critical** defects found. Several **Important** items are worth pinning into the plan before implementation so they cannot be lost; the rest are **Optional** sharpenings.

Everything below was re-derived from source; file:line citations are to the worktree as read.

---

## Verification of the load-bearing claims (all confirmed)

- **Perl `--multicore N` ≠ Perl single-core output (the plan's premise for the *stronger* gate).** Confirmed. `subset_input_file_FastQ`/`_FastA` stripe by `($line_count - $offset) % $multicore == 0` (`bismark:169` / `:234`), so worker `offset` gets reads `offset, offset+N, offset+2N, …` (round-robin), and `merge_individual_BAM_files` concatenates the per-offset BAMs in `@tempbam` order (`:1458–1477`), giving interleaved-then-blockwise order — NOT the contiguous single-core order. The plan is right to *not* replicate this and to instead use contiguous chunks for a single-core-identical merge.
- **Single-core Perl does NOT subset.** Confirmed: `unless ($multicore == 1)` guards `subset_input_file_*` for both PE (`:308`) and SE (`:478`); at N==1 the converter (`biTransform*`) reads the ORIGINAL file. This validates the plan's §3.1 design that N==1 takes the existing direct path and N>1 splits. Good.
- **Report merge is commutative count summation with the FIRST temp report's header.** Confirmed: `merge_individual_mapping_reports` (`:1006–1044`) prints the header lines from the first temp report then `read_alignment_report` `+=`-sums every count. The Rust plan instead writes a fresh header from the ORIGINAL file names + sums per-chunk `Counters` — byte-equivalent to single-core because the single-core Rust header is *also* written from the original names (`lib.rs:291` / `:802`). ✓
- **`Counters` is sum-mergeable.** Confirmed: every field of `Counters` (`merge.rs:63–122`) is a monotone `u64` count; `#[derive(Default, Clone, PartialEq, Eq)]`. A field-wise `+` is exactly correct and order-independent. ✓
- **Aligner children are reaped on worker error.** Confirmed: both `AlignerStream` (`align.rs:255–261`) and `PairedAlignerStream` (`align.rs:461–466`) have `Drop` impls that `kill()` + `wait()` the child. A worker that errors drops its local `streams`, so no orphaned Bowtie 2 processes — provided the worker owns its streams in a scope that unwinds on early `?` (it does, by reusing the `run_se`/`run_pe` body). ✓
- **Temp-naming collision-freedom.** Confirmed plausible: `convert_fastq_impl`/`convert_fasta_impl` derive the converted name from `input.file_name()` (`convert.rs:261`), so a subset named `<basename>.temp.<chunk>` yields converted files `<basename>.temp.<chunk>_C_to_T.fastq` — unique per chunk, and disjoint from both the single-core converted name (`<basename>_C_to_T.fastq`) and the final outputs (which `derive_output_path` builds from the ORIGINAL `read_file`, `lib.rs:431–449`). ✓ (One caveat under `--prefix` — see Important I-5.)
- **gzp finish/Drop hazard does not apply.** Confirmed: SE/PE aux sinks use `flate2::write::GzEncoder` (`lib.rs:343`, opened at `:395`/`:911`), not `gzp`. The plan's §2.5 note is correct; keep aux on flate2 at merge time.
- **Extractor precedent (std::thread, not rayon).** Confirmed against `bismark-extractor/src/parallel.rs` module docs (lines 50–68): rayon `ThreadPool::scope()` deadlocks at low N because the scope closure consumes a pool thread; the extractor uses `std::thread::spawn`. The plan's choice of `std::thread::scope` is the right analogue and sidesteps that trap.

---

## Logic review

The three invariants (contiguous partition, in-order single-writer merge, commutative counter sum) are individually correct and jointly sufficient for byte-identity *given* the per-read-independence assumption (§2.6). The chunk worker reuses the proven per-read-file body verbatim, so the per-record correctness is inherited from Phases 1–9a. The dispatch (§3.1), edge cases (§3.8), and signatures (§4) are internally consistent.

The areas that warrant tightening, in risk order, follow.

---

## Critical

**None.** Scope/behavior is fixed by the kickoff; the design reproduces single-core bytes by construction and the oxy gate is the authoritative proof. No defect found that would silently corrupt output or false-pass the gate as specified.

---

## Important

### I-1 — §3.6 / §4 `split_contiguous`: pin the *exact* skip/upto record-counting semantics, because the single-core converter and `drive_merge` count differently from the Perl subset
This is the highest-leverage correctness detail and the plan states the *principle* ("apply once at the split, clear in the per-chunk pipeline") but not the *arithmetic*, which is where an off-by-one would hide.

The single-core Rust path applies skip/upto on the **raw record ordinal** `count` (1-based, incremented BEFORE the skip/upto test) in two places that MUST agree: the converter (`convert.rs:307–327`: `count += 1; if skip>0 && count<=skip {continue}; if upto>0 && count>upto {break}`) and `drive_merge` (`lib.rs:513–525`, identical logic). So the effective set is reads with ordinal in `(skip, upto]` over the raw 1-based record index — **including** the falsy-0 disabling (`Some(0)`/`None` both mean "off").

`split_contiguous` MUST define the effective set with byte-identical arithmetic:
- iterate raw records, `count` starting at 0 and `+= 1` per record read;
- a record is in the effective set iff `!(skip.is_some_and(|s| s>0 && count<=s))` **and** `!(upto.is_some_and(|u| u>0 && count>u))`, evaluated with the SAME 1-based `count` and the SAME falsy-0 guard;
- stop reading at the upto break (don't scan the whole file).

Then the per-chunk converter + `drive_merge` run with `skip=None`/`upto=None` (NOT `Some(0)` — both are falsy but pass `None` to be unambiguous). Note the Perl subset's own `--upto` test (`$seqs_processed == $upto`, `:164`/`:229`) is a DIFFERENT, post-stripe counter and is NOT what the Rust single-core path matches — do not copy the Perl subset arithmetic; copy the Rust *converter* arithmetic. **Add a unit test that feeds skip and upto BOTH set, with the effective window straddling a chunk boundary, and asserts the concatenated subsets == the single-core converter's effective input.** §9 #2 covers skip/upto coverage but does not explicitly require the *both-set + boundary-straddle* case.

Action: in §3.6/§4, state the counting rule verbatim ("1-based raw ordinal, falsy-0, `(skip, upto]`, stop at upto") and cite `convert.rs:307–327` as the spec; strengthen §9 #2 to require skip+upto-together.

### I-2 — §3.4.3 / Q4: assert the aux gz raw-byte determinism rests on (a) identical `Compression::default()` AND (b) no mid-stream flush in *either* path
The plan's Q4 reasoning ("both are one flate2 pass over the same in-order bytes") is correct, but it silently depends on two facts the implementation must hold, and one of them is a real foot-gun:

1. The single-core aux encoder is `GzEncoder::new(BufWriter::new(File), Compression::default())` (`lib.rs:392–399`, `:909–915`). The merge encoder MUST use the **same level** (`Compression::default()`) and the same `GzEncoder` type. flate2/miniz_oxide deflate output is a pure function of (input bytes, level, strategy) and is **independent of write-call chunking** — *only if* there is no intervening `flush()`. A `flush()` forces a deflate block boundary and would change the bytes.
2. Neither path may call `flush()` mid-stream on the aux encoder. The single-core path only `finish()`es at the end (`Sinks::finish`, `lib.rs:358–363`). The merge path must likewise stream all chunks' plain bytes through ONE encoder and `finish()` once — no per-chunk flush. (And re-confirm the gzp finish/Drop fix is irrelevant here because this is flate2; it is.)

Action: §3.5/Q4 should state "same `Compression::default()`, single `GzEncoder`, no mid-stream flush" as an explicit invariant. **§9 #6 already asserts raw-byte identity — keep it, and make it the canary: it will fail loudly if either condition is violated.** This is the one place where a subtle implementation slip (e.g. a stray `flush()` or a different default level) would produce decompressed-identical-but-raw-different output.

### I-3 — Q2 N==1 delegation: the 226 tests are a *necessary* but possibly *insufficient* byte-freeze guard; pin what they actually assert
The plan recommends `run_se`/`run_pe` (N==1) delegate to the shared `process_*_chunk`, with "the 226 existing tests as the byte-frozen regression guard." That is the right shape, but the guard's strength depends on whether the existing tests assert the **full report content** (header + counts + the structure of the footer), not just the BAM. Two byte-surfaces the delegation could perturb that a BAM-only assertion would miss:
- **Report header/footer ordering and the count lines.** The wall-clock line is filtered in the oxy gate, but the *count* lines and header are not. If the refactor moves where the report header is written relative to `drive_merge` (it currently writes header at `lib.rs:291` BEFORE the merge and the final-analysis + completion AFTER, `:319–321`), the bytes stay identical only if the caller preserves that exact sequence. A delegation that returns `Counters` and has the caller write the WHOLE report is fine, but the plan should say so explicitly.
- **STDERR cleanup/`eprintln!` ordering** is not gated (acknowledged §3.7), so not a byte risk — fine.

Action: before relying on "226 tests", confirm (and note in the plan) that at least one existing SE and one existing PE test asserts the **full `_SE_report.txt`/`_PE_report.txt` body modulo the wall-clock line**. If they only diff the BAM, add that assertion as part of Phase 9b's regression guard. Otherwise the delegation could silently change report bytes that no test catches and that the gate's wall-clock filter could mask if a reviewer over-filters.

### I-4 — §2.6 central assumption: the plan's "would fail loudly at the oxy gate, not silently" claim is correct ONLY because the gate compares against single-core; make that dependency explicit and add a *small-N cross-check* before the expensive oxy run
The per-read-independence assumption is sound for Bowtie 2 with a single thread per instance (no `-p`/`--reorder`), and Phase 0 already demonstrated run-to-run determinism. The failure mode if it were false (a read's alignment depending on its file-mates/ordinal) would manifest as `--parallel N` BAM ≠ `--parallel 1` BAM — which the gate DOES catch, loudly. Good.

But two sharpenings:
- The gate's loudness depends entirely on `--parallel 1` being a *true* single-core Bowtie 2 invocation (one thread per instance, no reorder flags). Confirm the per-chunk `AlignerStream::spawn` passes the SAME `aligner_options` as single-core (it does — `process_*_chunk` reuses `se_instance_plan`/`pe_instance_plan` + `config.aligner_options`), so a chunk's Bowtie 2 is invoked identically to single-core's, just on fewer reads. State this as the reason the assumption is *testable* by the gate.
- The cheap unit/integration cross-check in §9 #7 (fake bt2) does NOT exercise real Bowtie 2 seeding — the fake aligner is deterministic per converted-read by construction, so §9 #7 cannot disprove the assumption; only §9 #11 (real GRCh38 on oxy) can. The plan should say plainly that **§9 #7 validates the merge/split/counter machinery, and §9 #11 validates the Bowtie-2-independence assumption** — they are not redundant, and #11 is load-bearing, not a formality. Run #11 early (the plan's §11 risk-1 mitigation says "pin it early" — good; elevate that to a sequencing note).

### I-5 — Temp-naming under `--prefix`: re-verify no collision when prefix is set, and that the subset basename feeding the converter is the *subset's* name (not the original)
The collision-free claim holds for the no-prefix case. With `--prefix p`, the single-core converted name is `p.<basename>_C_to_T.fastq` (`convert.rs:264–266`). If the subset file is named `<basename>.temp.<chunk>` and the per-chunk converter is invoked with the SAME `ConvertOptions` (which carries `prefix`), the per-chunk converted name becomes `p.<basename>.temp.<chunk>_C_to_T.fastq` — still unique and still disjoint from the single-core name. That is fine, BUT note: `pbat` is incompatible with `--prefix`? No — `--prefix` is orthogonal; only `--gzip` and `-f` are pbat-incompatible (`config.rs:287–300`). So prefix+pbat+multicore is a live combination. Confirm the splitter names the subset off the original `basename` (so the chunk index is the only discriminator) and lets the converter prepend the prefix as usual — i.e. **do not** also prepend the prefix to the subset file name, or you'd get `p.p.<basename>…`.

Action: §3.2/§3.3 should state the subset file name is `<basename>.temp.<chunk>[.ext]` with NO prefix applied (the prefix lands only on the converted/output names via the existing helpers). Add a unit test with `--prefix` + N>1 asserting the final output name is unchanged vs single-core.

### I-6 — PE lockstep split: the R1==R2 per-chunk count assertion is good, but also assert the *global* R1==R2 count, and decide the mismatch-failure mode
`drive_merge_pe` already breaks on the first incomplete record (`lib.rs:1037–1049`), so single-core tolerates ragged mate files by truncating at the shorter. The contiguous splitter must reproduce that: if R1 has more records than R2 (or vice versa), the effective set is the min, and the chunk boundaries must be computed over the COMMON record count, not each file independently — otherwise a chunk could get a trailing R1 with no R2 mate and the per-chunk `drive_merge_pe` would pair across the boundary or truncate differently than single-core.

Action: §3.2/§3.8 should specify that PE splitting reads BOTH mates in lockstep and stops at the shorter (mirroring `drive_merge_pe`'s incomplete-break), partitions the COMMON count, and the per-chunk R1==R2 assertion is a sanity check on top of that (it should never fire if split lockstep is correct). Confirm the global truncation point equals single-core's.

---

## Optional

### O-1 — Empty-input / `eff < N`: confirm Bowtie 2 over an empty FastQ produces a clean header-only BAM (not a non-zero exit)
§3.2 step 3 and §3.8 assume Bowtie 2 on empty input is a no-op header-only BAM. `AlignerStream::finish` checks exit status (`align.rs:241–251`) and errors on non-zero. Verify (cheaply, in §9 #9) that Bowtie 2 2.5.5 exits 0 on an empty input file rather than erroring — if it errors, a trailing empty chunk would fail the whole run where single-core (no chunks) would not. This is the one edge case where the "empty chunk contributes nothing" claim could break loudly. The §9 #9 test as written ("empty input + eff < N → header-only BAM; no crash") covers it *if* it actually runs the empty chunk through the real spawn path, not just the splitter.

### O-2 — §9 #7 false-pass hardening: assert aux *raw AND decompressed*, report *modulo wall-clock only*, and a count NOT divisible by N for EVERY {2,4,8}
The plan flags the false-pass trap (its own §11 risk-2) and §9 #7 says "count NOT divisible by N (spans a chunk boundary)". Make this concrete to avoid a Phase-8/9a-style fake passing trivially:
- pick a read count `c` such that `c % N != 0` for each N in {2,4,8} simultaneously (e.g. `c = 13` or `c = 1000003`), so the boundary-straddle is exercised at every N with one fixture;
- assert BAM decompressed records, report (filter ONLY the single wall-clock line — not the whole footer), aux **both** decompressed content AND raw gz bytes (the raw assertion is the Q4 canary, I-2);
- include at least one chunk that ends up EMPTY at high N (so the empty-chunk merge path is on the byte-identity gate, not just the no-crash test).

### O-3 — `std::thread::scope` error precedence: spell out "join all, return first error" deterministically
The extractor went through a multi-step error-precedence design (`parallel.rs:332–376`). 9b is simpler (no producer/collector, just N scoped workers), but the plan should state the rule: collect all workers' `Result`s after scope join, and return the **lowest-chunk-index** error (deterministic stderr), having let all `Drop`s reap children. §9 #10 tests "first error returned; no orphan" — make "first" mean "lowest chunk index", not "first to arrive", for determinism. (Output bytes on an error path aren't gated, but a deterministic error message is friendlier and matches the extractor precedent.)

### O-4 — Memory-estimate warning (§3.7): the `4N × index-size` figure double-counts; clarify
The warning estimates `n × instances-per-chunk × index-file-size`. Note that the two SE directional instances read the SAME C→T file but DIFFERENT indexes (`BS_CT` and `BS_GA`, `se_instance_plan` `lib.rs:200`), and non-dir's four instances span both indexes. So "instances-per-chunk × index-size" is a reasonable upper bound, but the two indexes (`BS_CT`/`BS_GA`) are typically equal size, so `stat`-ing one and multiplying is fine. Minor: the message should say "peak resident is bounded by, not equal to" since the OS may share read-only index pages across instances of the same index. Purely a wording nicety (STDERR, not gated).

### O-5 — Re-base prerequisite (§2.1): correctly scoped as a non-implementation, Felix-gated step
No action — just confirming the plan correctly flags the `rust/aligner-v1` re-base/force-push-blocked dance as out-of-scope and Felix-gated, consistent with the 9a precedent and the MEMORY note. Good.

---

## Efficiency

- Genome loaded once, shared read-only across workers via `std::thread::scope` (no `Arc`) — correct and matches the extractor's shared-immutable pattern. ✓
- The 2-pass count-then-split (Q1) adds one extra decompress pass over the input; negligible vs alignment, as the plan says. The count pass must use the SAME record arity (4-line FastQ / 2-line FastA) and gz handling as the splitter to avoid a count/split mismatch — fold the count and split into one helper that reads once for counting then once for writing, or stream-count into a Vec of byte-offsets. (Implementation detail; no gate impact.)
- Up to 4N concurrent Bowtie 2 processes is inherent to the file-level model and matches Perl; no cap below N per Q5. Fine.

---

## Alternatives considered (and correctly rejected by the plan)

- Per-record reorder-buffer (extractor shape) — correctly rejected; the aligner's unit of work is a subprocess, not a record (§2.5 Insight). ✓
- Raw-BGZF block concat for BAM merge — correctly rejected in favor of noodles record-copy under one header to match the *decompressed-content* gate (Q3). One confirm at implementation: each per-chunk BAM carries the same `@HD`/`@SQ`/`@PG` (built from the shared header), so skipping per-chunk headers and writing one final header loses nothing — verify `merge_bams` writes the shared header ONCE and copies only records (the plan says so; §3.4.1). ✓
- Per-chunk gz + member concat for aux — correctly rejected (decompressed-identical only; fails raw gate). See I-2. ✓

---

## Summary of action items

| Rank | Item | One-line |
|------|------|----------|
| Critical | — | none |
| Important | I-1 | Pin exact skip/upto arithmetic (1-based raw ordinal, falsy-0, `(skip,upto]`, cite `convert.rs:307–327`); add skip+upto-together boundary-straddle unit test. |
| Important | I-2 | State aux-gz invariant: same `Compression::default()`, single `GzEncoder`, NO mid-stream flush; §9 #6 raw-byte assert is the canary. |
| Important | I-3 | Confirm the 226-test guard asserts the FULL report body (modulo wall-clock) for SE+PE; if not, add that assertion before relying on the N==1 delegation. |
| Important | I-4 | Make explicit that §9 #7 (fake bt2) validates machinery, §9 #11 (real oxy) validates the Bowtie-2-independence assumption — not redundant; run #11 early. |
| Important | I-5 | Subset file named off ORIGINAL basename with NO prefix; prefix lands only via existing converter/output helpers; add `--prefix`+N>1 output-name test. |
| Important | I-6 | PE split reads both mates in lockstep, partitions the COMMON (min) record count, truncating exactly as `drive_merge_pe` does; R1==R2 assert is a sanity net. |
| Optional | O-1 | Verify Bowtie 2 2.5.5 exits 0 on empty input (trailing empty chunk); §9 #9 must run the empty chunk through real spawn. |
| Optional | O-2 | Harden §9 #7: one count coprime-ish to {2,4,8}; assert aux raw AND decompressed, report modulo wall-clock only, include an empty high-N chunk on the gate. |
| Optional | O-3 | Deterministic error precedence: return lowest-chunk-index error after joining all scoped workers. |
| Optional | O-4 | Memory-warning wording: "bounded by, not equal to"; note OS page-sharing of read-only indexes. |
| Optional | O-5 | (No action) re-base prerequisite correctly scoped as Felix-gated. |

**Overall:** the plan is implementation-ready once I-1 through I-6 are folded in (mostly tightening the spec text + adding three targeted tests). The architecture is correct and the byte-identity story is sound; the residual risk is concentrated in skip/upto arithmetic (I-1), aux raw-byte determinism (I-2), and ensuring the validation suite cannot false-pass (I-4/O-2) — all pinnable before the oxy gate.
