# PLAN_REVIEW_A — `deduplicate_bismark_rs` graceful zero-alignment handling

**Reviewer:** A (independent)
**Plan:** `plans/06132026_dedup-empty-input/PLAN.md` (rev 0, 2026-06-13)
**Target crate:** `rust/bismark-dedup` (bin `deduplicate_bismark_rs`)
**Verdict:** **APPROVE-WITH-CHANGES** — 0 Critical, 4 Important, 5 Optional.

This is an unusually thorough plan. I verified every load-bearing source claim
against the actual code (pipeline.rs, report.rs, main.rs, error.rs, write.rs,
read.rs, dedup.rs, integration_dedup.rs, methylseq_conformance.rs). The core
design is correct and the line references are accurate. The Important items are
about *completeness of the surgery description* and *validation rigour*, not
about a flawed core approach.

---

## 1. Logic review

### 1.1 Core claim verified — empty stream flows to valid header-only output ✓
The end-to-end chain holds against source:
- `DedupState::new()` → `count: 0, removed: 0` (dedup.rs:125-126). `observe()` is
  the *only* mutator (dedup.rs:139-140) and is never called on an empty iterator,
  so `into_report` yields `count=0, removed=0, n_positions=0` (dedup.rs:174-178).
- `DedupReport::format()` renders `count == 0` as `0 (N/A%)` (report.rs:107-115),
  and that branch is exercised by `format_uses_na_when_count_is_zero`
  (report.rs:201-208). The plan's "dead in practice, activated by the fix" framing
  is correct.
- All four writer finalisers write a valid trailer regardless of record count:
  `BamWriter::finish` → `try_finish()` writes BGZF EOF (write.rs:98-101);
  `ThreadedBamWriter::finish` → `bgzf.finish()` (write.rs:179-184);
  `SamWriter::finish` flushes (no EOF marker — correct for SAM); `CramWriter::finish`
  writes the EOF container (write.rs:300-303). The header is written *eagerly* at
  writer construction (`BamWriter::new`, write.rs:60-63), so a header-only file is
  fully valid with zero `write_record` calls. **The "valid header-only output"
  claim is solid for BAM/SAM/CRAM and the threaded BAM path.**

### 1.2 `stream_pe` over empty iterator — no `UnpairedFinalRecord` ✓
Verified (pipeline.rs:257-263): the first `iter.next()` returns `None` → `break`
*before* the orphan-R1 arm (267-271) is reachable. `stream_pe_umi` is identical
(778-784). The plan's claim is correct.

### 1.3 The 8 entry points + guard line refs — all accurate ✓
I cross-checked every row of the plan's table against pipeline.rs. The peek-None
guards (run_single 314-316; run_single_parallel 488-489; run_single_umi 823-824;
run_single_parallel_umi 963-964) and the `first_record` peek-stash arms in the
4 `--multiple` variants (run_multiple 408-414; run_multiple_parallel 562-569;
run_multiple_umi 894-901; run_multiple_parallel_umi 1034-1041) all match. The
`inputs.is_empty()` guards (354, 523, 856, 997) are correctly identified as a
*different, retained* case. No surface was missed.

### 1.4 Exactly the two integration tests assert old error behavior ✓
Confirmed `empty_input_errors_before_any_output_file_is_created` (integration_dedup.rs:562)
and `multiple_mode_empty_file1_leaves_no_output_files_behind` (646). A repo-wide
grep for `EmptyInput|input file is empty|leaves_no_output|errors_before` across the
dedup crate (excluding pipeline.rs/error.rs) found **only** these two tests plus the
already-passing report.rs `count=0` unit test. **The plan did not miss a test.**

### 1.5 IMPORTANT-1 — the `--multiple` refactor is under-specified and the
proposed `readers.into_iter()` loop drops the per-file `refid_table` indexing
The plan §A.3 says: "Open the writer with `headers[0].clone()`, then iterate **all**
readers (not `readers_iter` after popping one) through `stream_*`." This is *almost*
right but glosses over a real constraint the current code encodes carefully: **each
reader i must be streamed with `refid_tables[i]`, not a single shared table**
(pipeline.rs:426/436 pass `refid_tables[0]` for file1 and `refid_tables[i]` for the
rest). A naive `readers.into_iter().enumerate()` works, but the implementer must
preserve the `(i, reader) → refid_tables[i]` pairing. Also note `headers` is built by
*borrowing* `readers.iter()` (pipeline.rs:375), and `refid_tables` borrows `headers`
(383-386) — both must be fully materialised *before* `readers` is consumed by
`into_iter()`. The current code already orders these correctly, but the plan's
one-line description ("simply streams every reader in order") would let an implementer
write a loop that either (a) reuses `refid_tables[0]` for all files, or (b) tries to
build `refid_tables` lazily inside the consuming loop after `headers` is gone.
**Action: spell out the exact post-refactor loop shape, including `refid_tables[i]`
indexing, for all four `--multiple` variants.** This is the single most likely place
to introduce a silent multi-file chr-mapping bug.

### 1.6 IMPORTANT-2 — the UMI `--multiple` variants wrap streaming in a closure +
`cleanup_partial_output_on_err`; the plan's "mirror run_multiple" is insufficient
`run_multiple_umi` (848) and `run_multiple_parallel_umi` (989) are NOT structurally
identical to `run_multiple`. They:
- Wrap all streaming in an inner `(|| { ... })()` closure so a single `?` controls
  early-exit (pipeline.rs:908-938 / 1047-1077).
- Combine `stream_result` + `finish_result` in a 3-arm match (839-843 / 940-944 /
  979-983 / 1079-1083).
- Run the whole thing through `cleanup_partial_output_on_err` (844 / 945 / 984 / 1085),
  which **unlinks the output file on error** (622-630).
The graceful path must integrate with this structure: after removing the peek-stash,
the closure should stream `first_reader.records_with_umi(...)` (no `iter::once` prepend
needed) then the rest. Critically, `cleanup_partial_output_on_err` must remain — it is
still wanted for the genuine mid-stream `UmiExtractionFailed` case — and the empty path
must NOT trip it (it won't, because empty → `Ok` → no cleanup). The plan's §A.4
("mirror the run_multiple change") does not mention the closure/cleanup machinery at
all. **Action: describe the UMI-variant surgery explicitly, confirming
`cleanup_partial_output_on_err` is retained and the empty path returns `Ok`.**

### 1.7 The single-file surgery is correctly described ✓
For `run_single` etc., "replace `.peekable()` + guard with a plain `reader.records()`
fed into `stream_*`" is exactly right. The `records` binding is already passed straight
into `stream_se`/`stream_pe` (pipeline.rs:325-327), so deleting lines 313-316 and
changing `let mut records = reader.records().peekable();` to `let records =
reader.records();` is a clean 2-line edit. Note `mut` is no longer needed once the
`.peek()` is gone — the implementer should drop it to avoid an `unused_mut` clippy
warning under `-D warnings` (V8 would catch this, but worth pre-empting).

### 1.8 `--multiple` count semantics for empty-file1 + non-empty-file2 ✓ (with a doc nit)
The plan (§ edge cases, V3) says "report `count` = records actually analysed across
all files (e.g. 1 pair)". Correct: `observe()` increments per analysed record/pair
regardless of which file it came from. The example "report `count = 1` pair" in §B.8
is right for one `ot_pair`. **Minor:** the report's first line echoes `file_label` =
`headers[0]`'s input path (`derive_output_paths`, main.rs:397 → `inputs[0]`), i.e. the
*empty* file1's name, while the counts reflect file2. This is faithful to Perl's
"file1 is the label" convention and is fine, but the plan should note the label/count
mismatch is intentional so a future reader doesn't "fix" it.

---

## 2. Assumptions

### 2.1 "Graceful is intentional, divergence from Perl is justified" — agreed ✓
The framing is correct and well-defended. The Perl oracle (exit 255 header-only via
`bam_isEmpty`; exit 29 all-unmapped) makes byte-identity impossible here, so the
divergence is forced by the goal, not a casual choice. It is correctly scoped to the
*zero-records* path only; all non-empty behavior is untouched (the perl-oracle CI gate
and the `compute_*_key`/report byte-identity tests are unaffected — I confirmed those
tests don't touch the empty path). This matches the established beta.3/beta.5
"methylseq drop-in" divergence class.

### 2.2 IMPORTANT-3 — Open Q-2 (`N/A%` vs `0.00%` in MultiQC) is the real residual
risk and the plan under-weights it
The plan calls this "the only residual risk to the end-to-end goal" (Open Q-2) but
then takes the assumption "keep `N/A%`, MultiQC is generally tolerant, confirm in V7."
The **entire point** of this change is "don't break methylseq." If MultiQC's Bismark
dedup module regex-parses the percentage as a float, `N/A` would fail to parse and
could *re-break the very pipeline this plan exists to unblock* — just one step later,
in the report-aggregation phase rather than the dedup phase. This deserves more than a
"probably tolerant" assumption:
- The Perl oracle never emits a `count=0` report at all (it dies / writes a 0-byte
  report), so there is **no Perl precedent** for what MultiQC sees here — the
  `N/A%` rendering is a *Rust-only invention* that has never been exercised against
  MultiQC in production.
- `0.00%` is unambiguously float-parseable and arguably *more correct* (0 of 0
  removed = 0%, not "not applicable"). The only cost is a 2-line change to the
  `count == 0` branch in report.rs:107-115 and updating two unit-test expectations
  (`format_uses_na_when_count_is_zero`).
**Action: do not defer this to V7 as a "confirm." Either (a) check the actual MultiQC
bismark dedup parser source before implementing and decide `N/A%` vs `0.00%`
deterministically, or (b) switch to `0.00%` as the safe default and note the divergence
from the (already-divergent) Rust `N/A%` convention.** Discovering `N/A%` breaks
MultiQC *after* shipping beta.6 + bumping the methylseq pin (plan §F.15) would be an
expensive round-trip. (Reviewer's lean: `0.00%` is safer and self-justifying.)

### 2.3 "Header is the input header verbatim, downstream-readable" ✓
Confirmed `run_*` clone `reader.header()` and pass it to the writer; the writer emits
it eagerly. A header-only BAM with the original `@HD`/`@SQ`/`@PG` is a standard,
samtools-readable file. Low risk.

### 2.4 All-unmapped FLAG-4 → filtered-to-empty ✓
Confirmed `filter_unmapped_then_classify` drops `flags & 0x4` records
(read.rs:637-638), so an all-unmapped BAM presents zero records and takes the same
graceful path; the output is header-only (unmapped records are NOT written — they were
dropped on read). The plan's "defensive only, Bismark never emits FLAG-4" caveat is
reasonable. One subtlety the plan states correctly: this *diverges* from Perl's
die-at-317 — fine per Felix, and it is the more robust behavior.

### 2.5 CRAM header-only output (Assumption 6) — leans on "covered defensively if cheap"
`CramWriter::finish` writes the EOF container (write.rs:300-303) and the header is
eager, so a header-only CRAM is structurally valid. The risk is low but the plan hedges
("rarely used in methylseq … covered defensively if cheap, else noted"). Since methylseq
uses BAM, this is genuinely low priority — see Optional-2.

---

## 3. Efficiency

Trivial and correct. Removing the `peek()`/`first_record` stash removes a tiny amount
of work; empty input becomes `O(1)` after header-clone + writer open/finish. No new
allocations on any path. Nothing to flag.

---

## 4. Validation sufficiency

V1-V9 cover the surface well, but there are gaps relative to the stated goal.

### 4.1 IMPORTANT-4 — V7 (the methylseq cascade) is the one gate that actually proves
the goal, yet it is described as "ideally a real run" rather than a hard gate
The plan's whole justification is "don't crash methylseq." V6 (unit/integration suite),
V8 (lint), V9 (conformance row) all prove *dedup* behaves — but the failure mode that
motivated this work is a *pipeline-level* crash, and the residual risk (2.2) lives
*downstream of dedup* (MultiQC parsing). V7 as written is soft ("ideally a real
methylseq run … or at least dedup→extract chain"). Given (a) the cascade finding that
the extractor is already graceful was done on a *plain* `-s` extraction, not the full
`--bedGraph --CX --cytosine_report --genome_folder` invocation methylseq actually uses
(the plan itself flags this caveat in §Context), and (b) the unverified MultiQC link,
**the full methylseq run on `bismark:2.0.0-beta.6` with a deliberately-no-alignment
sample should be a HARD gate before bumping the methylseq pin (§F.15), not an
"ideally."** At minimum, the full extractor invocation on a header-only BAM AND a
MultiQC pass over the resulting reports must both be exercised. This doesn't have to
block the *code merge*, but it must block the *release/pin bump*. **Action: split V7
into V7a (full extractor invocation on header-only BAM — hard gate for merge) and V7b
(end-to-end methylseq + MultiQC on a no-alignment sample — hard gate for the
beta.6 pin bump in §F).**

### 4.2 Test coverage gaps (Optional)
- The plan adds graceful tests for SE/PE single (V1/V2), `--parallel 2` and one UMI
  flag (V4/§C.9), all-unmapped (V5/§C.10), and `--multiple` all-empty (§C.11). Good.
- **Not covered:** `--multiple` empty-file1 + non-empty-file2 under `--parallel`
  (only the single-threaded variant is in §B.8). The `run_multiple_parallel` path has
  its own peek-stash (562-569) and a distinct `readers.drain(..)` consumption pattern
  (558) — a regression there wouldn't be caught. Cheap to add (Optional-3).
- **Not covered:** a header-only SAM (`.sam`) and a header-only CRAM. The plan
  validates BAM throughout. SAM has no EOF marker and CRAM has a different finaliser;
  if "matching input format" output is a real promise (Behavior §2), at least a SAM
  header-only smoke test is worth it (Optional-2).

### 4.3 The dropped Perl-oracle validation is correctly handled ✓
The plan explicitly drops "byte-identity on empty input" because Perl dies — correct,
and well-explained. The report-format oracle becomes the crate's own `count=0` unit
test, which is the right call.

---

## 5. Alternatives

### 5.1 Shared empty-input helper vs. editing 8 sites (Optional)
The plan edits 8 guard sites by hand. Since the single-file edit is "delete 4 lines"
and the `--multiple` edit is "remove the peek-stash block + restructure the loop," a
shared helper has limited leverage for the single-file cases. **However**, a small
helper for the `--multiple` "stream all readers with their refid_tables" loop —
extracted once and called by all four `--multiple` variants — would (a) eliminate the
IMPORTANT-1 refid_table-indexing footgun by encoding the pairing in one place, and
(b) reduce the 4-way copy-paste that the dual-driver-backport memory warns about
(independent drivers shipping the same infrastructure bug twice). Worth considering for
the `--multiple` family specifically; not worth it for the single-file family.

### 5.2 `0.00%` vs `N/A%` — see IMPORTANT-3. Reviewer leans `0.00%`.

### 5.3 Emit the info line from `main` vs `pipeline` (Open Q-1) — agree with the plan
Emitting once from `main::process_one`/`process_multiple` after a `count==0` report
(rather than duplicating across 8 `pipeline` functions) is the right call: less churn,
no risk of double-emission in the `--multiple` len==1 → `run_single` delegation path
(which would emit twice if done per-entry). **One caveat:** keying the info line on
`report.count() == 0` in `main` also fires for a genuinely-non-empty file that happens
to dedup to... no, `count` is *analysed* not *leftover*, so `count==0` ⟺ zero records
analysed ⟺ empty input. That's exactly the right trigger. Good. Recommend adopting the
plan's proposed `main`-side emission.

### 5.4 Replace `EmptyInput` for the `inputs.is_empty()` case with `NoInputFiles`
(Open Q-3) — agree with the plan's "keep EmptyInput, minimal churn." `NoInputFiles`
(error.rs:103) is already the upstream guard in `Cli::validate()`, so the
`inputs.is_empty()` arms are dead/defensive anyway; renaming them is pure churn. Keep
as-is, update the doc comment. Fine.

---

## 6. Action items (prioritized)

### Critical
*(none)*

### Important
1. **(I-1, §1.5)** Spell out the exact post-refactor `--multiple` streaming loop for
   all four variants, **explicitly preserving the `refid_tables[i]` per-file indexing**
   and the "materialise `headers`+`refid_tables` before consuming `readers`" ordering.
   This is the highest-risk silent-bug site (multi-file chr-mapping).
2. **(I-2, §1.6)** Describe the UMI `--multiple` surgery explicitly: it wraps streaming
   in a closure + 3-arm `(stream_result, finish_result)` match + `cleanup_partial_output_on_err`.
   Confirm the cleanup helper is **retained** (still needed for mid-stream
   `UmiExtractionFailed`) and that the empty path returns `Ok` so cleanup doesn't fire.
3. **(I-3, §2.2)** Resolve Open Q-2 deterministically *before* implementing: check the
   MultiQC bismark-dedup parser, or default to `0.00%` (safer, float-parseable). Do not
   defer a "MultiQC might choke on N/A%" risk to a soft post-hoc V7 check — it would
   re-break methylseq one step downstream after the pin bump.
4. **(I-4, §4.1)** Make the methylseq cascade a hard gate, split: V7a = full extractor
   invocation (`--bedGraph --CX --cytosine_report --genome_folder`) on a header-only BAM
   (hard gate for merge); V7b = end-to-end methylseq + MultiQC on a no-alignment sample
   (hard gate for the §F.15 beta.6 pin bump).

### Optional
1. **(O-1, §1.7)** Drop the now-unnecessary `mut` on the `records` binding in the
   single-file variants to avoid an `unused_mut` clippy failure under `-D warnings`.
2. **(O-2, §4.2)** Add a header-only **SAM** smoke test (no EOF marker path) — and a
   header-only **CRAM** test if cheap — to back the "matching input format" promise.
3. **(O-3, §4.2)** Add a `--multiple --parallel` empty-file1 test (the
   `run_multiple_parallel` `readers.drain(..)` + peek-stash path is distinct from the
   single-threaded one and otherwise uncovered for the graceful case).
4. **(O-4, §5.1)** Consider a single shared `--multiple` "stream-all-readers" helper to
   encode the refid_table pairing once and avoid 4-way copy-paste drift.
5. **(O-5, §1.8)** Note in the plan that the `--multiple` empty-file1 report's
   filename-label (file1) vs counts (file2) mismatch is intentional/faithful, so it
   isn't "fixed" later.

---

## Verdict

**APPROVE-WITH-CHANGES.** The core design is correct and every load-bearing source
claim checks out against the code: the empty stream flows cleanly to valid header-only
output across all writer types, `stream_pe` is safe on empty input, the `count=0`
report rendering already exists, and exactly the two named tests assert the old
behavior (no missed tests). The divergence from Perl is justified and correctly scoped.
The 4 Important items are about (1-2) under-specified surgery on the `--multiple` and
UMI variants where the refid_table indexing and cleanup-helper machinery could spawn a
silent bug, and (3-4) tightening the MultiQC/`N/A%` risk and the methylseq cascade from
soft assumptions into deterministic decisions / hard release gates. None block the
approach; all should be folded into a rev 1 before the implement trigger. **0 Critical,
4 Important, 5 Optional.**
